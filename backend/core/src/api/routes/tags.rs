use axum::{
    Json,
    extract::{OriginalUri, Path, Query, State},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::db::{
    fields::{ContentTagFields, Table},
    handles,
    models::Role,
    queries::{QueryObj, RespondObj},
    serialize::*,
};
use crate::utils::errors::NurError;

pub async fn tags_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentTagFields>>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<ContentTagSerializer>>, NurError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    match handles::select_record(&pool, &Table::ContentTags, params).await {
        Ok(tags) => Ok(Json(tags)),
        Err(e) => {
            error!("{e}");
            Err(NurError::InternalServerError)
        }
    }
}

pub async fn tag_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<Json<i32>, NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::insert_record(&pool, &Table::ContentTags, &content).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("Insert {e}");
                let mut err = e.to_string();

                if err.contains("duplicate key") && err.contains("slug") {
                    err = "Duplicate slug, create a unique one!".into();
                }
                Err(NurError::Conflict(err))
            }
        };
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn tag_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::update_record(&pool, &Table::ContentTags, id, &content).await {
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

pub async fn entry_tag_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path((e_id, t_id)): Path<(i32, i32)>,
    details: AuthDetails<Role>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::delete_tag_from_entry(&pool, e_id, t_id).await {
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

pub async fn entry_tag_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<Json<i32>, NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::insert_record(&pool, &Table::ContentEntryTags, &content).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("Insert {e}");
                let mut err = e.to_string();

                if err.contains("duplicate key") && err.contains("slug") {
                    err = "Duplicate slug, create a unique one!".into();
                }
                Err(NurError::Conflict(err))
            }
        };
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}
