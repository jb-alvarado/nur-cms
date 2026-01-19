use std::time::Instant;

use axum::{
    Router,
    body::{Body, HttpBody},
    http::{Request, Response},
    middleware::{Next, from_fn},
    response::IntoResponse,
    routing::get,
};
use real::RealIpLayer;
use std::net::SocketAddr;
use tracing::{error, info};

use nur_cms::utils::errors::ServiceError;

async fn log_middleware(req: Request<Body>, next: Next) -> Response<Body> {
    let timer = Instant::now();
    let ip = req
        .extensions()
        .get::<real::RealIp>()
        .map(|ip| ip.0.to_string())
        .unwrap_or_else(|| "-".to_string());
    let version = req.version();
    let request = format!("{} {} {:?}", req.method(), req.uri(), version);

    let referrer = req
        .headers()
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();

    let agent = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();

    let res = next.run(req).await;
    let status = res.status().as_u16();
    let size = res.size_hint().exact().unwrap_or_default();

    let latency = timer.elapsed().as_secs_f64();

    if status >= 500 {
        error!(r#"{ip} "{request}" {status} {size} "{referrer}" "{agent}" {latency:.6}"#);
    } else if matches!(status, 401 | 403 | 429) {
        info!(r#"{ip} "{request}" {status} {size} "{referrer}" "{agent}" {latency:.6}"#);
    } else {
        info!(r#"{ip} "{request}" {status} {size} "{referrer}" "{agent}" {latency:.6}"#);
    }

    res
}

#[tokio::main]
async fn main() -> Result<(), ServiceError> {
    // Init logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Minimal handler
    async fn handler() -> impl IntoResponse {
        "Hello World!"
    }

    // Build router
    let app = Router::new()
        .route("/", get(handler))
        // Real IP Layer
        // Access-Log Layer
        .layer(from_fn(log_middleware))
        .layer(RealIpLayer::default());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .map_err(|e| {
            error!("Failed to bind TCP listener: {e:?}");
            ServiceError::InternalServerError
        })?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
