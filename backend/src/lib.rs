use std::collections::HashSet;
use std::str::FromStr;
use std::{env, sync::LazyLock};

use axum::response::IntoResponse;
use axum::{
    Router,
    extract::Request,
    response::Response,
    routing::{delete, get, post},
};
use protect_endpoints_core::tower::middleware::GrantsLayer;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::error;

pub mod api;
pub mod db;
pub mod utils;

use crate::{
    api::{
        auth::{decode_jwt, login, refresh},
        routes::*,
    },
    db::{
        fields::OutputType,
        handles,
        models::{AuthUserMeta, Role},
    },
    utils::errors::ServiceError,
};

pub static ACCESS_LIFETIME: LazyLock<i64> = LazyLock::new(|| {
    env::var("ACCESS_LIFETIME")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(3)
});
pub static REFRESH_LIFETIME: LazyLock<i64> = LazyLock::new(|| {
    env::var("REFRESH_LIFETIME")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30)
});
pub static JWT_SECRET: LazyLock<String> =
    LazyLock::new(|| env::var("JWT_SECRET").expect("JWT_SECRET must be set"));

pub static OUTPUT_TYPE: LazyLock<OutputType> = LazyLock::new(|| {
    env::var("OUTPUT_TYPE")
        .ok()
        .and_then(|v| OutputType::from_str(&v.to_ascii_lowercase()).ok())
        .unwrap_or(OutputType::AST)
});

pub async fn init_db() -> Result<PgPool, ServiceError> {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let max_connections = env::var("MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(50);

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await?;

    handles::db_migrate(&pool).await?;

    Ok(pool)
}

pub async fn extract(req: &mut Request) -> Result<HashSet<Role>, Response> {
    let mut authorities = HashSet::new();

    let Some(auth) = req.headers().get("authorization") else {
        authorities.insert(Role::Guest);
        return Ok(authorities);
    };

    let Some((scheme, token)) = auth.to_str().ok().and_then(|s| s.trim().split_once(' ')) else {
        error!("Malformed or invalid authorization header");
        return Err(ServiceError::Unauthorized.into_response());
    };

    if !scheme.eq_ignore_ascii_case("bearer") {
        error!("Unsupported authorization scheme: {}", scheme);
        return Err(ServiceError::Unauthorized.into_response());
    }

    match decode_jwt(token).await {
        Ok(t) => {
            authorities.insert(t.role);
            req.extensions_mut().insert(AuthUserMeta::new(t.id));
            Ok(authorities)
        }
        Err(e) => {
            error!("JWT decode error: {e:?}");
            Err(ServiceError::Unauthorized.into_response())
        }
    }
}

pub fn router_entries() -> Result<(Router<PgPool>, Router<PgPool>), ServiceError> {
    let auth_routes = Router::new()
        .route("/login/", post(login))
        .route("/refresh/", post(refresh));

    let api_routes = Router::new()
        .route("/ts-language/", get(ts_language_select))
        .route("/auth-role/", get(auth_role_select))
        .route("/auth-user/", get(auth_user_select).post(auth_user_insert))
        .route(
            "/auth-user/{id}/",
            delete(auth_user_delete).put(auth_user_update),
        )
        .route("/locale/", get(locale_select).post(locale_insert))
        .route("/locale/{id}/", delete(locale_delete))
        .route("/content/types/", get(content_types_select))
        .route(
            "/content/entries/",
            get(content_entries_select).post(content_insert),
        )
        .route(
            "/content/{kind}/{id}/",
            delete(content_delete).put(content_update),
        )
        .layer(GrantsLayer::with_extractor(extract));

    Ok((auth_routes, api_routes))
}
