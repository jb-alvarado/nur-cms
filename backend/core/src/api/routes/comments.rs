use axum::{
    Extension, Json,
    extract::{OriginalUri, Path, Query, State},
};
use chrono::Utc;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use real::RealIp;
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::{
    AuthUserMeta,
    sse::{SSELevel, SSEMessage},
    utils::errors::NurError,
};
use crate::{
    CONFIG,
    db::{
        fields::{CommentFields, Table},
        handles,
        models::{Comment, MailTarget, Role},
        queries::{QueryObj, RespondObj},
    },
    mail::client::{Msg, message},
    utils::spam_detection::{evaluate_text, validate_email_address},
};

async fn notify(comment: Comment) -> Result<(), NurError> {
    let author_name = comment.author_name.unwrap_or_default();
    let author_email = comment.author_email.unwrap_or_default();
    let comment_text = comment.text.unwrap_or_default();

    // Format notification message
    let message_body = format!(
        "Name: {}\nEmail: {}\n------------------------------------\n\n{}",
        author_name, author_email, comment_text
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
        total_count: None,
    };

    let msg = Msg::new(author_email, author_name, None, message_body, target);

    message(msg).await?;

    Ok(())
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

        params.ordering = "-creating_at".to_string();
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
    if details.has_any_authority(&[&Role::Admin, &Role::Author, &Role::User]) {
        content.user_id = Some(user.id);
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
            {
                notify(content).await?;
            }

            Ok(Json(id))
        }
        Err(e) => {
            error!("Insert Comment {e}");

            Err(NurError::InternalServerError)
        }
    }
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
