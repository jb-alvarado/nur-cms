use axum::{
    body::Body,
    http::{Method, Request, Response},
    middleware::Next,
};
use lazy_limit::HttpMethod;
use real::RealIp;
use tracing::error;

use crate::{db::models::AuthUserMeta, utils::errors::NurError};

fn map_method(m: Method) -> HttpMethod {
    match m {
        Method::GET => HttpMethod::GET,
        Method::POST => HttpMethod::POST,
        Method::PUT => HttpMethod::PUT,
        Method::DELETE => HttpMethod::DELETE,
        Method::PATCH => HttpMethod::PATCH,
        Method::HEAD => HttpMethod::HEAD,
        Method::OPTIONS => HttpMethod::OPTIONS,
        Method::CONNECT => HttpMethod::CONNECT,
        Method::TRACE => HttpMethod::TRACE,
        _ => HttpMethod::OTHER,
    }
}

/// Applies request rate limiting based on client IP, path and HTTP method.
///
/// Authenticated users with a positive id bypass this check. Unauthenticated
/// requests are allowed only if `lazy_limit` permits the request; otherwise a
/// `TooManyRequests` error is returned.
pub async fn rate_limit(req: Request<Body>, next: Next) -> Result<Response<Body>, NurError> {
    let ip_ext = match req.extensions().get::<RealIp>() {
        Some(ip) => ip,
        None => {
            error!("RealIp extension not found");
            return Err(NurError::InternalServerError);
        }
    };
    let auth = req.extensions().get::<AuthUserMeta>();
    let method = req.method().clone();

    let ip_str = ip_ext.ip().to_string();
    let path = req.uri().path().to_string();

    if auth.is_some_and(|a| a.id > 0)
        || lazy_limit::limit_override!(&ip_str, &path, map_method(method)).await
    {
        let response = next.run(req).await;
        Ok(response)
    } else {
        Err(NurError::ToManyRequests)
    }
}
