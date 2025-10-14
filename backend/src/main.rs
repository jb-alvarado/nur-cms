use std::collections::HashSet;
use std::{env, sync::LazyLock};

use axum::response::IntoResponse;
use axum::{
    Router,
    extract::Request,
    response::Response,
    routing::{get, patch, post},
};
use clap::Parser;
use colored::Colorize;
use dotenvy::{dotenv, from_filename};
use protect_endpoints_core::tower::middleware::GrantsLayer;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use tracing::{debug, error};

pub mod api;
pub mod db;
pub mod utils;

use crate::{
    api::{
        auth::{decode_jwt, login, refresh},
        routes::*,
    },
    db::{handles, models::Role},
    utils::{
        cmd_args::{Args, add_user},
        errors::ServiceError,
        logging::init_tracing,
    },
};

// Token lifetime
const ACCESS_LIFETIME: i64 = 3;
const REFRESH_LIFETIME: i64 = 30;

pub static ARGS: LazyLock<Args> = LazyLock::new(Args::parse);
pub static JWT_SECRET: LazyLock<String> =
    LazyLock::new(|| env::var("JWT_SECRET").expect("JWT_SECRET must be set"));

async fn init_db() -> Result<Pool<Postgres>, ServiceError> {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(50)
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
            Ok(authorities)
        }
        Err(e) => {
            error!("JWT decode error: {e:?}");
            Err(ServiceError::Unauthorized.into_response())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), ServiceError> {
    if dotenv().is_err() {
        from_filename(".env.example").ok();
    }

    init_tracing();

    let pool = init_db().await?;

    if ARGS.add_user {
        add_user(&pool).await?;

        return Ok(());
    }

    let auth_routes = Router::new()
        .route("/login/", post(login))
        .route("/refresh/", post(refresh));
    let api_routes = Router::new()
        .route("/hello/", get(welcome))
        .route("/auth-user/", get(auth_user_select))
        .route("/auth-user/{id}/", patch(auth_user_update))
        .layer(GrantsLayer::with_extractor(extract));

    let app = Router::new()
        .nest("/auth", auth_routes)
        .nest("/api", api_routes)
        .with_state(pool);

    let listener =
        tokio::net::TcpListener::bind(ARGS.listen.as_deref().unwrap_or("127.0.0.1:7777"))
            .await
            .map_err(|e| {
                error!("Failed to bind TCP listener: {e:?}");
                ServiceError::InternalServerError
            })?;

    if let Ok(addr) = listener.local_addr() {
        debug!("listening on {}", addr.to_string().yellow());
    } else {
        debug!("listening on bound address (local_addr unavailable)");
    }

    axum::serve(listener, app).await?;

    Ok(())
}
