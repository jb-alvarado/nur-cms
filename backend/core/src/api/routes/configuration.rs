use axum::{
    Json,
    extract::{OriginalUri, Query, State},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::{
    CONFIG,
    db::{
        fields::{ConfigurationFields, Table},
        handles,
        models::{Configuration, Role},
        queries::QueryObj,
    },
    utils::errors::NurError,
};

pub async fn config_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ConfigurationFields>>,
    details: AuthDetails<Role>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<Configuration>, NurError> {
    if !details.has_any_authority(&[&Role::Admin]) {
        return Err(NurError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    match handles::select_record(&pool, &Table::Configuration, params).await {
        Ok(types) => Ok(Json(types.results.first().cloned().unwrap_or_default())),
        Err(e) => {
            error!("{e}");
            Err(NurError::InternalServerError)
        }
    }
}

pub async fn config_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::update_record(&pool, &Table::Configuration, 1, &content).await {
            Ok(_) => {
                {
                    let config = handles::select_configuration(&pool).await?;
                    let mut cfg = CONFIG.write().await;
                    *cfg = config;
                }

                Ok(())
            }
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
