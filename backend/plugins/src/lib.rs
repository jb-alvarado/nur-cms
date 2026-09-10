use std::{collections::HashSet, time::Duration};

use serde::Serialize;
use sqlx::PgPool;

mod manifest;
mod migrations;
mod runtime;
pub mod transport;

use manifest::RouteManifest;
pub use manifest::{AdminManifest, AdminMenuItem};
use runtime::{PluginComponent, Runtime, bindings};
pub use transport::{AssetDirectory, CachePolicy, Header, Identity, Request, Response, Route};

pub const API_VERSION: u32 = 1;
pub const MAX_RESPONSE_HEADERS: usize = 64;
pub const MAX_RESPONSE_HEADER_BYTES: usize = 64 * 1024;
pub const MAX_RESPONSE_HEADER_VALUE_BYTES: usize = 8 * 1024;
const MAX_ROUTE_PATH_BYTES: usize = 2 * 1024;
const MAX_ROUTE_PARAMS: usize = 16;

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
        if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt) {
            Self::Timeout
        } else {
            Self::Plugin(error.to_string())
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub admin: Option<AdminManifest>,
}

struct RegisteredRoute {
    route: Route,
    plugin: PluginComponent,
}

pub struct PluginManager {
    routes: Vec<RegisteredRoute>,
    assets: Vec<AssetDirectory>,
    metadata: Vec<PluginMetadata>,
    timeout: Duration,
    request_body_limit: usize,
    response_body_limit: usize,
}

impl PluginManager {
    pub async fn load(pool: &PgPool) -> Result<Self, Error> {
        let installed = manifest::discover()?;
        let timeout = plugin_timeout();
        let request_body_limit = request_body_limit();
        let response_body_limit = response_body_limit();
        if installed.is_empty() {
            return Ok(Self {
                routes: Vec::new(),
                assets: Vec::new(),
                metadata: Vec::new(),
                timeout,
                request_body_limit,
                response_body_limit,
            });
        }
        let runtime = Runtime::new(pool.clone())?;
        let mut routes = Vec::new();
        let mut assets = Vec::new();
        let mut metadata = Vec::new();
        let mut registered = HashSet::new();
        let allow_root = std::env::var("NUR_PLUGIN_ALLOW_ROOT_ROUTES").as_deref() == Ok("1");

        for plugin in installed {
            migrations::migrate_plugin(pool, &plugin).await?;
            let component = runtime.load(&plugin)?;
            let plugin_id = plugin.manifest.plugin.id.clone();
            if let Some(path) = plugin.assets.clone() {
                assets.push(AssetDirectory {
                    plugin_id: plugin_id.clone(),
                    path,
                });
            }
            metadata.push(PluginMetadata {
                id: plugin_id.clone(),
                name: plugin
                    .manifest
                    .plugin
                    .name
                    .clone()
                    .unwrap_or_else(|| plugin_id.clone()),
                version: plugin.manifest.plugin.version.clone(),
                admin: plugin.manifest.admin.clone(),
            });
            let cache = plugin.manifest.cache.as_ref().map(|cache| CachePolicy {
                ttl: Duration::from_secs(cache.ttl_seconds),
                max_entries: cache.max_entries,
            });
            for route in &plugin.manifest.routes {
                validate_route(&plugin_id, route, allow_root)?;
                let key = (route.method.to_ascii_uppercase(), route_shape(&route.path)?);
                if !registered.insert(key) {
                    return Err(Error::Manifest(format!(
                        "duplicate plugin route {} {}",
                        route.method, route.path
                    )));
                }
                let roles = route.roles()?;
                let cache_enabled = route.cache_enabled(cache.is_some())?;
                let route = Route::new(
                    routes.len(),
                    plugin_id.clone(),
                    route.id.clone(),
                    route.method.to_ascii_uppercase(),
                    route.path.clone(),
                    roles,
                    cache_enabled.then_some(cache).flatten(),
                );
                routes.push(RegisteredRoute {
                    route,
                    plugin: component.clone(),
                });
            }
        }
        Ok(Self {
            routes,
            assets,
            metadata,
            timeout,
            request_body_limit,
            response_body_limit,
        })
    }

    pub fn routes(&self) -> Vec<Route> {
        self.routes
            .iter()
            .map(|route| route.route.clone())
            .collect()
    }
    pub fn assets(&self) -> &[AssetDirectory] {
        &self.assets
    }
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
    pub fn request_body_limit(&self) -> usize {
        self.request_body_limit
    }
    pub fn metadata(&self) -> &[PluginMetadata] {
        &self.metadata
    }

    pub fn visible_metadata(&self, roles: &[String]) -> Vec<PluginMetadata> {
        self.metadata
            .iter()
            .cloned()
            .map(|mut plugin| {
                plugin.admin = visible_admin(plugin.admin, &plugin.id, roles);
                plugin
            })
            .collect()
    }

    pub fn authorized(&self, route: &Route, identity: Option<&Identity>) -> bool {
        let Some(registered) = self.routes.get(route.key()) else {
            return false;
        };
        registered.route.roles.is_empty()
            || identity.is_some_and(|identity| {
                identity
                    .roles
                    .iter()
                    .any(|role| registered.route.roles.contains(role))
            })
    }

    pub async fn dispatch(&self, route: &Route, request: Request) -> Result<Response, Error> {
        let registered = self.routes.get(route.key()).ok_or(Error::PluginNotFound)?;
        if !self.authorized(route, request.identity.as_ref()) {
            return Err(Error::PluginForbidden);
        }
        if request.body.len() > self.request_body_limit {
            return Err(Error::PluginBadRequest(
                "plugin request body exceeds limit".into(),
            ));
        }
        let response = registered
            .plugin
            .call(
                bindings::nur::cms::types::Request {
                    route_id: registered.route.id.clone(),
                    method: request.method,
                    path: request.path,
                    path_params: request
                        .path_params
                        .into_iter()
                        .map(|(name, value)| bindings::nur::cms::types::PathParam { name, value })
                        .collect(),
                    query: request.query,
                    headers: request
                        .headers
                        .into_iter()
                        .map(|header| bindings::nur::cms::types::Header {
                            name: header.name,
                            value: header.value,
                        })
                        .collect(),
                    body: request.body,
                    identity: request.identity.map(|identity| {
                        bindings::nur::cms::types::Identity {
                            user_id: identity.user_id,
                            roles: identity.roles,
                        }
                    }),
                },
                registered.route.roles.is_empty(),
                request.client_ip,
            )
            .await?;
        validate_response(&response, self.response_body_limit)?;
        Ok(Response {
            status: response.status,
            headers: response
                .headers
                .into_iter()
                .map(|header| Header {
                    name: header.name,
                    value: header.value,
                })
                .collect(),
            body: response.body,
        })
    }
}

fn visible_admin(
    admin: Option<AdminManifest>,
    plugin_id: &str,
    user_roles: &[String],
) -> Option<AdminManifest> {
    let mut admin = admin?;
    if !admin
        .roles(plugin_id)
        .is_ok_and(|required| required.iter().any(|role| user_roles.contains(role)))
    {
        return None;
    }
    let menu = admin.menu.clone();
    admin.menu = menu
        .into_iter()
        .filter(|item| {
            admin
                .menu_roles(item, plugin_id)
                .is_ok_and(|required| required.iter().any(|role| user_roles.contains(role)))
        })
        .collect();
    Some(admin)
}

fn validate_response(
    response: &bindings::nur::cms::types::Response,
    body_limit: usize,
) -> Result<(), Error> {
    if !(100..=599).contains(&response.status)
        || response.body.len() > body_limit
        || response.headers.len() > MAX_RESPONSE_HEADERS
    {
        return Err(Error::Plugin("plugin returned an invalid response".into()));
    }
    let mut bytes = 0usize;
    for header in &response.headers {
        if header.name.is_empty()
            || !header
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || header
                .value
                .bytes()
                .any(|byte| byte.is_ascii_control() && byte != b'\t')
            || header.value.len() > MAX_RESPONSE_HEADER_VALUE_BYTES
        {
            return Err(Error::Plugin(
                "plugin returned invalid response headers".into(),
            ));
        }
        bytes = bytes
            .checked_add(header.name.len() + header.value.len())
            .ok_or_else(|| Error::Plugin("plugin response headers exceed limit".into()))?;
    }
    if bytes > MAX_RESPONSE_HEADER_BYTES {
        return Err(Error::Plugin("plugin response headers exceed limit".into()));
    }
    Ok(())
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
    if path.len() > MAX_ROUTE_PATH_BYTES
        || path.chars().any(char::is_control)
        || path.contains(['?', '#'])
    {
        return Err(Error::Manifest(
            "plugin route path is invalid or too long".into(),
        ));
    }
    let mut params = HashSet::new();
    let mut shape = Vec::new();
    for segment in path.split('/') {
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
            let name = &segment[1..segment.len() - 1];
            if !(1..=64).contains(&name.len())
                || !name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                || !name.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                || !params.insert(name)
                || params.len() > MAX_ROUTE_PARAMS
            {
                return Err(Error::Manifest(format!(
                    "invalid or duplicate plugin route parameter in '{path}'"
                )));
            }
            shape.push("{}");
        } else {
            if segment.contains(['{', '}']) || segment.starts_with(':') || segment.starts_with('*')
            {
                return Err(Error::Manifest(format!(
                    "invalid plugin route path '{path}'"
                )));
            }
            shape.push(segment);
        }
    }
    Ok(shape.join("/"))
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
pub(crate) fn plugin_timeout() -> Duration {
    Duration::from_millis(env_u64("NUR_PLUGIN_TIMEOUT_MS", 5_000, 100, 60_000))
}
fn request_body_limit() -> usize {
    env_usize(
        "NUR_PLUGIN_REQUEST_BODY_LIMIT",
        1024 * 1024,
        1024,
        16 * 1024 * 1024,
    )
}
fn response_body_limit() -> usize {
    env_usize(
        "NUR_PLUGIN_RESPONSE_BODY_LIMIT",
        4 * 1024 * 1024,
        1024,
        64 * 1024 * 1024,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        AdminManifest, AdminMenuItem, Error, RouteManifest, bindings, route_shape,
        validate_response, validate_route, visible_admin,
    };

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
    fn routes_require_their_own_namespace_without_root_permission() {
        assert!(validate_route("example", &route("/api/plugins/example/items"), false).is_ok());
        assert!(validate_route("example", &route("/api/plugins/other/items"), false).is_err());
    }

    #[test]
    fn root_routes_require_permission_and_cannot_use_reserved_prefixes() {
        assert!(validate_route("example", &route("/feed.xml"), false).is_err());
        assert!(validate_route("example", &route("/feed.xml"), true).is_ok());
        assert!(validate_route("example", &route("/admin/plugin"), true).is_err());
        assert!(validate_route("example", &route("/api/other"), true).is_err());
    }

    #[test]
    fn normalizes_route_shapes_without_accepting_invalid_parameters() {
        assert_eq!(route_shape("/items/{id}").unwrap(), "/items/{}");
        assert_eq!(route_shape("/items/{slug}").unwrap(), "/items/{}");
        assert!(route_shape("/items/{broken").is_err());
        assert!(route_shape("/items/:legacy").is_err());
        assert!(route_shape("/items/*legacy").is_err());
        assert!(route_shape("/items/{invalid*}").is_err());
        assert!(route_shape("/items/{id}/{id}").is_err());
        assert!(route_shape("/items?format=json").is_err());
    }

    #[test]
    fn rejects_invalid_plugin_response_headers_before_the_http_adapter() {
        let response = bindings::nur::cms::types::Response {
            status: 200,
            headers: vec![bindings::nur::cms::types::Header {
                name: "x-test".into(),
                value: "line\r\nbreak".into(),
            }],
            body: Vec::new(),
        };
        assert!(matches!(
            validate_response(&response, 1024),
            Err(Error::Plugin(_))
        ));
    }

    #[test]
    fn rejects_oversized_plugin_response_headers() {
        let response = bindings::nur::cms::types::Response {
            status: 200,
            headers: vec![bindings::nur::cms::types::Header {
                name: "x-plugin-value".into(),
                value: "x".repeat(super::MAX_RESPONSE_HEADER_VALUE_BYTES + 1),
            }],
            body: Vec::new(),
        };
        assert!(validate_response(&response, 1024).is_err());
    }

    #[test]
    fn admin_metadata_is_filtered_for_the_current_roles() {
        let admin = AdminManifest {
            entry: Some("admin.js".into()),
            element: Some("example-admin".into()),
            access: "admin,stat".into(),
            styles: Vec::new(),
            menu: vec![
                AdminMenuItem {
                    label: "Statistics".into(),
                    labels: BTreeMap::new(),
                    path: "/admin/plugins/example/statistics".into(),
                    icon: None,
                    access: Some("admin,stat".into()),
                },
                AdminMenuItem {
                    label: "Products".into(),
                    labels: BTreeMap::new(),
                    path: "/admin/plugins/example/products".into(),
                    icon: None,
                    access: Some("admin".into()),
                },
            ],
        };

        let visible = visible_admin(Some(admin.clone()), "example", &["stat".into()])
            .expect("stat role can load the admin component");
        assert_eq!(visible.menu.len(), 1);
        assert_eq!(visible.menu[0].path, "/admin/plugins/example/statistics");
        assert!(visible_admin(Some(admin), "example", &["author".into()]).is_none());
    }

    #[test]
    fn manifest_routes_keep_the_existing_public_default() {
        let route: RouteManifest =
            toml_edit::de::from_str("id = 'test'\nmethod = 'GET'\npath = '/api/plugins/test'\n")
                .expect("route manifest parses");
        assert!(route.roles().expect("roles parse").is_empty());
    }

    #[test]
    fn epoch_interrupts_are_reported_as_timeouts() {
        assert!(matches!(
            Error::wasmtime(wasmtime::Trap::Interrupt.into()),
            Error::Timeout
        ));
    }
}
