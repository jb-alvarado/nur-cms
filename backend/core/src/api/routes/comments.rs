use axum::{
    Extension, Json,
    extract::{OriginalUri, Path, Query, Request, State},
    http::{
        StatusCode,
        header::{CACHE_CONTROL, CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_FRAME_OPTIONS},
    },
    middleware::Next,
    response::{Html, IntoResponse, Response},
};
use chrono::{Duration, Utc};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use real::RealIp;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;
use uuid::Uuid;

use crate::{
    AuthUserMeta, CMS_CONFIG, CONFIG,
    db::{
        fields::{CommentFields, Table},
        handles,
        models::{Comment, MailTarget, Role},
        queries::{QueryObj, RespondObj},
    },
    mail::client::{Msg, message},
    sse::{SSELevel, SSEMessage},
    utils::{
        errors::NurError,
        public_url::configured_public_url,
        spam_detection::{evaluate_text, validate_email_address},
    },
};

pub async fn comments_feature_guard(request: Request, next: Next) -> Response {
    let disabled = {
        let configuration = CMS_CONFIG.read().await;
        feature_disabled(&configuration, "comments")
    };

    if disabled {
        return NurError::NotFound.into_response();
    }

    next.run(request).await
}

fn feature_disabled(configuration: &crate::db::models::CmsConfiguration, feature: &str) -> bool {
    configuration
        .disabled_features
        .iter()
        .any(|disabled| disabled == feature)
}

const COMMENT_MODERATION_TOKEN_TTL_DAYS: i64 = 14;

async fn notify(pool: &PgPool, comment_id: i64, comment: &Comment) -> Result<(), NurError> {
    let author_name = comment.author_name.as_deref().unwrap_or_default();
    let author_email = comment.author_email.as_deref().unwrap_or_default();
    let comment_text = comment.text.as_deref().unwrap_or_default();
    let entry = sqlx::query(
        "SELECT e.title, e.slug, t.slug AS type_slug
         FROM content_entries e
         INNER JOIN content_types t ON t.id = e.type_id
         WHERE e.id = $1",
    )
    .bind(comment.entry_id.ok_or(NurError::InvalidInput)?)
    .fetch_one(pool)
    .await?;
    let entry_title: String = sqlx::Row::try_get(&entry, "title")?;
    let entry_slug: String = sqlx::Row::try_get(&entry, "slug")?;
    let entry_type: String = sqlx::Row::try_get(&entry, "type_slug")?;

    let public_url = configured_public_url();
    let moderation_links = match public_url.as_deref() {
        Some(public_url) => Some(create_moderation_links(pool, comment_id, public_url).await?),
        None => None,
    };
    let entry_link = public_url
        .as_deref()
        .map(|public_url| entry_public_url(public_url, &entry_type, &entry_slug));
    let comment_link = public_url
        .as_deref()
        .map(|public_url| format!("{public_url}/admin/comment/{comment_id}"));

    let message_body = format!(
        "<h2>New comment awaiting moderation</h2>\
         <p><strong>Entry:</strong> {}</p>\
         <p><strong>Name:</strong> {}<br><strong>Email:</strong> {}</p>\
         <hr><p>{}</p>{}",
        escape_html(&entry_title),
        escape_html(author_name),
        escape_html(author_email),
        escape_html(comment_text).replace('\n', "<br>"),
        notification_links(entry_link, comment_link, moderation_links),
    );

    let target = MailTarget {
        id: 0,
        name: "New Comment".to_string(),
        subject: Some(format!("New Comment from: {author_name}")),
        recipients: CONFIG
            .read()
            .await
            .notification_emails
            .clone()
            .unwrap_or_default(),
        allow_html: true,
        allow_dynamic_recipient: false,
        total_count: None,
    };

    let msg = Msg::new(
        author_email.to_string(),
        author_name.to_string(),
        None,
        message_body,
        target,
    );

    message(msg).await?;

    Ok(())
}

struct ModerationLinks {
    approve: String,
    reject: String,
}

async fn create_moderation_links(
    pool: &PgPool,
    comment_id: i64,
    public_url: &str,
) -> Result<ModerationLinks, NurError> {
    let expires_at = Utc::now() + Duration::days(comment_moderation_token_ttl_days());
    let approve_token = Uuid::new_v4().simple().to_string();
    let reject_token = Uuid::new_v4().simple().to_string();
    handles::insert_comment_moderation_tokens(
        pool,
        comment_id,
        &hash_moderation_token(&approve_token),
        &hash_moderation_token(&reject_token),
        expires_at,
    )
    .await?;

    Ok(ModerationLinks {
        approve: format!("{public_url}/api/comments/moderate/{approve_token}"),
        reject: format!("{public_url}/api/comments/moderate/{reject_token}"),
    })
}

fn notification_links(
    entry_link: Option<String>,
    comment_link: Option<String>,
    moderation_links: Option<ModerationLinks>,
) -> String {
    let mut links = String::from("<hr><p>");
    if let Some(link) = entry_link {
        links.push_str(&format!(r#"<a href="{link}">Open article</a><br>"#));
    }
    if let Some(link) = comment_link {
        links.push_str(&format!(
            r#"<a href="{link}">Review comment in admin</a><br>"#
        ));
    }
    if let Some(links_for_action) = moderation_links {
        links.push_str(&format!(
            r#"<a href="{}">Approve comment</a> · <a href="{}">Reject comment</a>"#,
            links_for_action.approve, links_for_action.reject
        ));
    }
    links.push_str("</p>");
    links
}

fn entry_public_url(public_url: &str, entry_type: &str, entry_slug: &str) -> String {
    format!("{public_url}/{entry_type}/{entry_slug}")
}

fn comment_moderation_token_ttl_days() -> i64 {
    let value = std::env::var("NUR_COMMENT_MODERATION_TOKEN_TTL_DAYS").ok();
    configured_comment_moderation_token_ttl_days(value.as_deref())
}

fn configured_comment_moderation_token_ttl_days(value: Option<&str>) -> i64 {
    value
        .and_then(|value| value.parse().ok())
        .filter(|value| (1..=30).contains(value))
        .unwrap_or(COMMENT_MODERATION_TOKEN_TTL_DAYS)
}

fn hash_moderation_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

pub async fn comments_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<CommentFields>>,
    details: AuthDetails<Role>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<Comment>>, NurError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        if params.search_slug.is_none() {
            return Err(NurError::Forbidden(
                "You do not have permission to access this resource.".into(),
            ));
        }

        params.ordering = "-created_at".to_string();
        params.search_status = Some("approved".to_string());

        params.fields.retain(|f| {
            [
                CommentFields::ID,
                CommentFields::AuthorName,
                CommentFields::Text,
                CommentFields::CreatedAt,
                CommentFields::ParentID,
            ]
            .contains(f)
        });
    }

    match handles::select_comments(&pool, &params).await {
        Ok(categories) => Ok(Json(categories)),
        Err(e) => {
            error!("{e}");
            Err(NurError::InternalServerError)
        }
    }
}

pub async fn comment_insert(
    real_ip: RealIp,
    State((pool, tx)): State<(PgPool, Sender<String>)>,
    Extension(user): Extension<AuthUserMeta>,
    details: AuthDetails<Role>,
    Json(mut content): Json<Comment>,
) -> Result<Json<i64>, NurError> {
    if content
        .text
        .as_ref()
        .is_none_or(|text| text.trim().is_empty() || text.chars().count() > 20_000)
        || content
            .author_name
            .as_ref()
            .is_some_and(|name| name.chars().count() > 160)
    {
        return Err(NurError::BadRequest("Invalid comment.".into()));
    }

    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        content.user_id = Some(user.id);
    } else if details.has_any_authority(&[&Role::User]) {
        content.user_id = Some(user.id);
        content.status = Some("pending".to_string());
    } else {
        // require both name and email and ensure they're not empty strings
        if content.author_name.as_ref().is_none_or(String::is_empty)
            || content.author_email.as_ref().is_none_or(String::is_empty)
        {
            return Err(NurError::Conflict(
                "Name and email are required.".to_string(),
            ));
        }

        content.author_email = Some(validate_email_address(content.author_email.unwrap()).await?);
        content.status = Some("pending".to_string());

        let result = evaluate_text(content.text.as_deref().unwrap_or(""), None);

        if !result.passed {
            error!(
                "Spam detected from: {:?}, score: {:?}",
                real_ip.ip(),
                result.score
            );
            return Err(NurError::Conflict(
                "This message is not allowed!".to_string(),
            ));
        }
    }

    match handles::insert_comment(&pool, &content).await {
        Ok(id) => {
            let msg = SSEMessage::new(SSELevel::Success, &format!("New Comment received: {id}"));
            let _ = tx.send(msg.to_string());

            if content.author_email.is_some()
                && content.author_name.is_some()
                && CONFIG.read().await.mail_smtp.is_some()
                && !details.has_any_authority(&[&Role::Admin, &Role::Author])
                && let Err(error) = notify(&pool, id, &content).await
            {
                error!(%error, comment_id = id, "failed to send comment notification");
            }

            Ok(Json(id))
        }
        Err(e) => {
            error!("Insert Comment {e}");

            Err(NurError::InternalServerError)
        }
    }
}

pub async fn comment_moderation_confirm(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(token): Path<String>,
) -> Response {
    let Some(token_hash) = valid_moderation_token_hash(&token) else {
        return moderation_response(StatusCode::NOT_FOUND, "This moderation link is invalid.");
    };

    let action = match handles::select_comment_moderation_action(&pool, &token_hash).await {
        Ok(Some(action)) => action,
        Ok(None) => {
            return moderation_response(
                StatusCode::GONE,
                "This moderation link has expired, was already used, or the comment was already moderated.",
            );
        }
        Err(error) => {
            error!(%error, "failed to validate comment moderation token");
            return moderation_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The moderation link could not be validated.",
            );
        }
    };
    let action_label = match action.as_str() {
        "approved" => "Approve comment",
        "rejected" => "Reject comment",
        _ => {
            return moderation_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The moderation link contains an invalid action.",
            );
        }
    };

    moderation_response(
        StatusCode::OK,
        &format!(
            "<h1>{action_label}</h1>\
             <p>Confirm this moderation action.</p>\
             <form method=\"post\"><button type=\"submit\">{action_label}</button></form>"
        ),
    )
}

pub async fn comment_moderation_apply(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(token): Path<String>,
) -> Response {
    let Some(token_hash) = valid_moderation_token_hash(&token) else {
        return moderation_response(StatusCode::NOT_FOUND, "This moderation link is invalid.");
    };

    match handles::consume_comment_moderation_token(&pool, &token_hash).await {
        Ok(Some(action)) => moderation_response(
            StatusCode::OK,
            &format!("<h1>Comment {action}</h1><p>The moderation action was applied.</p>"),
        ),
        Ok(None) => moderation_response(
            StatusCode::GONE,
            "This moderation link has expired, was already used, or the comment was already moderated.",
        ),
        Err(error) => {
            error!(%error, "failed to apply comment moderation token");
            moderation_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The moderation action could not be applied.",
            )
        }
    }
}

fn moderation_response(status: StatusCode, body: &str) -> Response {
    (
        status,
        [
            (REFERRER_POLICY, "no-referrer"),
            (CACHE_CONTROL, "no-store"),
            (X_FRAME_OPTIONS, "DENY"),
            (
                CONTENT_SECURITY_POLICY,
                "default-src 'none'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
            ),
        ],
        Html(format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Comment moderation</title></head><body>{body}</body></html>"
        )),
    )
        .into_response()
}

fn valid_moderation_token_hash(token: &str) -> Option<Vec<u8>> {
    Uuid::parse_str(token).ok()?;
    Some(hash_moderation_token(token))
}

pub async fn comment_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(mut content): Json<Value>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        content["updated_at"] = Value::String(Utc::now().to_rfc3339());

        return match handles::update_record(&pool, &Table::Comments, id, &content).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(NurError::InternalServerError)
            }
        };
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn comment_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::delete_record(&pool, &Table::Comments, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(NurError::InternalServerError)
            }
        };
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use crate::db::models::CmsConfiguration;

    use super::{
        configured_comment_moderation_token_ttl_days, entry_public_url, escape_html,
        feature_disabled, hash_moderation_token, valid_moderation_token_hash,
    };

    #[test]
    fn comments_are_enabled_by_default_and_can_be_disabled() {
        let mut configuration = CmsConfiguration::default();
        assert!(!feature_disabled(&configuration, "comments"));

        configuration.disabled_features.push("comments".into());
        assert!(feature_disabled(&configuration, "comments"));
    }

    #[test]
    fn moderation_tokens_are_hashed_without_preserving_the_token() {
        let token = "d7eb0f8f936a4f1884d50f26ccd9bf55";
        let hash = hash_moderation_token(token);

        assert_ne!(hash, token.as_bytes());
        assert_eq!(hash, hash_moderation_token(token));
    }

    #[test]
    fn notification_values_are_html_escaped() {
        assert_eq!(
            escape_html("<commenter@example.org> & \"name\""),
            "&lt;commenter@example.org&gt; &amp; &quot;name&quot;"
        );
    }

    #[test]
    fn builds_public_article_url_from_type_and_entry_slug() {
        assert_eq!(
            entry_public_url("https://www.example.org", "news", "example-article",),
            "https://www.example.org/news/example-article"
        );
    }

    #[test]
    fn invalid_moderation_token_is_rejected() {
        assert!(valid_moderation_token_hash("not-a-token").is_none());
    }

    #[test]
    fn moderation_token_ttl_is_configured_in_days() {
        assert_eq!(configured_comment_moderation_token_ttl_days(Some("7")), 7);
        assert_eq!(configured_comment_moderation_token_ttl_days(None), 14);
        assert_eq!(configured_comment_moderation_token_ttl_days(Some("0")), 14);
        assert_eq!(configured_comment_moderation_token_ttl_days(Some("31")), 14);
        assert_eq!(
            configured_comment_moderation_token_ttl_days(Some("invalid")),
            14
        );
    }
}
