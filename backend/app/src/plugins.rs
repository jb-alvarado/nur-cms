use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Extension, Path, Request, State},
    http::{HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{MethodFilter, get, on},
};
use bytes::Bytes;
use moka::sync::Cache;
use nur_core::db::models::{AuthUserMeta, Role};
use nur_plugins::{
    CachePolicy, Error, Header, Identity, PluginManager, Request as PluginRequest,
    Response as PluginResponse, Route,
};
use protect_axum::authorities::AuthDetails;
use real::RealIp;
use tower_http::{services::ServeDir, timeout::TimeoutLayer};
use tracing::{error, info};

const FORWARDED_REQUEST_HEADERS: &[&str] =
    &["accept", "accept-language", "content-type", "user-agent"];

#[derive(Clone)]
pub struct PluginCacheInvalidator {
    caches: Arc<Vec<RouteCache>>,
}
impl PluginCacheInvalidator {
    pub fn invalidate(&self) {
        for cache in self.caches.iter() {
            cache.invalidate();
        }
    }
}

#[derive(Clone)]
struct RouteCache {
    responses: Cache<String, CachedResponse>,
    ttl: Duration,
    generation: Arc<AtomicU64>,
}
#[derive(Clone)]
struct CachedResponse {
    status: u16,
    headers: Vec<Header>,
    body: Bytes,
    expires_at: Instant,
}
#[derive(Clone)]
struct RouteState {
    manager: Arc<PluginManager>,
    route: Route,
    cache: Option<RouteCache>,
    plugin_cache: Option<RouteCache>,
}

pub struct PluginRouter {
    pub router: Router,
    invalidator: PluginCacheInvalidator,
}
impl PluginRouter {
    pub fn cache_invalidator(&self) -> PluginCacheInvalidator {
        self.invalidator.clone()
    }
}

pub fn router(manager: Arc<PluginManager>) -> Result<PluginRouter, Error> {
    for plugin in manager.metadata() {
        info!(plugin = %plugin.id, version = %plugin.version, "loaded plugin");
    }
    let routes = manager.routes();
    let configured = routes
        .iter()
        .filter(|route| route.cache.is_some())
        .map(|route| route.plugin_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    let capacity = cache_capacity(configured);
    let mut caches = HashMap::<String, RouteCache>::new();
    for route in &routes {
        if let Some(policy) = route.cache {
            caches
                .entry(route.plugin_id.clone())
                .or_insert_with(|| route_cache(policy, capacity));
        }
    }
    let invalidator = PluginCacheInvalidator {
        caches: Arc::new(caches.values().cloned().collect()),
    };
    let mut router =
        Router::new().route("/api/plugins", get(index).with_state(Arc::clone(&manager)));
    for asset in manager.assets() {
        router = router.nest_service(
            &format!("/plugins/{}/assets", asset.plugin_id),
            ServeDir::new(&asset.path),
        );
    }
    for route in routes {
        let method = method_filter(&route.method)?;
        let state = Arc::new(RouteState {
            cache: route
                .cache
                .and_then(|_| caches.get(&route.plugin_id).cloned()),
            plugin_cache: caches.get(&route.plugin_id).cloned(),
            manager: Arc::clone(&manager),
            route,
        });
        router = router.merge(
            Router::new()
                .route(&state.route.path, on(method, dispatch))
                .with_state(state),
        );
    }
    Ok(PluginRouter {
        router: router.layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            manager.timeout() + Duration::from_millis(250),
        )),
        invalidator,
    })
}

async fn index(State(manager): State<Arc<PluginManager>>, details: AuthDetails<Role>) -> Response {
    let roles = role_names(&details);
    if roles.is_empty() {
        return StatusCode::FORBIDDEN.into_response();
    }
    axum::Json(manager.visible_metadata(&roles)).into_response()
}

async fn dispatch(
    State(state): State<Arc<RouteState>>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Extension(real_ip): Extension<RealIp>,
    path_params: Option<Path<HashMap<String, String>>>,
    request: Request,
) -> Response {
    let roles = role_names(&details);
    let identity = request_identity(!state.route.roles.is_empty(), user.id, roles);
    if !state.manager.authorized(&state.route, identity.as_ref()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let request = match into_plugin_request(
        request,
        path_params,
        identity,
        real_ip.ip(),
        state.manager.request_body_limit(),
    )
    .await
    {
        Ok(request) => request,
        Err(status) => return status.into_response(),
    };
    if let Err(status) = validate_cached_request_body(state.cache.is_some(), &request.body) {
        return status.into_response();
    }
    let key = state
        .cache
        .as_ref()
        .map(|cache| cache.key(cache_key(&state.route.id, &request)));
    if let (Some(cache), Some(key)) = (&state.cache, &key)
        && let Some(cached) = cache.responses.get(key)
    {
        if cached.expires_at > Instant::now() {
            return into_cached_response(cached);
        }
        cache.responses.invalidate(key);
    }
    let is_write = is_plugin_write_method(&request.method);
    let result = state.manager.dispatch(&state.route, request).await;
    if is_write && let Some(cache) = &state.plugin_cache {
        cache.invalidate();
    }
    match result {
        Ok(response) => {
            if let (Some(cache), Some(key)) = (&state.cache, key)
                && response.status == 200
            {
                let body = Bytes::from(response.body);
                cache.responses.insert(
                    key,
                    CachedResponse {
                        status: response.status,
                        headers: response.headers.clone(),
                        body: body.clone(),
                        expires_at: Instant::now() + cache.ttl,
                    },
                );
                return into_response_parts(response.status, response.headers, body);
            }
            into_response(response)
        }
        Err(error) => {
            error!(plugin = %state.route.plugin_id, route = %state.route.id, %error, "plugin request failed");
            error_response(error)
        }
    }
}

async fn into_plugin_request(
    request: Request,
    path_params: Option<Path<HashMap<String, String>>>,
    identity: Option<Identity>,
    client_ip: std::net::IpAddr,
    body_limit: usize,
) -> Result<PluginRequest, StatusCode> {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(ToOwned::to_owned);
    let headers = request_headers(request.headers());
    let body = to_bytes(request.into_body(), body_limit)
        .await
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?
        .to_vec();
    let params = plugin_path_params(path_params);
    Ok(PluginRequest {
        method,
        path,
        path_params: params,
        query,
        headers,
        body,
        client_ip: Some(client_ip),
        identity,
    })
}

fn role_names(details: &AuthDetails<Role>) -> Vec<String> {
    details
        .authorities
        .iter()
        .filter(|role| !matches!(role, Role::Guest))
        .map(ToString::to_string)
        .collect()
}
fn request_identity(protected: bool, user_id: i32, roles: Vec<String>) -> Option<Identity> {
    (protected && user_id >= 0).then_some(Identity { user_id, roles })
}
fn plugin_path_params(path_params: Option<Path<HashMap<String, String>>>) -> Vec<(String, String)> {
    let mut params: Vec<_> = path_params
        .unwrap_or(Path(HashMap::new()))
        .0
        .into_iter()
        .collect();
    params.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    params
}
fn validate_cached_request_body(cache_enabled: bool, body: &[u8]) -> Result<(), StatusCode> {
    if cache_enabled && !body.is_empty() {
        Err(StatusCode::BAD_REQUEST)
    } else {
        Ok(())
    }
}
fn is_plugin_write_method(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
}
fn request_headers(headers: &axum::http::HeaderMap) -> Vec<Header> {
    headers
        .iter()
        .filter(|(name, _)| FORWARDED_REQUEST_HEADERS.contains(&name.as_str()))
        .take(32)
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .filter(|value| value.len() <= 8 * 1024)
                .map(|value| Header {
                    name: name.to_string(),
                    value: value.to_string(),
                })
        })
        .collect()
}
fn into_response(response: PluginResponse) -> Response {
    into_response_parts(
        response.status,
        response.headers,
        Bytes::from(response.body),
    )
}

fn into_cached_response(response: CachedResponse) -> Response {
    into_response_parts(response.status, response.headers, response.body)
}

fn into_response_parts(status: u16, headers: Vec<Header>, body: Bytes) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(status);
    for header in headers {
        let Ok(name) = HeaderName::try_from(header.name) else {
            continue;
        };
        if matches!(
            name.as_str(),
            "connection"
                | "content-length"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "set-cookie"
                | "te"
                | "trailer"
                | "transfer-encoding"
                | "upgrade"
        ) {
            continue;
        }
        if let Ok(value) = HeaderValue::try_from(header.value) {
            builder = builder.header(name, value);
        }
    }
    builder
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}
fn error_response(error: Error) -> Response {
    match error {
        Error::Timeout => StatusCode::GATEWAY_TIMEOUT.into_response(),
        Error::Busy => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        Error::RateLimited => StatusCode::TOO_MANY_REQUESTS.into_response(),
        Error::PluginBadRequest(_) => StatusCode::BAD_REQUEST.into_response(),
        Error::PluginForbidden => StatusCode::FORBIDDEN.into_response(),
        Error::PluginNotFound => StatusCode::NOT_FOUND.into_response(),
        _ => StatusCode::BAD_GATEWAY.into_response(),
    }
}
fn method_filter(method: &str) -> Result<MethodFilter, Error> {
    match Method::from_bytes(method.as_bytes())
        .map_err(|_| Error::Manifest("invalid plugin HTTP method".into()))?
    {
        Method::GET => Ok(MethodFilter::GET),
        Method::POST => Ok(MethodFilter::POST),
        Method::PUT => Ok(MethodFilter::PUT),
        Method::PATCH => Ok(MethodFilter::PATCH),
        Method::DELETE => Ok(MethodFilter::DELETE),
        Method::HEAD => Ok(MethodFilter::HEAD),
        Method::OPTIONS => Ok(MethodFilter::OPTIONS),
        _ => Err(Error::Manifest("unsupported plugin HTTP method".into())),
    }
}
fn cache_capacity(count: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    std::env::var("NUR_PLUGIN_CACHE_MEMORY_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| (1024 * 1024..=1024 * 1024 * 1024).contains(value))
        .unwrap_or(64 * 1024 * 1024)
        / count as u64
}
fn route_cache(policy: CachePolicy, capacity: u64) -> RouteCache {
    let minimum_weight = capacity
        .div_ceil(policy.max_entries.max(1))
        .min(u64::from(u32::MAX)) as u32;
    RouteCache {
        responses: Cache::builder()
            .max_capacity(capacity)
            .weigher(move |key: &String, cached: &CachedResponse| {
                cache_entry_weight(key, cached, minimum_weight)
            })
            .build(),
        ttl: policy.ttl,
        generation: Arc::new(AtomicU64::new(0)),
    }
}
impl RouteCache {
    fn key(&self, key: String) -> String {
        format!("{}:{key}", self.generation.load(Ordering::Acquire))
    }
    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.responses.invalidate_all();
    }
}
fn cache_entry_weight(key: &str, cached: &CachedResponse, minimum_weight: u32) -> u32 {
    let header_bytes = cached.headers.iter().fold(0_u64, |total, header| {
        total.saturating_add((header.name.len() + header.value.len()) as u64)
    });
    u32::try_from(
        key.len() as u64
            + cached.body.len() as u64
            + header_bytes
            + std::mem::size_of::<CachedResponse>() as u64,
    )
    .unwrap_or(u32::MAX)
    .max(minimum_weight)
}
fn cache_key(route_id: &str, request: &PluginRequest) -> String {
    let mut key = format!("{}\n{}\n{}", route_id, request.method, request.path);
    if let Some(query) = &request.query {
        key.push('?');
        key.push_str(query);
    }
    for header in &request.headers {
        key.push('\n');
        key.push_str(&header.name);
        key.push(':');
        key.push_str(&header.value);
    }
    key
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc, time::Instant};

    use axum::{extract::Path, http::StatusCode};
    use bytes::Bytes;
    use nur_plugins::{CachePolicy, Header, Request as PluginRequest};

    use super::{
        CachedResponse, PluginCacheInvalidator, cache_entry_weight, cache_key, into_response_parts,
        is_plugin_write_method, plugin_path_params, request_identity, route_cache,
        validate_cached_request_body,
    };

    #[test]
    fn cached_response_body_clones_share_the_same_allocation() {
        let response = CachedResponse {
            status: 200,
            headers: Vec::new(),
            body: Bytes::from(vec![1, 2, 3]),
            expires_at: Instant::now(),
        };
        let clone = response.clone();

        assert_eq!(response.body.as_ptr(), clone.body.as_ptr());
    }

    #[test]
    fn public_routes_never_receive_an_authenticated_identity() {
        assert!(request_identity(false, 42, vec!["admin".into()]).is_none());
        let identity = request_identity(true, 42, vec!["admin".into()])
            .expect("protected route receives identity");
        assert_eq!(identity.user_id, 42);
    }

    #[test]
    fn path_parameters_are_forwarded_in_stable_order() {
        let params = plugin_path_params(Some(Path(HashMap::from([
            ("year".into(), "2026".into()),
            ("slug".into(), "summer-festival".into()),
        ]))));
        assert_eq!(params[0], ("slug".into(), "summer-festival".into()));
        assert_eq!(params[1], ("year".into(), "2026".into()));
        assert!(plugin_path_params(None).is_empty());
    }

    #[test]
    fn cache_invalidation_changes_the_generation() {
        let cache = route_cache(
            CachePolicy {
                ttl: std::time::Duration::from_secs(60),
                max_entries: 4,
            },
            4096,
        );
        let before = cache.key("home".into());
        PluginCacheInvalidator {
            caches: Arc::new(vec![cache.clone()]),
        }
        .invalidate();
        assert_ne!(before, cache.key("home".into()));
    }

    #[test]
    fn cache_weight_accounts_for_payload_headers_and_entry_limit() {
        let response = CachedResponse {
            status: 200,
            headers: vec![Header {
                name: "content-type".into(),
                value: "text/plain".into(),
            }],
            body: Bytes::from(vec![0; 256]),
            expires_at: Instant::now(),
        };

        assert!(cache_entry_weight("cache-key", &response, 1) >= 256);
        assert_eq!(cache_entry_weight("a", &response, 512), 512);
    }

    #[test]
    fn cache_keys_include_method_query_and_every_forwarded_header() {
        let request = |method: &str, query: Option<&str>, user_agent: &str| PluginRequest {
            method: method.into(),
            path: "/events".into(),
            path_params: Vec::new(),
            query: query.map(Into::into),
            headers: vec![
                Header {
                    name: "accept".into(),
                    value: "text/html".into(),
                },
                Header {
                    name: "user-agent".into(),
                    value: user_agent.into(),
                },
            ],
            body: Vec::new(),
            client_ip: None,
            identity: None,
        };

        let desktop = cache_key("events", &request("GET", Some("year=2026"), "desktop"));
        let mobile = cache_key("events", &request("GET", Some("year=2026"), "mobile"));
        let head = cache_key("events", &request("HEAD", Some("year=2026"), "mobile"));
        let other_query = cache_key("events", &request("GET", Some("year=2027"), "desktop"));

        assert_ne!(desktop, mobile);
        assert_ne!(mobile, head);
        assert_ne!(desktop, other_query);
    }

    #[test]
    fn cached_routes_reject_bodies_and_writes_trigger_invalidation() {
        assert_eq!(
            validate_cached_request_body(true, b"content"),
            Err(StatusCode::BAD_REQUEST)
        );
        assert!(validate_cached_request_body(true, &[]).is_ok());
        assert!(validate_cached_request_body(false, b"content").is_ok());
        assert!(is_plugin_write_method("POST"));
        assert!(is_plugin_write_method("DELETE"));
        assert!(!is_plugin_write_method("GET"));
    }

    #[test]
    fn forbidden_plugin_response_headers_are_removed() {
        let response = into_response_parts(
            200,
            vec![
                Header {
                    name: "set-cookie".into(),
                    value: "session=secret".into(),
                },
                Header {
                    name: "content-type".into(),
                    value: "text/plain".into(),
                },
            ],
            Bytes::new(),
        );
        assert!(!response.headers().contains_key("set-cookie"));
        assert_eq!(response.headers()["content-type"], "text/plain");
    }
}
