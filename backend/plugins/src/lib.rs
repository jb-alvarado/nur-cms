use std::{collections::HashSet, sync::Arc};

use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Extension, Request, State},
    http::{HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{MethodFilter, get, on},
};
use nur_core::db::models::{AuthUserMeta, Role};
use protect_axum::authorities::AuthDetails;
use serde::Serialize;
use sqlx::PgPool;
use tracing::{error, info};

mod manifest;
mod migrations;
mod runtime;

use manifest::{AdminManifest, InstalledPlugin, RouteManifest};
use runtime::{PluginComponent, Runtime, bindings};

pub const API_VERSION: u32 = 1;

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

struct LoadedPlugin {
    installed: InstalledPlugin,
    component: PluginComponent,
}

#[derive(Clone)]
struct RouteState {
    plugin: PluginComponent,
    route_id: String,
    roles: Vec<String>,
    request_body_limit: usize,
    response_body_limit: usize,
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

        let runtime = Runtime::new()?;
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
            plugins.push(LoadedPlugin {
                installed: plugin,
                component,
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
                });
                let route_router = Router::new()
                    .route(&route.path, on(method, dispatch))
                    .with_state(state);
                router = router.merge(route_router);
            }
        }
        Ok(router)
    }
}

async fn plugin_index(
    State(metadata): State<Arc<Vec<PluginMetadata>>>,
    details: AuthDetails<Role>,
) -> Response {
    if !details.authorities.contains(&Role::Admin) && !details.authorities.contains(&Role::Author) {
        return StatusCode::FORBIDDEN.into_response();
    }
    Json(&*metadata).into_response()
}

async fn dispatch(
    State(state): State<Arc<RouteState>>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
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

    match dispatch_inner(&state, user, role_names, request).await {
        Ok(response) => response,
        Err(error) => {
            error!(plugin = %state.plugin.id, route = %state.route_id, %error, "plugin request failed");
            match error {
                Error::Timeout => StatusCode::GATEWAY_TIMEOUT.into_response(),
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
    request: Request,
) -> Result<Response, Error> {
    let method = request.method().to_string();
    let uri = request.uri().clone();
    let headers = request_headers(request.headers());
    let body = to_bytes(request.into_body(), state.request_body_limit)
        .await
        .map_err(|error| Error::Plugin(error.to_string()))?;
    let identity = (user.id >= 0).then_some(bindings::nur::cms::types::Identity {
        user_id: user.id,
        roles,
    });
    let plugin_request = bindings::nur::cms::types::Request {
        route_id: state.route_id.clone(),
        method,
        path: uri.path().into(),
        query: uri.query().map(ToOwned::to_owned),
        headers,
        body: body.to_vec(),
        identity,
    };
    let response = state.plugin.call(plugin_request).await?;
    build_response(response, state.response_body_limit)
}

fn request_headers(headers: &axum::http::HeaderMap) -> Vec<bindings::nur::cms::types::Header> {
    const ALLOWED: &[&str] = &["accept", "accept-language", "content-type", "user-agent"];
    headers
        .iter()
        .filter(|(name, _)| ALLOWED.contains(&name.as_str()))
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

#[cfg(test)]
mod tests {
    use super::{RouteManifest, route_shape, validate_route};

    fn route(path: &str) -> RouteManifest {
        RouteManifest {
            id: "route".into(),
            method: "GET".into(),
            path: path.into(),
            access: "public".into(),
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
}
