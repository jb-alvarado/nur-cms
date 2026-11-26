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
use jiff::SignedDuration;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use real::RealIp;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use tokio::sync::broadcast::Sender;
use tracing::error;
use uuid::Uuid;

use crate::db::Timestamp;
use crate::{
    AuthUserMeta, CMS_CONFIG, CONFIG,
    db::{
        fields::{CommentFields, Table},
        handles,
        models::{Comment, Role},
        queries::{QueryObj, RespondObj},
    },
    mail::{
        queue::{self, COMMENT_NOTIFICATION_TARGET},
        service::MailRequest,
    },
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

#[cfg(test)]
const COMMENT_MODERATION_TOKEN_TTL_DAYS: i64 = 14;

async fn enqueue_notification(
    connection: &mut PgConnection,
    comment_id: i64,
    comment: &Comment,
    public_url: Option<&str>,
) -> Result<(), NurError> {
    let author_name = comment.author_name.as_deref().unwrap_or_default();
    let author_email = comment.author_email.as_deref().unwrap_or_default();
    let comment_text = comment.text.as_deref().unwrap_or_default();
    let entry =
        handles::select_comment_entry(connection, comment.entry_id.ok_or(NurError::InvalidInput)?)
            .await?;

    let moderation_links = match public_url {
        Some(public_url) => {
            Some(create_moderation_links(connection, comment_id, public_url).await?)
        }
        None => None,
    };
    let entry_link =
        public_url.map(|public_url| entry_public_url(public_url, &entry.type_slug, &entry.slug));
    let comment_link =
        public_url.map(|public_url| format!("{public_url}/admin/comment/{comment_id}"));

    let entry_title: String = entry.title.chars().take(1_000).collect();
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

    let request = MailRequest {
        reply_to: author_email.to_owned(),
        name: author_name.to_owned(),
        subject: Some(format!("New Comment from: {author_name}")),
        text: message_body,
    };
    queue::enqueue(
        connection,
        &format!("comment:{comment_id}"),
        COMMENT_NOTIFICATION_TARGET,
        &request,
    )
    .await
    .map_err(|_| {
        error!(comment_id, "Failed to queue comment notification");
        NurError::InternalServerError
    })?;

    Ok(())
}

struct ModerationLinks {
    approve: String,
    reject: String,
}

async fn create_moderation_links(
    connection: &mut PgConnection,
    comment_id: i64,
    public_url: &str,
) -> Result<ModerationLinks, NurError> {
    let expires_at =
        Timestamp::now() + SignedDuration::from_hours(24 * comment_moderation_token_ttl_days());
    let approve_token = Uuid::new_v4().simple().to_string();
    let reject_token = Uuid::new_v4().simple().to_string();
    handles::insert_comment_moderation_tokens(
        connection,
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
    crate::config::settings()
        .comments
        .moderation_token_lifetime_days
}

#[cfg(test)]
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

    let notify = if content.author_email.is_some()
        && content.author_name.is_some()
        && !details.has_any_authority(&[&Role::Admin, &Role::Author])
    {
        let config = CONFIG.read().await;
        config.mail_smtp.is_some()
            && config
                .notification_emails
                .as_ref()
                .is_some_and(|recipients| !recipients.is_empty())
    } else {
        false
    };
    let mut transaction = pool.begin().await?;
    let id = handles::insert_comment(&mut transaction, &content).await?;
    if notify {
        enqueue_notification(
            &mut transaction,
            id,
            &content,
            configured_public_url().as_deref(),
        )
        .await?;
    }
    transaction.commit().await?;

    let msg = SSEMessage::new(SSELevel::Success, &format!("New Comment received: {id}"));
    let _ = tx.send(msg.to_string());

    Ok(Json(id))
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
        content["updated_at"] = Value::String(Timestamp::now().to_string());

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

    #[sqlx::test]
    async fn comment_notification_is_committed_with_the_comment(pool: sqlx::PgPool) {
        let entry_id: i32 = sqlx::query_scalar(
            "INSERT INTO content_entries (type_id, locale_id, slug, title, status)
             VALUES ((SELECT id FROM content_types WHERE slug = 'article'),
                     (SELECT id FROM locales WHERE code = 'en'),
                     'queued-comment-test', 'Queue test', 'published')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let comment = super::Comment {
            entry_id: Some(entry_id),
            author_name: Some("Test author".into()),
            author_email: Some("author@example.org".into()),
            text: Some("<&".repeat(10_000)),
            ..Default::default()
        };

        let mut tx = pool.begin().await.unwrap();
        let id = super::handles::insert_comment(&mut tx, &comment)
            .await
            .unwrap();
        super::enqueue_notification(&mut tx, id, &comment, Some("https://example.org"))
            .await
            .unwrap();
        tx.rollback().await.unwrap();

        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM comments), (SELECT count(*) FROM public.mail_queue),
                    (SELECT count(*) FROM comment_moderation_tokens)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (0, 0, 0));

        let mut tx = pool.begin().await.unwrap();
        let id = super::handles::insert_comment(&mut tx, &comment)
            .await
            .unwrap();
        super::enqueue_notification(&mut tx, id, &comment, Some("https://example.org"))
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let (state, body): (String, String) = sqlx::query_as(
            "SELECT state, body FROM public.mail_queue WHERE source = 'cms' AND job_key = $1",
        )
        .bind(format!("comment:{id}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, "pending");
        assert!(body.contains("&lt;&amp;"));
        assert!(body.len() > 65_536);
        assert!(body.contains("https://example.org/api/comments/moderate/"));
        let token_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM comment_moderation_tokens WHERE comment_id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(token_count, 2);
    }

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
