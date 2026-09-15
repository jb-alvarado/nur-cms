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
            "sqlx=warn,tower_http=info,nur_core=debug,nur_plugins={l},{}={l}",
            env!("CARGO_CRATE_NAME")
        )),
        None => EnvFilter::new(format!(
            "sqlx=warn,tower_http=info,nur_core=debug,nur_plugins=debug,{}=debug",
            env!("CARGO_CRATE_NAME")
        )),
    };

    let fmt_layer = if timestamp {
        fmt::layer()
            .with_timer(ChronoLocalTimer)
            .with_target(false)
            .with_level(true)
            .with_ansi(true)
            .with_ansi_sanitization(false)
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
            .with_ansi_sanitization(false)
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
    let r = redact_sensitive_link_value(&r);

    let a = req
        .headers()
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();

    let uri = redact_sensitive_link(&uri);
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

fn redact_sensitive_link(uri: &axum::http::Uri) -> String {
    if let Some(prefix) = plugin_file_link_prefix(uri.path()) {
        return format!("{prefix}[redacted]");
    }
    const PREFIX: &str = "/api/comments/moderate/";

    if let Some(token) = uri.path().strip_prefix(PREFIX)
        && !token.is_empty()
        && !token.contains('/')
    {
        return format!("{PREFIX}[redacted]");
    }
    uri.to_string()
}

fn redact_sensitive_link_value(value: &str) -> String {
    if value
        .parse::<axum::http::Uri>()
        .ok()
        .is_some_and(|uri| plugin_file_link_prefix(uri.path()).is_some())
    {
        return "[redacted plugin file link]".to_string();
    }
    if value.contains("/api/comments/moderate/") {
        "[redacted moderation link]".to_string()
    } else {
        value.to_string()
    }
}

fn plugin_file_link_prefix(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/api/plugins/")?;
    let (plugin, rest) = rest.split_once('/')?;
    if plugin.is_empty() {
        return None;
    }
    let token = rest
        .strip_prefix("files/upload/")
        .or_else(|| rest.strip_prefix("files/download/"))?;
    Some(&path[..path.len() - token.len()])
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;

    use super::{redact_sensitive_link, redact_sensitive_link_value};

    #[test]
    fn redacts_plugin_file_tokens_from_urls_and_referers() {
        for action in ["upload", "download"] {
            let path = format!("/api/plugins/example/files/{action}/secret-token");
            let uri = format!("{path}?copy=secret-token").parse().unwrap();
            assert_eq!(
                redact_sensitive_link(&uri),
                format!("/api/plugins/example/files/{action}/[redacted]")
            );
            for value in [
                path.clone(),
                format!("https://cms.example.org{path}?copy=secret-token"),
            ] {
                assert_eq!(
                    redact_sensitive_link_value(&value),
                    "[redacted plugin file link]"
                );
            }
        }
        let path = "/api/plugins/example/files/documents/download?path=report.pdf";
        assert_eq!(redact_sensitive_link(&path.parse().unwrap()), path);
        assert_eq!(redact_sensitive_link_value(path), path);
    }

    #[test]
    fn redacts_moderation_tokens_from_request_data() {
        let uri: Uri = "/api/comments/moderate/secret-token?ignored=true"
            .parse()
            .expect("valid URI");
        assert_eq!(
            redact_sensitive_link(&uri),
            "/api/comments/moderate/[redacted]"
        );
        assert_eq!(
            redact_sensitive_link_value(
                "https://cms.example.org/api/comments/moderate/secret-token"
            ),
            "[redacted moderation link]"
        );
    }
}
