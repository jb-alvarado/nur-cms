use std::{collections::HashSet, sync::Arc};

use axum::{
    Router,
    routing::{get, post},
};
use clap::Parser;
use colored::Colorize;
use dotenvy::{dotenv, from_filename};
use protect_axum::GrantsLayer;
use tokio::sync::{Mutex, broadcast};
use tracing::{debug, error};

#[cfg(debug_assertions)]
use tower_http::services::ServeDir;

#[cfg(debug_assertions)]
use nur_cms::STORAGE;

use nur_cms::{
    CONFIG,
    db::handles,
    extract, init_db, router_entries,
    serve::routes::admin_routes,
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

    let (auth_routes, api_routes) = router_entries();

    let sse_router = Router::new()
        .route(
            "/",
            get(sse_handler).with_state((tx.clone(), sse_state.clone())),
        )
        .route("/generate-uuid", post(generate_uuid).with_state(sse_state))
        .layer(GrantsLayer::with_extractor(extract));

    #[cfg(debug_assertions)]
    let mut app = Router::new()
        .nest("/auth", auth_routes.with_state(pool.clone()))
        .nest("/api", api_routes.with_state((pool, tx.clone())))
        .merge(admin_routes())
        .nest("/sse", sse_router);

    #[cfg(not(debug_assertions))]
    let app = Router::new()
        .nest("/auth", auth_routes.with_state(pool.clone()))
        .nest("/api", api_routes.with_state((pool, tx.clone())))
        .merge(admin_routes())
        .nest("/sse", sse_router);

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

    axum::serve(listener, app).await?;

    Ok(())
}
