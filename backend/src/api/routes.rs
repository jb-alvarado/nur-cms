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
        fields::{AuthRoleFields, AuthUserFields, LocaleFields, Table},
        handles,
        models::{AuthRole, AuthUser, Locale, Role},
        queries::{QueryObj, RespondObj},
        serialize::AuthUserSerializer,
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

pub async fn auth_role_select(
    State(pool): State<PgPool>,
    Query(mut params): Query<QueryObj<AuthRoleFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<AuthRole>>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        params.path = original_uri.path().to_string();
        params.query = original_uri.query().unwrap_or("").to_string();

        return match handles::select_record(&pool, &Table::AuthRoles, params).await {
            Ok(role) => Ok(Json(role)),
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

pub async fn auth_user_select(
    State(pool): State<PgPool>,
    Query(mut params): Query<QueryObj<AuthUserFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<AuthUserSerializer>>, ServiceError> {
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

pub async fn auth_user_delete(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_record(&pool, &Table::AuthUsers, id).await {
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

pub async fn auth_user_insert(
    State(pool): State<PgPool>,
    details: AuthDetails<Role>,
    Json(auth_user): Json<AuthUser>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::insert_record(&pool, &Table::AuthUsers, &auth_user).await {
            Ok(id) => Ok(Json(id)),
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
        return match handles::update_record(&pool, &Table::AuthUsers, id, &auth_user).await {
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

/* ------------------------------
LOCALES
---------------------------------*/

pub async fn locale_select(
    State(pool): State<PgPool>,
    Query(mut params): Query<QueryObj<LocaleFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<Locale>>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        params.path = original_uri.path().to_string();
        params.query = original_uri.query().unwrap_or("").to_string();

        return match handles::select_record(&pool, &Table::Locales, params).await {
            Ok(locale) => Ok(Json(locale)),
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

pub async fn locale_delete(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_record(&pool, &Table::Locales, id).await {
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

pub async fn locale_insert(
    State(pool): State<PgPool>,
    details: AuthDetails<Role>,
    Json(locale): Json<Locale>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::insert_record(&pool, &Table::Locales, &locale).await {
            Ok(id) => Ok(Json(id)),
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

pub async fn locale_update(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(locale): Json<Locale>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::update_record(&pool, &Table::Locales, id, &locale).await {
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
