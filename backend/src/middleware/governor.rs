use std::task::{Context, Poll};

use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode},
};
use futures_util::future::BoxFuture;
use lazy_limit::HttpMethod;
use real::RealIp;
use tower::Service;
use tracing::warn;

use crate::db::models::AuthUserMeta;

#[derive(Debug, Default, Clone)]
pub struct GovernorLayer {}

impl<S> tower::Layer<S> for GovernorLayer {
    type Service = GovernorMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GovernorMiddleware::new(inner)
    }
}

/// The middleware service that performs rate-limiting.
#[derive(Clone)]
pub struct GovernorMiddleware<S> {
    inner: S,
}

impl<S> GovernorMiddleware<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for GovernorMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let method = req.method().clone();
            let ip_ext = req.extensions().get::<RealIp>();
            let auth = req.extensions().get::<AuthUserMeta>();

            if ip_ext.is_none() {
                warn!(
                    "RealIp extension not found. Make sure RealIpLayer is installed before GovernorLayer."
                );
                let response = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(
                        "Internal Server Error: Rate limiter misconfigured",
                    ))
                    .unwrap();
                return Ok(response);
            }

            let ip_str = ip_ext.unwrap().ip().to_string();
            let path = req.uri().path().to_string();
            let allow = lazy_limit::limit_override!(&ip_str, &path, map_method(method)).await;

            if auth.is_some_and(|a| a.id > 0) || allow {
                inner.call(req).await
            } else {
                // return `429 Too Many Requests`.
                let response = Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .body(Body::from("Too Many Requests"))
                    .unwrap();
                Ok(response)
            }
        })
    }
}

pub fn map_method(m: Method) -> HttpMethod {
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
