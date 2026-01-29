use std::{collections::HashSet, net::SocketAddr, sync::Arc, time::Instant};

use axum::{
    Router,
    body::Body,
    http::{
        Request, Response,
        header::{CONTENT_LENGTH, REFERER, USER_AGENT},
    },
    middleware::{self, Next},
    routing::{get, post},
};
use clap::Parser;
use colored::Colorize;
use dotenvy::{dotenv, from_filename};
use lazy_limit::{Duration as LDuration, HttpMethod, RuleConfig, init_rate_limiter};
use protect_axum::GrantsLayer;
use real::RealIpLayer;
use tokio::sync::{Mutex, broadcast};
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tracing::{debug, error, info, warn};

#[cfg(not(debug_assertions))]
mod serve;

mod utils;

use core::{
    CONFIG, STORAGE,
    db::handles,
    extract, init_db,
    middleware::governor::rate_limit,
    router_entries,
    sse::{
        SseAuthState,
        routes::{generate_uuid, sse_handler},
    },
    utils::{cmd_args::add_user, errors::NurError, importer},
};

use utils::{extend_args::AppArgs, logging::init_tracing};

#[cfg(not(debug_assertions))]
use serve::routes::admin_ui_routes;

async fn log_middleware(req: Request<Body>, next: Next) -> Response<Body> {
    let start = Instant::now();

    let m = req.method().clone();
    let uri = req.uri().clone();
    let v = req.version();

    let ip = req
        .extensions()
        .get::<real::RealIp>()
        .map(|ip| ip.0.to_string())
        .unwrap_or_else(|| "-".into());

    let r = req
        .headers()
        .get(REFERER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();

    let a = req
        .headers()
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();

    let res = next.run(req).await;

    let status = res.status().as_u16();
    let size = res
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");

    let l = start.elapsed().as_secs_f64();

    match status {
        500..=599 => {
            error!(r#"{ip} "{m} {uri} {v:?}" {status} {size} "{r}" "{a}" {l:.6}"#);
        }
        401 | 403 | 429 => {
            warn!(r#"{ip} "{m} {uri} {v:?}" {status} {size} "{r}" "{a}" {l:.6}"#);
        }
        _ => {
            info!(r#"{ip} "{m} {uri} {v:?}" {status} {size} "{r}" "{a}" {l:.6}"#);
        }
    }

    res
}

#[tokio::main]
async fn main() -> Result<(), NurError> {
    if dotenv().is_err() {
        from_filename("./assets/.env.example").ok();
    }

    let args = AppArgs::parse();

    init_tracing(args.log_level.clone(), args.log_timestamp);

    let pool = init_db().await?;

    {
        let config = handles::select_configuration(&pool).await?;
        let mut cfg = CONFIG.write().await;
        *cfg = config;
    }

    if args.core.add_user {
        add_user(&pool).await?;
        return Ok(());
    }

    if let Some(path) = args.core.import_markdown {
        let ignore = args.core.ignore_files.unwrap_or_default();
        importer::import_markdown(&pool, path, ignore, args.core.import_media.clone()).await?;
        return Ok(());
    }

    let (tx, _rx) = broadcast::channel(20);

    let sse_state = SseAuthState {
        uuids: Arc::new(Mutex::new(HashSet::new())),
    };

    init_rate_limiter!(
        default: RuleConfig::new(LDuration::seconds(1), 10), // 10 req/s globally
        max_memory: Some(64 * 1024 * 1024), // 64MB max memory
        routes: [
            ("/auth/", RuleConfig::new(LDuration::minutes(1), 3).match_prefix(true)), // 3 req/min
            ("/api/comments", RuleConfig::new(LDuration::minutes(3), 1).for_methods(vec![HttpMethod::POST])), // 1 req/3 min
            ("/api/contact/target/", RuleConfig::new(LDuration::minutes(3), 1).match_prefix(true)), // 1 req/3 min
        ]
    )
    .await;

    let (auth_routes, api_routes) = router_entries();

    let sse_router = Router::new()
        .route(
            "/",
            get(sse_handler).with_state((tx.clone(), sse_state.clone())),
        )
        .route("/generate-uuid", post(generate_uuid).with_state(sse_state));

    let middlewares = ServiceBuilder::new()
        .layer(RealIpLayer::default())
        .layer(middleware::from_fn(log_middleware))
        .layer(GrantsLayer::with_extractor(extract))
        .layer(middleware::from_fn(rate_limit));

    #[cfg(debug_assertions)]
    let mut app = Router::new()
        .nest(
            "/auth",
            auth_routes.with_state((pool.clone(), args.core.clone())),
        )
        .nest("/api", api_routes.with_state((pool, tx.clone())))
        .nest("/sse", sse_router)
        .layer(middlewares);

    #[cfg(not(debug_assertions))]
    let mut app = Router::new()
        .nest(
            "/auth",
            auth_routes.with_state((pool.clone(), args.core.clone())),
        )
        .nest("/api", api_routes.with_state((pool, tx.clone())))
        .merge(admin_ui_routes())
        .nest("/sse", sse_router)
        .layer(middlewares);

    if cfg!(debug_assertions) || args.serve_static {
        debug!("Serving static files from {:?}", STORAGE.as_str());
        let uploads_service = ServeDir::new(&*STORAGE);
        app = app.nest_service("/uploads", uploads_service);
    }

    let listener =
        tokio::net::TcpListener::bind(args.core.listen.as_deref().unwrap_or("127.0.0.1:8777"))
            .await
            .map_err(|e| {
                error!("Failed to bind TCP listener: {e:?}");
                NurError::InternalServerError
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
