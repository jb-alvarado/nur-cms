use axum::{
    Json,
    extract::{OriginalUri, Path, Query, State},
    http::StatusCode,
};
use chrono::Utc;
// use axum_macros::debug_handler;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sqlx::postgres::PgPool;
use tracing::error;

use crate::{
    db::{
        fields::AuthUserFields,
        handles,
        models::{AuthUser, Role},
        queries::{QueryObj, RespondObj},
    },
    utils::errors::ServiceError,
};

pub async fn welcome(details: AuthDetails<Role>) -> Result<String, (StatusCode, String)> {
    if details.has_authority(&Role::Admin) {
        return Ok("Hello Admin!".to_string());
    } else if details.has_authority(&Role::Author) {
        return Ok("Hello Author!".to_string());
    } else if details.has_authority(&Role::User) {
        return Ok("Hello User!".to_string());
    }

    Ok("Hello Guest!".to_string())
}

pub async fn auth_user_select(
    State(pool): State<PgPool>,
    Query(mut params): Query<QueryObj<AuthUserFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<AuthUser>>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        params.path = original_uri.path().to_string();
        params.query = original_uri.query().unwrap_or("").to_string();

        return match handles::select_auth_user(&pool, params).await {
            Ok(user) => Ok(Json(user)),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn auth_user_update(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(auth_user): Json<AuthUser>,
) -> Result<(), ServiceError> {
    let mut auth_user: AuthUser = auth_user;
    auth_user.updated_at = Some(Utc::now());

    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::update_record(&pool, "auth_users", id, &auth_user).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}
