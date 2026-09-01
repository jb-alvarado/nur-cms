use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Extension, Path, Request, State},
    http::{HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{MethodFilter, get, on},
};
use moka::sync::Cache;
use nur_core::db::models::{AuthUserMeta, Role};
use protect_axum::authorities::AuthDetails;
use real::RealIp;
use serde::Serialize;
use sqlx::PgPool;
use tower_http::{services::ServeDir, timeout::TimeoutLayer};
use tracing::{error, info};

mod manifest;
mod migrations;
mod runtime;

use manifest::{AdminManifest, CacheManifest, InstalledPlugin, RouteManifest};
use runtime::{PluginComponent, Runtime, bindings};

pub const API_VERSION: u32 = 1;
const FORWARDED_REQUEST_HEADERS: &[&str] =
    &["accept", "accept-language", "content-type", "user-agent"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("plugin manifest error: {0}")]
    Manifest(String),
    #[error("plugin migration error: {0}")]
    Migration(String),
    #[error("plugin runtime error: {0}")]
    Plugin(String),
    #[error("plugin rejected the request: {0}")]
    PluginBadRequest(String),
    #[error("plugin denied the request")]
    PluginForbidden,
    #[error("plugin resource was not found")]
    PluginNotFound,
    #[error("plugin timed out")]
    Timeout,
    #[error("plugin runtime is busy")]
    Busy,
    #[error("plugin mail rate limit exceeded")]
    RateLimited,
    #[error("invalid plugin value")]
    InvalidValue,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
}

impl Error {
    fn wasmtime(error: wasmtime::Error) -> Self {
        Self::Plugin(error.to_string())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PluginMetadata {
    pub id: String,
    pub version: String,
    pub admin: Option<AdminManifest>,
}

pub struct PluginManager {
    plugins: Vec<LoadedPlugin>,
    metadata: Arc<Vec<PluginMetadata>>,
}

#[derive(Clone, Default)]
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

struct LoadedPlugin {
    installed: InstalledPlugin,
    component: PluginComponent,
    cache: Option<RouteCache>,
}

#[derive(Clone)]
struct RouteState {
    plugin: PluginComponent,
    route_id: String,
    roles: Vec<String>,
    request_body_limit: usize,
    response_body_limit: usize,
    cache: Option<RouteCache>,
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
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    expires_at: Instant,
}

impl PluginManager {
    pub async fn load(pool: &PgPool) -> Result<Self, Error> {
        let installed = manifest::discover()?;
        if installed.is_empty() {
            return Ok(Self {
                plugins: Vec::new(),
                metadata: Arc::new(Vec::new()),
            });
        }

        let runtime = Runtime::new(pool.clone())?;
        let configured_caches = installed
            .iter()
            .filter(|plugin| plugin.manifest.cache.is_some())
            .count();
        let cache_capacity = plugin_cache_capacity(configured_caches);
        let mut plugins = Vec::with_capacity(installed.len());
        let mut metadata = Vec::with_capacity(installed.len());
        for plugin in installed {
            migrations::migrate_plugin(pool, &plugin).await?;
            let component = runtime.load(&plugin)?;
            info!(plugin = %plugin.manifest.plugin.id, "loaded plugin");
            metadata.push(PluginMetadata {
                id: plugin.manifest.plugin.id.clone(),
                version: plugin.manifest.plugin.version.clone(),
                admin: plugin.manifest.admin.clone(),
            });
            let cache = plugin_cache(plugin.manifest.cache.as_ref(), cache_capacity);
            plugins.push(LoadedPlugin {
                installed: plugin,
                component,
                cache,
            });
        }

        Ok(Self {
            plugins,
            metadata: Arc::new(metadata),
        })
    }

    pub fn router(&self) -> Result<Router, Error> {
        let mut router = Router::new().route(
            "/api/plugins",
            get(plugin_index).with_state(Arc::clone(&self.metadata)),
        );
        let mut registered = HashSet::new();
        let allow_root = std::env::var("NUR_PLUGIN_ALLOW_ROOT_ROUTES").as_deref() == Ok("1");
        let request_body_limit = env_usize(
            "NUR_PLUGIN_REQUEST_BODY_LIMIT",
            1024 * 1024,
            1024,
            16 * 1024 * 1024,
        );
        let response_body_limit = env_usize(
            "NUR_PLUGIN_RESPONSE_BODY_LIMIT",
            4 * 1024 * 1024,
            1024,
            64 * 1024 * 1024,
        );

        for plugin in &self.plugins {
            if let Some(assets) = &plugin.installed.assets {
                let path = format!("/plugins/{}/assets", plugin.installed.manifest.plugin.id);
                router = router.nest_service(&path, ServeDir::new(assets));
            }
            for route in &plugin.installed.manifest.routes {
                validate_route(&plugin.installed.manifest.plugin.id, route, allow_root)?;
                let method = method_filter(&route.method)?;
                let key = (route.method.to_ascii_uppercase(), route_shape(&route.path)?);
                if !registered.insert(key) {
                    return Err(Error::Manifest(format!(
                        "duplicate plugin route {} {}",
                        route.method, route.path
                    )));
                }
                let state = Arc::new(RouteState {
                    plugin: plugin.component.clone(),
                    route_id: route.id.clone(),
                    roles: route.roles()?,
                    request_body_limit,
                    response_body_limit,
                    cache: route
                        .cache_enabled(plugin.cache.is_some())?
                        .then(|| plugin.cache.clone())
                        .flatten(),
                });
                let route_router = Router::new()
                    .route(&route.path, on(method, dispatch))
                    .with_state(state);
                router = router.merge(route_router);
            }
        }
        Ok(router.layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            plugin_timeout() + Duration::from_millis(250),
        )))
    }

    pub fn cache_invalidator(&self) -> PluginCacheInvalidator {
        PluginCacheInvalidator {
            caches: Arc::new(
                self.plugins
                    .iter()
                    .filter_map(|plugin| plugin.cache.clone())
                    .collect(),
            ),
        }
    }
}

async fn plugin_index(
    State(metadata): State<Arc<Vec<PluginMetadata>>>,
    details: AuthDetails<Role>,
) -> Response {
    let role_names: Vec<String> = details
        .authorities
        .iter()
        .filter(|role| !matches!(role, Role::Guest))
        .map(ToString::to_string)
        .collect();
    if role_names.is_empty() {
        return StatusCode::FORBIDDEN.into_response();
    }

    let visible: Vec<_> = metadata
        .iter()
        .cloned()
        .map(|mut plugin| {
            let allowed = plugin.admin.as_ref().is_some_and(|admin| {
                admin
                    .roles(&plugin.id)
                    .is_ok_and(|required| required.iter().any(|role| role_names.contains(role)))
            });
            if !allowed {
                plugin.admin = None;
            }
            plugin
        })
        .collect();
    Json(visible).into_response()
}

async fn dispatch(
    State(state): State<Arc<RouteState>>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Extension(real_ip): Extension<RealIp>,
    path_params: Option<Path<HashMap<String, String>>>,
    request: Request,
) -> Response {
    let role_names: Vec<String> = details
        .authorities
        .iter()
        .map(ToString::to_string)
        .collect();
    if !state.roles.is_empty()
        && !role_names
            .iter()
            .any(|role| state.roles.iter().any(|required| required == role))
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    let path_params = plugin_path_params(path_params);

    match dispatch_inner(&state, user, role_names, path_params, real_ip.ip(), request).await {
        Ok(response) => response,
        Err(error) => {
            error!(plugin = %state.plugin.id, route = %state.route_id, %error, "plugin request failed");
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
    }
}

async fn dispatch_inner(
    state: &RouteState,
    user: AuthUserMeta,
    roles: Vec<String>,
    path_params: Vec<bindings::nur::cms::types::PathParam>,
    client_ip: std::net::IpAddr,
    request: Request,
) -> Result<Response, Error> {
    let method = request.method().to_string();
    let uri = request.uri().clone();
    let headers = request_headers(request.headers());
    let cache_key = state.cache.as_ref().map(|cache| {
        cache.key(response_cache_key(
            &state.route_id,
            request.method(),
            &uri,
            &headers,
        ))
    });
    if let (Some(cache), Some(key)) = (&state.cache, &cache_key)
        && let Some(cached) = cache.responses.get(key)
    {
        if cached.expires_at > Instant::now() {
            return build_response(cached.into_plugin_response(), state.response_body_limit);
        }
        cache.responses.invalidate(key);
    }
    let body = to_bytes(request.into_body(), state.request_body_limit)
        .await
        .map_err(|error| Error::Plugin(error.to_string()))?;
    let identity = request_identity(state.roles.is_empty(), user, roles);
    let plugin_request = bindings::nur::cms::types::Request {
        route_id: state.route_id.clone(),
        method,
        path: uri.path().into(),
        path_params,
        query: uri.query().map(ToOwned::to_owned),
        headers,
        body: body.to_vec(),
        identity,
    };
    let response = state
        .plugin
        .call(plugin_request, state.roles.is_empty(), client_ip)
        .await?;
    if response.status == StatusCode::OK.as_u16()
        && response.body.len() <= state.response_body_limit
        && let (Some(cache), Some(key)) = (&state.cache, cache_key)
    {
        cache.responses.insert(
            key,
            CachedResponse::from_plugin_response(&response, cache.ttl),
        );
    }
    build_response(response, state.response_body_limit)
}

fn plugin_path_params(
    path_params: Option<Path<HashMap<String, String>>>,
) -> Vec<bindings::nur::cms::types::PathParam> {
    let Some(Path(path_params)) = path_params else {
        return Vec::new();
    };
    let mut path_params: Vec<_> = path_params
        .into_iter()
        .map(|(name, value)| bindings::nur::cms::types::PathParam { name, value })
        .collect();
    path_params.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    path_params
}

impl CachedResponse {
    fn from_plugin_response(response: &bindings::nur::cms::types::Response, ttl: Duration) -> Self {
        Self {
            status: response.status,
            headers: response
                .headers
                .iter()
                .map(|header| (header.name.clone(), header.value.clone()))
                .collect(),
            body: response.body.clone(),
            expires_at: Instant::now() + ttl,
        }
    }

    fn into_plugin_response(self) -> bindings::nur::cms::types::Response {
        bindings::nur::cms::types::Response {
            status: self.status,
            headers: self
                .headers
                .into_iter()
                .map(|(name, value)| bindings::nur::cms::types::Header { name, value })
                .collect(),
            body: self.body,
        }
    }
}

fn plugin_cache(cache: Option<&CacheManifest>, capacity: u64) -> Option<RouteCache> {
    cache.map(|cache| RouteCache {
        responses: Cache::builder()
            .max_capacity(capacity)
            .weigher(cache_weigher(capacity, cache.max_entries))
            .build(),
        ttl: Duration::from_secs(cache.ttl_seconds),
        generation: Arc::new(AtomicU64::new(0)),
    })
}

fn plugin_cache_capacity(configured_caches: usize) -> u64 {
    if configured_caches == 0 {
        return 0;
    }
    env_u64(
        "NUR_PLUGIN_CACHE_MEMORY_LIMIT",
        64 * 1024 * 1024,
        1024 * 1024,
        1024 * 1024 * 1024,
    ) / u64::try_from(configured_caches).unwrap_or(u64::MAX)
}

fn cache_weigher(
    capacity: u64,
    max_entries: u64,
) -> impl Fn(&String, &CachedResponse) -> u32 + Send + Sync + 'static {
    let minimum_weight = capacity
        .div_ceil(max_entries.max(1))
        .clamp(1, u64::from(u32::MAX)) as u32;
    move |key, response| cache_entry_weight(key, response).max(minimum_weight)
}

fn cache_entry_weight(key: &str, response: &CachedResponse) -> u32 {
    let header_bytes = response.headers.iter().fold(0_u64, |total, (name, value)| {
        total.saturating_add(name.len() as u64 + value.len() as u64)
    });
    let bytes = key.len() as u64
        + response.body.len() as u64
        + header_bytes
        + std::mem::size_of::<CachedResponse>() as u64;
    u32::try_from(bytes).unwrap_or(u32::MAX)
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

fn response_cache_key(
    route_id: &str,
    method: &Method,
    uri: &axum::http::Uri,
    headers: &[bindings::nur::cms::types::Header],
) -> String {
    let mut key = format!("{route_id}\n{method}\n{uri}");
    for header in headers {
        key.push('\n');
        key.push_str(&header.name);
        key.push(':');
        key.push_str(&header.value.len().to_string());
        key.push(':');
        key.push_str(&header.value);
    }

    key
}

fn request_headers(headers: &axum::http::HeaderMap) -> Vec<bindings::nur::cms::types::Header> {
    headers
        .iter()
        .filter(|(name, _)| FORWARDED_REQUEST_HEADERS.contains(&name.as_str()))
        .take(32)
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .filter(|value| value.len() <= 8 * 1024)
                .map(|value| bindings::nur::cms::types::Header {
                    name: name.to_string(),
                    value: value.to_string(),
                })
        })
        .collect()
}

fn request_identity(
    public_route: bool,
    user: AuthUserMeta,
    roles: Vec<String>,
) -> Option<bindings::nur::cms::types::Identity> {
    (!public_route && user.id >= 0).then_some(bindings::nur::cms::types::Identity {
        user_id: user.id,
        roles,
    })
}

fn build_response(
    response: bindings::nur::cms::types::Response,
    body_limit: usize,
) -> Result<Response, Error> {
    if response.body.len() > body_limit {
        return Err(Error::Plugin("plugin response body exceeds limit".into()));
    }
    let status = StatusCode::from_u16(response.status)
        .map_err(|_| Error::Plugin("plugin returned an invalid status".into()))?;
    let mut builder = Response::builder().status(status);
    for header in response.headers.into_iter().take(64) {
        let name = HeaderName::try_from(header.name)
            .map_err(|_| Error::Plugin("plugin returned an invalid header name".into()))?;
        if forbidden_response_header(&name) {
            continue;
        }
        let value = HeaderValue::try_from(header.value)
            .map_err(|_| Error::Plugin("plugin returned an invalid header value".into()))?;
        builder = builder.header(name, value);
    }
    builder
        .body(Body::from(response.body))
        .map_err(|error| Error::Plugin(error.to_string()))
}

fn forbidden_response_header(name: &HeaderName) -> bool {
    matches!(
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
    )
}

fn validate_route(plugin_id: &str, route: &RouteManifest, allow_root: bool) -> Result<(), Error> {
    if !route.path.starts_with('/')
        || route.path.contains("//")
        || route.path.contains("..")
        || route.path.contains("{*")
    {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' has invalid route path '{}'",
            route.path
        )));
    }
    route_shape(&route.path)?;
    let namespace = format!("/api/plugins/{plugin_id}");
    let namespaced = route.path == namespace
        || route
            .path
            .strip_prefix(&namespace)
            .is_some_and(|suffix| suffix.starts_with('/'));
    if namespaced {
        return Ok(());
    }
    if !allow_root {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' root route '{}' requires NUR_PLUGIN_ALLOW_ROOT_ROUTES=1",
            route.path
        )));
    }
    if ["/auth", "/api", "/admin", "/sse", "/uploads"]
        .iter()
        .any(|prefix| {
            route.path == *prefix
                || route
                    .path
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
    {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' route '{}' uses a reserved prefix",
            route.path
        )));
    }
    Ok(())
}

fn route_shape(path: &str) -> Result<String, Error> {
    path.split('/')
        .map(|segment| {
            if segment.starts_with('{') || segment.ends_with('}') {
                if segment.len() < 3
                    || !segment.starts_with('{')
                    || !segment.ends_with('}')
                    || segment[1..segment.len() - 1].contains(['{', '}'])
                {
                    return Err(Error::Manifest(format!(
                        "invalid plugin route parameter in '{path}'"
                    )));
                }
                Ok("{}")
            } else if segment.contains(['{', '}']) {
                Err(Error::Manifest(format!(
                    "invalid plugin route parameter in '{path}'"
                )))
            } else {
                Ok(segment)
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|segments| segments.join("/"))
}

fn method_filter(method: &str) -> Result<MethodFilter, Error> {
    match Method::from_bytes(method.as_bytes())
        .map_err(|_| Error::Manifest(format!("invalid plugin HTTP method '{method}'")))?
    {
        Method::GET => Ok(MethodFilter::GET),
        Method::POST => Ok(MethodFilter::POST),
        Method::PUT => Ok(MethodFilter::PUT),
        Method::PATCH => Ok(MethodFilter::PATCH),
        Method::DELETE => Ok(MethodFilter::DELETE),
        Method::HEAD => Ok(MethodFilter::HEAD),
        Method::OPTIONS => Ok(MethodFilter::OPTIONS),
        _ => Err(Error::Manifest(format!(
            "unsupported plugin HTTP method '{method}'"
        ))),
    }
}

fn env_usize(key: &str, default: usize, minimum: usize, maximum: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64, minimum: u64, maximum: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .unwrap_or(default)
}

fn plugin_timeout() -> Duration {
    Duration::from_millis(env_u64("NUR_PLUGIN_TIMEOUT_MS", 5_000, 100, 60_000))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use axum::extract::Path;
    use axum::http::{HeaderMap, HeaderValue, Method, Uri};
    use nur_core::db::models::AuthUserMeta;

    use super::{
        CachedResponse, PluginCacheInvalidator, RouteManifest, cache_entry_weight, cache_weigher,
        plugin_cache, plugin_path_params, request_identity, response_cache_key, route_shape,
        validate_route,
    };
    use crate::manifest::CacheManifest;

    fn route(path: &str) -> RouteManifest {
        RouteManifest {
            id: "route".into(),
            method: "GET".into(),
            path: path.into(),
            access: "public".into(),
            cache: None,
        }
    }

    #[test]
    fn allows_own_namespace_without_root_permission() {
        assert!(validate_route("example", &route("/api/plugins/example/items"), false).is_ok());
        assert!(validate_route("example", &route("/api/plugins/other/items"), false).is_err());
    }

    #[test]
    fn root_routes_require_permission_and_cannot_use_reserved_prefixes() {
        assert!(validate_route("example", &route("/feed.xml"), false).is_err());
        assert!(validate_route("example", &route("/feed.xml"), true).is_ok());
        assert!(validate_route("example", &route("/admin/plugin"), true).is_err());
    }

    #[test]
    fn route_shapes_detect_parameter_name_conflicts() {
        assert_eq!(route_shape("/items/{id}").unwrap(), "/items/{}");
        assert_eq!(route_shape("/items/{slug}").unwrap(), "/items/{}");
        assert!(route_shape("/items/{broken").is_err());
    }

    #[test]
    fn path_parameters_are_forwarded_by_name() {
        let params = plugin_path_params(Some(Path(HashMap::from([
            ("slug".into(), "summer-festival".into()),
            ("year".into(), "2026".into()),
        ]))));

        assert_eq!(params[0].name, "slug");
        assert_eq!(params[0].value, "summer-festival");
        assert_eq!(params[1].name, "year");
        assert_eq!(params[1].value, "2026");
        assert!(plugin_path_params(None).is_empty());
    }

    #[test]
    fn invalidating_plugin_caches_changes_the_cache_generation() {
        let cache = plugin_cache(
            Some(&CacheManifest {
                ttl_seconds: 60,
                max_entries: 1,
            }),
            1024,
        )
        .expect("cache is configured");
        let before = cache.key("home".into());

        PluginCacheInvalidator {
            caches: Arc::new(vec![cache.clone()]),
        }
        .invalidate();

        let after = cache.key("home".into());
        assert_ne!(before, after);
    }

    #[test]
    fn cache_weight_accounts_for_payload_headers_and_entry_limit() {
        let response = CachedResponse {
            status: 200,
            headers: vec![("content-type".into(), "text/plain".into())],
            body: vec![0; 256],
            expires_at: std::time::Instant::now(),
        };
        assert!(cache_entry_weight("cache-key", &response) >= 256);

        let weigher = cache_weigher(1024, 2);
        assert_eq!(weigher(&"a".into(), &response), 512);
    }

    #[test]
    fn public_routes_never_receive_an_authenticated_identity() {
        assert!(request_identity(true, AuthUserMeta::new(42), vec!["admin".into()]).is_none());

        let identity = request_identity(false, AuthUserMeta::new(42), vec!["admin".into()])
            .expect("protected route receives identity");
        assert_eq!(identity.user_id, 42);
    }

    #[test]
    fn cache_keys_include_method_and_every_forwarded_header() {
        let uri: Uri = "/events?year=2026".parse().expect("URI is valid");
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("text/html"));
        headers.insert("user-agent", HeaderValue::from_static("desktop"));
        let desktop = response_cache_key(
            "events",
            &Method::GET,
            &uri,
            &super::request_headers(&headers),
        );

        headers.insert("user-agent", HeaderValue::from_static("mobile"));
        let forwarded = super::request_headers(&headers);
        let mobile = response_cache_key("events", &Method::GET, &uri, &forwarded);
        let head = response_cache_key("events", &Method::HEAD, &uri, &forwarded);

        assert_ne!(desktop, mobile);
        assert_ne!(mobile, head);
    }
}
