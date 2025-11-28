use std::str::FromStr;

use axum::{
    Json,
    extract::{OriginalUri, Path, Query, State},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sqlx::postgres::PgPool;
use strum::IntoEnumIterator;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::db::{
    fields::{ContentTypeFields, Table},
    handles,
    models::{ContentType, Role},
    queries::{QueryObj, RespondObj},
};
use crate::utils::errors::ServiceError;

pub async fn content_types_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentTypeFields>>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<ContentType>>, ServiceError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    match handles::select_record(&pool, &Table::ContentTypes, params).await {
        Ok(types) => Ok(Json(types)),
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    }
}

pub async fn content_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path((table, id)): Path<(String, i32)>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    let table = Table::from_str(&format!("content_{table}"))?;

    if (table == Table::ContentTypes && details.has_any_authority(&[&Role::Admin]))
        || (table != Table::ContentTypes
            && Table::iter().any(|t| t == table)
            && details.has_any_authority(&[&Role::Admin, &Role::Author]))
    {
        return match handles::delete_record(&pool, &table, id).await {
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

pub async fn content_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(table): Path<String>,
    details: AuthDetails<Role>,
    Json(content): Json<serde_json::Value>,
) -> Result<Json<i32>, ServiceError> {
    let table = Table::from_str(&format!("content_{table}"))?;

    if (table == Table::ContentTypes && details.has_any_authority(&[&Role::Admin]))
        || (table != Table::ContentTypes
            && Table::iter().any(|t| t == table)
            && details.has_any_authority(&[&Role::Admin, &Role::Author]))
    {
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
