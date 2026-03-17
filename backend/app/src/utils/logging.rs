use std::time::Instant;

use axum::{
    body::Body,
    http::{
        Request, Response,
        header::{CONTENT_LENGTH, REFERER, USER_AGENT},
    },
    middleware::Next,
};
use chrono::Local;
use tracing::{error, info, warn};
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::{self, format::Writer, time::FormatTime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

struct ChronoLocalTimer;

impl FormatTime for ChronoLocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", Local::now().format("[%Y-%m-%d %H:%M:%S%.6f]"))
    }
}

pub fn init_tracing(level: Option<String>, timestamp: bool) {
    let filter = match level {
        Some(l) => EnvFilter::new(format!(
            "sqlx=warn,tower_http=info,nur_core=debug,{l}={}",
            env!("CARGO_CRATE_NAME")
        )),
        None => EnvFilter::new(format!(
            "sqlx=warn,tower_http=info,nur_core=debug,{}=debug",
            env!("CARGO_CRATE_NAME")
        )),
    };

    let fmt_layer = if timestamp {
        fmt::layer()
            .with_timer(ChronoLocalTimer)
            .with_target(false)
            .with_level(true)
            .with_ansi(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .boxed()
    } else {
        fmt::layer()
            .compact()
            .without_time()
            .with_target(false)
            .with_level(true)
            .with_ansi(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .boxed()
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}

pub async fn log_middleware(req: Request<Body>, next: Next) -> Response<Body> {
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
