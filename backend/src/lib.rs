use std::collections::HashSet;
use std::{
    env,
    sync::{Arc, LazyLock},
};

use axum::response::IntoResponse;
use axum::{
    Router,
    extract::Request,
    response::Response,
    routing::{delete, get, post, put},
};
use protect_endpoints_core::tower::middleware::GrantsLayer;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::sync::RwLock;
use tracing::{error, warn};

pub mod api;
pub mod db;
pub mod file;
pub mod serve;
pub mod sse;
pub mod utils;

use crate::{
    api::{
        auth::{decode_jwt, login, refresh},
        routes::*,
    },
    db::{
        handles,
        models::{AuthUserMeta, Configuration, Role},
    },
    file::routes::upload_chunk,
    utils::errors::ServiceError,
};

// Small helper to parse env vars with a typed default.
fn env_parse_or<T>(key: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<T>().ok())
        .unwrap_or(default)
}

pub static ACCESS_LIFETIME: LazyLock<i64> = LazyLock::new(|| env_parse_or("ACCESS_LIFETIME", 3));
pub static REFRESH_LIFETIME: LazyLock<i64> = LazyLock::new(|| env_parse_or("REFRESH_LIFETIME", 30));
pub static STORAGE: LazyLock<String> =
    LazyLock::new(|| env_parse_or("STORAGE", "./uploads".to_string()));
pub static PUBLIC_UPLOADS: &str = "/uploads";

pub static CONFIG: LazyLock<Arc<RwLock<Configuration>>> =
    LazyLock::new(|| Arc::new(RwLock::new(Configuration::default())));

pub async fn init_db() -> Result<PgPool, ServiceError> {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let max_connections = env_parse_or("MAX_CONNECTIONS", 50u32);

    let pool = PgPoolOptions::new()
        .min_connections(1)
        .max_connections(max_connections)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .idle_timeout(Some(std::time::Duration::from_secs(300)))
        .max_lifetime(Some(std::time::Duration::from_secs(3600)))
        .connect(&database_url)
        .await?;

    handles::db_migrate(&pool).await?;

    Ok(pool)
}

pub async fn extract(req: &mut Request) -> Result<HashSet<Role>, Response> {
    let Some(auth) = req.headers().get("authorization") else {
        req.extensions_mut().insert(AuthUserMeta::new(-1));
        return Ok(HashSet::from([Role::Guest]));
    };

    let Some((scheme, token)) = auth.to_str().ok().and_then(|s| s.trim().split_once(' ')) else {
        warn!("Malformed or invalid authorization header");
        return Err(ServiceError::Unauthorized.into_response());
    };

    if !scheme.eq_ignore_ascii_case("bearer") {
        warn!(scheme = %scheme, "Unsupported authorization scheme");
        return Err(ServiceError::Unauthorized.into_response());
    }

    match decode_jwt(token).await {
        Ok(t) => {
            let mut authorities = HashSet::with_capacity(1);
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

pub fn router_entries() -> (
    Router<PgPool>,
    Router<(PgPool, tokio::sync::broadcast::Sender<String>)>,
) {
    let auth_routes = Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh));

    let auth_user_routes = Router::new()
        .route("/", get(auth_user_select).post(auth_user_insert))
        .route("/{id}", delete(auth_user_delete).put(auth_user_update));

    let locale_routes = Router::new()
        .route("/", get(locale_select).post(locale_insert))
        .route("/{id}", delete(locale_delete));

    let content_routes = Router::new()
        .route("/types", get(content_types_select))
        .route("/authors", get(authors_select).post(author_insert))
        .route("/authors/{id}", put(author_update).delete(author_delete))
        .route("/categories", get(categories_select).post(category_insert))
        .route(
            "/categories/{id}",
            put(category_update).delete(category_delete),
        )
        .route("/entries", get(entries_select).post(entry_insert))
        .route("/entries/{id}", put(entry_update).delete(entry_delete))
        .route("/entries/{param}/{slug}", get(entry_select));
    // .route("/{kind}", post(content_insert))
    // .route("/{kind}/{id}", delete(content_delete).put(content_update));

    let api_routes = Router::new()
        .route("/ts-language", get(ts_language_select))
        .route("/auth-role", get(auth_role_select))
        .route("/upload", post(upload_chunk))
        .nest("/auth-user", auth_user_routes)
        .nest("/locales", locale_routes)
        .nest("/content", content_routes)
        .layer(GrantsLayer::with_extractor(extract));

    (auth_routes, api_routes)
}
