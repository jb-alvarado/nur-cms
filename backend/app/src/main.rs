use std::{
    collections::HashSet,
    env,
    net::{IpAddr, SocketAddr},
    sync::{Arc, LazyLock},
};

use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, State},
    http::{Method, Request, StatusCode},
    middleware::{self},
    routing::{any, get, post},
};
use clap::Parser;
use colored::Colorize;
use dotenvy::{dotenv, from_filename};
use ipnet::IpNet;
use lazy_limit::{Duration as LDuration, HttpMethod, RuleConfig, init_rate_limiter};
use protect_axum::GrantsLayer;
use real::RealIp;
use tokio::{
    net::TcpListener,
    sync::{Mutex, broadcast},
};
use tower::{ServiceBuilder, limit::ConcurrencyLimitLayer};
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer, timeout::TimeoutLayer};
use tracing::{debug, error};

#[cfg(not(debug_assertions))]
mod serve;

mod utils;

use nur_core::{
    CMS_CONFIG, CONFIG, STORAGE,
    db::handles,
    extract,
    file::video::start_video_workers,
    init_db,
    middleware::governor::rate_limit,
    router_entries,
    sse::{
        SseAuthState,
        routes::{generate_uuid, sse_handler},
    },
    utils::{cmd_args::add_user, errors::NurError, importer},
};
use nur_plugins::{PluginCacheInvalidator, PluginManager};

use utils::{
    extend_args::AppArgs,
    logging::{init_tracing, log_middleware},
};

static TRUSTED_PROXY_CIDRS: LazyLock<Vec<IpNet>> = LazyLock::new(|| {
    env::var("TRUSTED_PROXY_CIDRS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|cidr| cidr.trim().parse().ok())
        .collect()
});

fn is_trusted_proxy(ip: &IpAddr, trusted_proxies: &[IpNet]) -> bool {
    trusted_proxies.iter().any(|network| network.contains(ip))
}

fn forwarded_client_ip(
    headers: &axum::http::HeaderMap,
    peer_ip: IpAddr,
    trusted_proxies: &[IpNet],
) -> Option<IpAddr> {
    if let Some(value) = headers.get("x-forwarded-for") {
        let mut chain = value
            .to_str()
            .ok()?
            .split(',')
            .map(str::trim)
            .map(str::parse::<IpAddr>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        chain.push(peer_ip);

        return chain
            .into_iter()
            .rev()
            .find(|address| !is_trusted_proxy(address, trusted_proxies));
    }

    headers.get("x-real-ip")?.to_str().ok()?.trim().parse().ok()
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = terminate.recv() => {},
                }
            }
            Err(error) => {
                error!(%error, "Failed to install SIGTERM handler");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn resolve_real_ip(
    mut req: Request<axum::body::Body>,
    next: middleware::Next,
) -> axum::response::Response {
    let peer_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|peer| peer.0.ip());

    let client_ip = peer_ip.map(|peer| {
        if is_trusted_proxy(&peer, &TRUSTED_PROXY_CIDRS) {
            forwarded_client_ip(req.headers(), peer, &TRUSTED_PROXY_CIDRS).unwrap_or(peer)
        } else {
            peer
        }
    });

    if let Some(client_ip) = client_ip {
        req.extensions_mut().insert(RealIp(client_ip));
    }

    next.run(req).await
}

async fn invalidate_plugin_caches(
    State(caches): State<PluginCacheInvalidator>,
    request: Request<Body>,
    next: middleware::Next,
) -> axum::response::Response {
    let method = request.method().clone();
    let response = next.run(request).await;

    if response.status().is_success()
        && matches!(
            method,
            Method::POST | Method::PUT | Method::PATCH | Method::DELETE
        )
    {
        caches.invalidate();
    }

    response
}

#[cfg(not(debug_assertions))]
use serve::routes::admin_ui_routes;

#[tokio::main]
async fn main() -> Result<(), NurError> {
    match dotenv() {
        Ok(_) => {}
        Err(error) if error.not_found() => {
            if let Err(error) = from_filename("./assets/.env.example")
                && !error.not_found()
            {
                error!("Failed to load fallback environment file: {error}");
                return Err(NurError::InternalServerError);
            }
        }
        Err(error) => {
            error!("Failed to load .env: {error}");
            return Err(NurError::InternalServerError);
        }
    }

    let args = AppArgs::parse();

    init_tracing(args.log_level.clone(), args.log_timestamp);

    let pool = init_db().await?;

    {
        let config = handles::select_configuration(&pool).await?;
        let mut cfg = CONFIG.write().await;
        *cfg = config;
    }
    {
        let config = handles::select_cms_configuration(&pool).await?;
        let mut cfg = CMS_CONFIG.write().await;
        *cfg = config;
    }

    if args.core.add_user {
        add_user(&pool).await?;
        return Ok(());
    }

    #[cfg(debug_assertions)]
    handles::dev_migrate(&pool).await?;

    if let Some(path) = args.core.import_markdown {
        let ignore = args.core.ignore_files.unwrap_or_default();
        importer::import_markdown(&pool, path, ignore, args.core.import_media.clone()).await?;
        return Ok(());
    }

    let plugin_manager = PluginManager::load(&pool).await.map_err(|error| {
        error!(%error, "Failed to load plugins");
        NurError::InternalServerError
    })?;
    let plugin_routes = plugin_manager.router().map_err(|error| {
        error!(%error, "Failed to register plugin routes");
        NurError::InternalServerError
    })?;
    let plugin_cache_invalidator = plugin_manager.cache_invalidator();

    let (tx, _rx) = broadcast::channel(20);
    let (shutdown_tx, _) = broadcast::channel(1);
    start_video_workers(pool.clone(), tx.clone());

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
    let auth_routes =
        auth_routes
            .layer(ConcurrencyLimitLayer::new(32))
            .layer(TimeoutLayer::with_status_code(
                axum::http::StatusCode::REQUEST_TIMEOUT,
                std::time::Duration::from_secs(20),
            ));
    let api_routes = api_routes
        .layer(middleware::from_fn_with_state(
            plugin_cache_invalidator,
            invalidate_plugin_caches,
        ))
        .layer(ConcurrencyLimitLayer::new(64))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(300),
        ));

    let sse_router = Router::new()
        .route(
            "/",
            get(sse_handler).with_state((tx.clone(), sse_state.clone(), shutdown_tx.clone())),
        )
        .route("/generate-uuid", post(generate_uuid).with_state(sse_state));

    let middlewares = ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            axum::http::HeaderValue::from_static("nosniff"),
        ))
        .layer(middleware::from_fn(resolve_real_ip))
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
        .merge(plugin_routes)
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
        .merge(plugin_routes)
        .layer(middlewares);

    if cfg!(debug_assertions) || args.serve_static {
        debug!("Serving static files from {:?}", STORAGE.as_str());
        let uploads_service = ServeDir::new(&*STORAGE);
        app = app
            .route(
                "/uploads/.processing",
                any(|| async { StatusCode::NOT_FOUND }),
            )
            .route(
                "/uploads/.processing/{*path}",
                any(|| async { StatusCode::NOT_FOUND }),
            )
            .nest_service("/uploads", uploads_service);
    }

    let listener = TcpListener::bind(args.core.listen.as_deref().unwrap_or("127.0.0.1:8777"))
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

    let server_shutdown = shutdown_tx.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        let _ = server_shutdown.send(());
    })
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::forwarded_client_ip;
    use axum::http::{HeaderMap, HeaderValue};
    use ipnet::IpNet;
    use std::net::{IpAddr, Ipv4Addr};

    fn trusted_proxies() -> Vec<IpNet> {
        vec!["10.0.0.0/8".parse().expect("valid test CIDR")]
    }

    #[test]
    fn uses_the_first_forwarded_for_address() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 10.0.0.2"),
        );

        assert_eq!(
            forwarded_client_ip(
                &headers,
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3)),
                &trusted_proxies(),
            ),
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)))
        );
    }

    #[test]
    fn rejects_a_spoofed_prefix_when_a_proxy_appends() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.99, 203.0.113.10, 10.0.0.2"),
        );

        assert_eq!(
            forwarded_client_ip(
                &headers,
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3)),
                &trusted_proxies(),
            ),
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)))
        );
    }

    #[test]
    fn malformed_forwarded_chain_falls_back_to_peer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("not-an-ip, 10.0.0.2"),
        );

        assert_eq!(
            forwarded_client_ip(
                &headers,
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3)),
                &trusted_proxies(),
            ),
            None
        );
    }
}
