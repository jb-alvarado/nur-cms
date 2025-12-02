use axum::{
    Extension, Json,
    extract::{OriginalUri, Path, Query, State},
};
use chrono::Utc;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::db::{
    fields::{CommentFields, Table},
    handles,
    models::{Comment, Role},
    queries::{QueryObj, RespondObj},
};
use crate::{
    AuthUserMeta,
    sse::{SSELevel, SSEMessage},
    utils::errors::ServiceError,
};

pub async fn comments_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<CommentFields>>,
    details: AuthDetails<Role>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<Comment>>, ServiceError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        params.search_status = Some("approved".to_string());
    }

    match handles::select_comments(&pool, &params).await {
        Ok(categories) => Ok(Json(categories)),
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    }
}

pub async fn comment_insert(
    State((pool, tx)): State<(PgPool, Sender<String>)>,
    Extension(user): Extension<AuthUserMeta>,
    details: AuthDetails<Role>,
    Json(mut content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author, &Role::User]) {
        content["user_id"] = Value::Number(user.id.into());
        content["status"] = Value::String("approved".to_string());
    } else {
        // require both name and email and ensure they're not empty strings
        if content
            .get("author_name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
            || content
                .get("author_email")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        {
            return Err(ServiceError::Conflict(
                "Name and email are required.".to_string(),
            ));
        }

        content["status"] = Value::String("pending".to_string());
    }

    match handles::insert_record(&pool, &Table::Comments, &content).await {
        Ok(id) => {
            let msg = SSEMessage::new(SSELevel::Success, &format!("New Comment received: {id}"));
            let _ = tx.send(msg.to_string());
            Ok(Json(id))
        }
        Err(e) => {
            error!("Insert {e}");

            Err(ServiceError::InternalServerError)
        }
    }
}

pub async fn comment_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(mut content): Json<Value>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        content["updated_at"] = Value::String(Utc::now().to_rfc3339());

        return match handles::update_record(&pool, &Table::Comments, id, &content).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn comment_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::delete_record(&pool, &Table::Comments, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}
