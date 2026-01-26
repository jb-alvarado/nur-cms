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
    fields::{ContentTypeFields, Table},
    handles,
    models::{ContentType, Role},
    queries::{QueryObj, RespondObj},
};
use crate::utils::errors::NurError;

pub async fn types_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentTypeFields>>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<ContentType>>, NurError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    match handles::select_record(&pool, &Table::ContentTypes, params).await {
        Ok(types) => Ok(Json(types)),
        Err(e) => {
            error!("{e}");
            Err(NurError::InternalServerError)
        }
    }
}

pub async fn type_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<serde_json::Value>,
) -> Result<Json<i32>, NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::insert_record(&pool, &Table::ContentTypes, &content).await {
            Ok(id) => Ok(Json(id)),
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

pub async fn type_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_record(&pool, &Table::ContentTypes, id).await {
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

pub async fn type_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::update_record(&pool, &Table::ContentTypes, id, &content).await {
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
