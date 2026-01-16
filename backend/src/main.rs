use std::{collections::HashSet, net::SocketAddr, sync::Arc};

use axum::{
    Router,
    routing::{get, post},
};
use axum_governor::{GovernorConfig, GovernorLayer};
use clap::Parser;
use colored::Colorize;
use dotenvy::{dotenv, from_filename};
use lazy_limit::{Duration, HttpMethod, RuleConfig, init_rate_limiter};
use protect_axum::GrantsLayer;
use real::RealIpLayer;
use tokio::sync::{Mutex, broadcast};
use tower::ServiceBuilder;
use tracing::{debug, error};

#[cfg(debug_assertions)]
use tower_http::services::ServeDir;

#[cfg(debug_assertions)]
use nur_cms::STORAGE;

use nur_cms::{
    CONFIG,
    db::handles,
    extract, init_db, router_entries,
    sse::{
        SseAuthState,
        routes::{generate_uuid, sse_handler},
    },
    utils::{
        cmd_args::{Args, add_user},
        errors::ServiceError,
        importer,
        logging::init_tracing,
    },
};

#[cfg(not(debug_assertions))]
use nur_cms::serve::routes::admin_ui_routes;

#[tokio::main]
async fn main() -> Result<(), ServiceError> {
    if dotenv().is_err() {
        from_filename("./assets/.env.example").ok();
    }

    let args = Args::parse();

    init_tracing(args.log_level, args.log_timestamp);

    let pool = init_db().await?;

    {
        let config = handles::select_configuration(&pool).await?;
        let mut cfg = CONFIG.write().await;
        *cfg = config;
    }

    if args.add_user {
        add_user(&pool).await?;
        return Ok(());
    }

    if let Some(path) = args.import_markdown {
        let ignore = args.ignore_files.unwrap_or_default();
        importer::import_markdown(&pool, path, ignore, args.import_media.clone()).await?;
        return Ok(());
    }

    let (tx, _rx) = broadcast::channel(20);

    let sse_state = SseAuthState {
        uuids: Arc::new(Mutex::new(HashSet::new())),
    };

    init_rate_limiter!(
        default: RuleConfig::new(Duration::seconds(1), 1), // 100 req/s globally
        max_memory: Some(64 * 1024 * 1024), // 64MB max memory
        routes: [
            ("/auth/login", RuleConfig::new(Duration::minutes(1), 3)), // 6 req/min
            ("/api/comments", RuleConfig::new(Duration::minutes(3), 1).for_methods(vec![HttpMethod::POST])), // 1 req/3 min
            ("/api/contact/target/", RuleConfig::new(Duration::minutes(3), 1).match_prefix(true)), // 1 req/3 min
        ]
    )
    .await;

    let (auth_routes, api_routes) = router_entries();

    let sse_router = Router::new()
        .route(
            "/",
            get(sse_handler).with_state((tx.clone(), sse_state.clone())),
        )
        .route("/generate-uuid", post(generate_uuid).with_state(sse_state))
        .layer(GrantsLayer::with_extractor(extract));

    let limiter = ServiceBuilder::new()
        .layer(RealIpLayer::default())
        .layer(GovernorLayer::new(
            GovernorConfig::new().override_mode(true),
        ));

    #[cfg(debug_assertions)]
    let mut app = Router::new()
        .nest("/auth", auth_routes.with_state(pool.clone()))
        .nest("/api", api_routes.with_state((pool, tx.clone())))
        .nest("/sse", sse_router)
        .layer(limiter);

    #[cfg(not(debug_assertions))]
    let app = Router::new()
        .nest("/auth", auth_routes.with_state(pool.clone()))
        .nest("/api", api_routes.with_state((pool, tx.clone())))
        .merge(admin_ui_routes())
        .nest("/sse", sse_router)
        .layer(limiter);

    #[cfg(debug_assertions)]
    {
        debug!("Dev mode: serving static files from {:?}", STORAGE.as_str());
        let uploads_service = ServeDir::new(&*STORAGE);
        app = app.nest_service("/uploads", uploads_service);
    }

    let listener =
        tokio::net::TcpListener::bind(args.listen.as_deref().unwrap_or("127.0.0.1:8777"))
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

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
