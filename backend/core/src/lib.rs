use std::{
    collections::HashSet,
    env,
    sync::{Arc, LazyLock},
    time::Duration,
};

use axum::{
    Router,
    extract::{DefaultBodyLimit, Extension, Request, State},
    http::Method,
    middleware::{self as axum_middleware, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
pub use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::sync::{RwLock, Semaphore, broadcast::Sender};
use tracing::{error, warn};

pub mod api;
pub mod db;
pub mod file;
pub mod mail;
pub mod middleware;
pub mod sse;
pub mod utils;

use crate::{
    api::{
        auth::{decode_jwt, login, logout, refresh, verify},
        entry_cache::EntryCache,
        routes::*,
    },
    db::{
        handles,
        models::{AuthUserMeta, Configuration, Role},
    },
    file::routes::{upload_chunk, upload_status},
    utils::{cmd_args::Args, errors::NurError},
};

type AuthRouter = Router<(PgPool, Args)>;
type ApiRouter = Router<(PgPool, Sender<String>)>;

/// Compact rejection type for the authorization middleware.
#[derive(Clone, Copy, Debug)]
pub struct AuthorizationError;

impl From<AuthorizationError> for Response {
    fn from(_: AuthorizationError) -> Self {
        NurError::Unauthorized.into_response()
    }
}

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

pub(crate) fn env_bounded_i64(key: &str, default: i64, minimum: i64, maximum: i64) -> i64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .unwrap_or(default)
}

/// Legacy access-token lifetime in days. Prefer `ACCESS_LIFETIME_MINUTES`.
pub static ACCESS_LIFETIME: LazyLock<i64> = LazyLock::new(|| env_parse_or("ACCESS_LIFETIME", 1));
/// Access-token lifetime in minutes. The legacy day setting is honored only when explicitly set.
pub static ACCESS_LIFETIME_MINUTES: LazyLock<i64> = LazyLock::new(|| {
    if env::var_os("ACCESS_LIFETIME_MINUTES").is_some() {
        env_bounded_i64("ACCESS_LIFETIME_MINUTES", 15, 5, 1_440)
    } else if env::var_os("ACCESS_LIFETIME").is_some() {
        env_bounded_i64("ACCESS_LIFETIME", 1, 1, 30) * 1_440
    } else {
        15
    }
});
pub static REFRESH_LIFETIME: LazyLock<i64> =
    LazyLock::new(|| env_bounded_i64("REFRESH_LIFETIME", 30, 1, 365));
pub static STORAGE: LazyLock<String> =
    LazyLock::new(|| env_parse_or("STORAGE", "./uploads".to_string()));
pub static PUBLIC_UPLOADS: &str = "/uploads";
pub static MAX_UPLOAD_SIZE: LazyLock<u64> =
    LazyLock::new(|| env_parse_or("MAX_UPLOAD_SIZE", 800 * 1024 * 1024)); // 800MB default
pub static MAX_CHUNK_SIZE: LazyLock<u64> =
    LazyLock::new(|| env_parse_or("MAX_CHUNK_SIZE", 10 * 1024 * 1024)); // 10MB default
pub static MAX_IMAGE_PIXELS: LazyLock<u64> =
    LazyLock::new(|| env_parse_or("MAX_IMAGE_PIXELS", 40_000_000));
pub static MAX_ACTIVE_UPLOADS_PER_USER: LazyLock<usize> =
    LazyLock::new(|| env_parse_or("MAX_ACTIVE_UPLOADS_PER_USER", 4));
pub static UPLOAD_TTL_SECONDS: LazyLock<u64> =
    LazyLock::new(|| env_parse_or("UPLOAD_TTL_SECONDS", 24 * 60 * 60));
pub static IMAGE_PROCESSING_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| {
    Semaphore::new(env_parse_or("IMAGE_PROCESSING_CONCURRENCY", 2usize).clamp(1, 16))
});

pub static CONFIG: LazyLock<Arc<RwLock<Configuration>>> =
    LazyLock::new(|| Arc::new(RwLock::new(Configuration::default())));

pub async fn init_db() -> Result<PgPool, NurError> {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let max_connections = env_parse_or("MAX_CONNECTIONS", 50u32);

    let pool = PgPoolOptions::new()
        .min_connections(1)
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(300)))
        .max_lifetime(Some(Duration::from_secs(3600)))
        .connect(&database_url)
        .await?;

    handles::db_migrate(&pool).await?;
    file::helper::cleanup_persisted_uploads().await;

    Ok(pool)
}

pub async fn extract(req: &mut Request) -> Result<HashSet<Role>, AuthorizationError> {
    let Some(auth) = req.headers().get("authorization") else {
        req.extensions_mut().insert(AuthUserMeta::new(-1));
        return Ok(HashSet::from([Role::Guest]));
    };

    let Some((scheme, token)) = auth.to_str().ok().and_then(|s| s.trim().split_once(' ')) else {
        warn!("Malformed or invalid authorization header");
        return Err(AuthorizationError);
    };

    if !scheme.eq_ignore_ascii_case("bearer") {
        warn!(scheme = %scheme, "Unsupported authorization scheme");
        return Err(AuthorizationError);
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
            Err(AuthorizationError)
        }
    }
}

async fn invalidate_entry_cache(
    State(cache): State<EntryCache>,
    request: Request,
    next: Next,
) -> Response {
    let mutates_content = matches!(
        request.method(),
        &Method::POST | &Method::PUT | &Method::DELETE
    ) && matches!(
        request.uri().path(),
        path if path.starts_with("/content/")
            || path.starts_with("/api/content/")
            || path.starts_with("/media/")
            || path.starts_with("/api/media/")
            || path.starts_with("/locales/")
            || path.starts_with("/api/locales/")
            || path.starts_with("/configuration/")
            || path.starts_with("/api/configuration/")
    );
    let response = next.run(request).await;

    if mutates_content && response.status().is_success() {
        cache.invalidate();
    }

    response
}

pub fn router_entries() -> (AuthRouter, ApiRouter) {
    let entry_cache = EntryCache::from_env();
    let auth_routes = Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/verify", post(verify))
        .layer(DefaultBodyLimit::max(16 * 1024));

    let upload_routes = Router::new()
        .route("/upload", get(upload_status).post(upload_chunk))
        .layer(DefaultBodyLimit::max(
            usize::try_from(*MAX_CHUNK_SIZE)
                .unwrap_or(10 * 1024 * 1024)
                .saturating_add(64 * 1024),
        ));

    let auth_user_routes = Router::new()
        .route("/", get(auth_user_select).post(auth_user_insert))
        .route("/{id}", delete(auth_user_delete).put(auth_user_update));

    let config_routes = Router::new().route("/", get(config_select).put(config_update));

    let locale_routes = Router::new()
        .route("/", get(locale_select).post(locale_insert))
        .route("/{id}", put(locale_update).delete(locale_delete));

    let comment_routes = Router::new()
        .route("/", get(comments_select).post(comment_insert))
        .route("/{id}", delete(comment_delete).put(comment_update));

    let content_routes = Router::new()
        .route("/types", get(types_select).post(type_insert))
        .route("/types/{id}", put(type_update).delete(type_delete))
        .route("/authors", get(authors_select).post(author_insert))
        .route("/authors/{id}", put(author_update).delete(author_delete))
        .route("/categories", get(categories_select).post(category_insert))
        .route(
            "/categories/{id}",
            put(category_update).delete(category_delete),
        )
        .route("/entries/author", post(entry_author_insert))
        .route("/entries/{e_id}/author/{a_id}", delete(entry_author_delete))
        .route("/entries/tag", post(entry_tag_insert))
        .route("/entries/{e_id}/tag/{t_id}", delete(entry_tag_delete))
        .route("/entries/facets", get(entry_facets_select))
        .route("/entries", get(entries_select).post(entry_insert))
        .route("/entries/{id}", put(entry_update).delete(entry_delete))
        .route("/entries/{param}/{slug}", get(entry_select))
        .route(
            "/node/templates",
            get(template_select).post(template_insert),
        )
        .route(
            "/node/templates/{id}",
            delete(template_delete).put(template_update),
        )
        .route("/tags", get(tags_select).post(tag_insert))
        .route("/tags/{id}", put(tag_update))
        .layer(Extension(entry_cache.clone()));

    let media_routes = Router::new()
        .route("/", get(media_select))
        .route("/{id}", put(media_update).delete(media_delete));

    let contact_routes = Router::new()
        .route("/targets", get(targets_select).post(target_insert))
        .route("/targets/{id}", put(target_update).delete(target_delete))
        .route("/target/{target}", post(mailer));

    let api_routes = Router::new()
        .route("/ts-language", get(ts_language_select))
        .route("/auth-role", get(auth_role_select))
        .merge(upload_routes)
        .nest("/auth-user", auth_user_routes)
        .nest("/configuration", config_routes)
        .nest("/contact", contact_routes)
        .nest("/locales", locale_routes)
        .nest("/comments", comment_routes)
        .nest("/content", content_routes)
        .nest("/media", media_routes)
        .layer(axum_middleware::from_fn_with_state(
            entry_cache,
            invalidate_entry_cache,
        ));

    (auth_routes, api_routes)
}
