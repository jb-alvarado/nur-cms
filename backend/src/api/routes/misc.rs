use axum::{Json, extract::State};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::db::{
    handles,
    models::{Role, TSConfig},
    queries::RespondObj,
};
use crate::utils::errors::ServiceError;

pub async fn ts_language_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<TSConfig>>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::select_ts_language(&pool).await {
            Ok(lang) => Ok(Json(lang)),
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
