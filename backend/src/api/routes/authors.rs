use axum::{
    Json,
    extract::{OriginalUri, Path, Query, State},
};
use chrono::Utc;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::db::{
    fields::{ContentAuthorFields, Table},
    handles,
    models::Role,
    queries::{QueryObj, RespondObj},
    serialize::*,
};
use crate::utils::errors::ServiceError;

pub async fn authors_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentAuthorFields>>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<AuthorSerializer>>, ServiceError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    return match handles::select_content_author(&pool, params).await {
        Ok(authors) => Ok(Json(authors)),
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    };
}

pub async fn author_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    let table = Table::ContentAuthors;

    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::insert_record(&pool, &table, &content).await {
            Ok(id) => Ok(Json(id)),
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

pub async fn author_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(mut content): Json<Value>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        content["updated_at"] = Value::String(Utc::now().to_rfc3339());

        return match handles::update_record(&pool, &Table::ContentAuthors, id, &content).await {
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

pub async fn author_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::delete_record(&pool, &Table::ContentAuthors, id).await {
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

pub async fn entry_author_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path((e_id, a_id)): Path<(i32, i32)>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::delete_author_from_entry(&pool, e_id, a_id).await {
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

pub async fn entry_author_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::insert_record(&pool, &Table::ContentEntryAuthors, &content).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("Insert {e}");
                let mut err = e.to_string();

                if err.contains("duplicate key") && err.contains("slug") {
                    err = "Duplicate slug, create a unique one!".into();
                }
                Err(ServiceError::Conflict(err))
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}
