use std::{collections::HashSet, io::Error as IoError, time::Duration};

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Error as SqlxError, PgPool, Row, query, query_scalar};
use tokio::task::JoinError;
use wasmtime::{Error as WasmtimeError, Trap};

use nur_core::config::{mb, settings};

use self::{
    runtime::{PluginComponent, Runtime, bindings},
    utils::{
        manifest::{self, InstalledPlugin, RouteManifest, RouteScope},
        migrations,
    },
};

mod runtime;
mod utils;

pub use utils::{
    manifest::{AdminManifest, AdminMenuItem},
    storage::{BrowserUpload, PluginStorage, StorageDirectory, StorageVisibility, StoredFile},
    transport::{self, AssetDirectory, CachePolicy, Header, Identity, Request, Response, Route},
};

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
    Io(#[from] IoError),
    #[error(transparent)]
    Database(#[from] SqlxError),
    #[error(transparent)]
    Join(#[from] JoinError),
}

impl Error {
    fn wasmtime(error: WasmtimeError) -> Self {
        if error.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
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
    storage: Option<PluginStorage>,
    storage_directories: Vec<StorageDirectory>,
    pool: PgPool,
    storage_quota: u64,
    storage_write_limit: usize,
    timeout: Duration,
    request_body_limit: usize,
    response_body_limit: usize,
}

struct LoadedPlugins {
    routes: Vec<RegisteredRoute>,
    assets: Vec<AssetDirectory>,
    metadata: Vec<PluginMetadata>,
}

impl PluginManager {
    pub async fn load(pool: &PgPool) -> Result<Self, Error> {
        let installed = manifest::discover()?;
        let timeout = plugin_timeout();
        let request_body_limit = request_body_limit();
        let response_body_limit = response_body_limit();
        let storage_write_limit = storage_write_limit();

        if installed.is_empty() {
            return Ok(Self {
                routes: Vec::new(),
                assets: Vec::new(),
                metadata: Vec::new(),
                storage: None,
                storage_directories: Vec::new(),
                pool: pool.clone(),
                storage_quota: storage_quota(),
                storage_write_limit,
                timeout,
                request_body_limit,
                response_body_limit,
            });
        }

        let storage_directories: Vec<_> = installed
            .iter()
            .flat_map(|plugin| {
                let plugin_id = &plugin.manifest.plugin.id;
                plugin
                    .manifest
                    .storage
                    .directories
                    .iter()
                    .map(move |directory| StorageDirectory::from_manifest(plugin_id, directory))
            })
            .collect::<Result<_, _>>()?;

        let storage = (!storage_directories.is_empty())
            .then(PluginStorage::from_config)
            .transpose()?;

        if let Some(storage) = &storage {
            storage.cleanup_incomplete_uploads(Duration::from_secs(
                plugin_upload_session_seconds() as u64,
            ))?;
            for directory in &storage_directories {
                storage.validate_directory(directory)?;
            }
        }

        let runtime = Runtime::new(pool.clone(), storage.clone())?;
        let loaded = load_plugins(pool, installed, &runtime).await?;

        query(
            "DELETE FROM public.plugin_file_links \
             WHERE (upload_id IS NULL AND expires_at <= now()) \
                OR consumed_at <= now() - interval '1 day' \
                OR (upload_id IS NOT NULL AND consumed_at IS NULL \
                    AND claimed_at <= now() - make_interval(secs => $1))",
        )
        .bind(plugin_upload_session_seconds())
        .execute(pool)
        .await?;

        Ok(Self {
            routes: loaded.routes,
            assets: loaded.assets,
            metadata: loaded.metadata,
            storage,
            storage_directories,
            pool: pool.clone(),
            storage_quota: storage_quota(),
            storage_write_limit,
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

    pub fn storage_directory(
        &self,
        plugin_id: &str,
        directory_id: &str,
    ) -> Option<StorageDirectory> {
        self.storage_directories
            .iter()
            .find(|directory| directory.plugin_id == plugin_id && directory.id == directory_id)
            .cloned()
    }

    pub fn storage(&self) -> Option<&PluginStorage> {
        self.storage.as_ref()
    }

    pub fn storage_quota(&self) -> u64 {
        self.storage_quota
    }

    pub fn storage_write_limit(&self) -> usize {
        self.storage_write_limit
    }

    pub async fn download_link(
        &self,
        plugin_id: &str,
        token: &str,
    ) -> Result<(i64, StorageDirectory, String), Error> {
        if !valid_file_token(token) {
            return Err(Error::PluginNotFound);
        }

        let row = query(
            "SELECT id, directory_id, filename FROM public.plugin_file_links \
             WHERE plugin_id = $1 AND purpose = 'download' AND token_hash = $2 \
               AND consumed_at IS NULL AND expires_at > now()",
        )
        .bind(plugin_id)
        .bind(Sha256::digest(token.as_bytes()).to_vec())
        .fetch_optional(&self.pool)
        .await?;

        let row = row.ok_or(Error::PluginNotFound)?;
        let directory_id: String = row.try_get("directory_id")?;
        let path: String = row.try_get("filename")?;
        let directory = self
            .storage_directory(plugin_id, &directory_id)
            .ok_or(Error::PluginNotFound)?;

        Ok((row.try_get("id")?, directory, path))
    }

    pub async fn consume_download_link(&self, link_id: i64) -> Result<(), Error> {
        let result = query(
            "UPDATE public.plugin_file_links SET consumed_at = now() \
             WHERE id = $1 AND purpose = 'download' AND consumed_at IS NULL AND expires_at > now()",
        )
        .bind(link_id)
        .execute(&self.pool)
        .await?;

        (result.rows_affected() == 1)
            .then_some(())
            .ok_or(Error::PluginNotFound)
    }

    pub async fn reserve_upload_link(
        &self,
        plugin_id: &str,
        token: &str,
        upload_id: &str,
        total_size: u64,
    ) -> Result<(i64, StorageDirectory, String, u64, bool), Error> {
        if !valid_file_token(token)
            || upload_id.is_empty()
            || upload_id.len() > 128
            || !upload_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || total_size == 0
        {
            return Err(Error::PluginNotFound);
        }

        let total_size = i64::try_from(total_size).map_err(|_| Error::PluginNotFound)?;

        let directory_id: String = query_scalar(
            "SELECT directory_id FROM public.plugin_file_links \
             WHERE plugin_id = $1 AND purpose = 'upload' AND token_hash = $2",
        )
        .bind(plugin_id)
        .bind(Sha256::digest(token.as_bytes()).to_vec())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::PluginNotFound)?;

        let resolved_directory = self
            .storage_directory(plugin_id, &directory_id)
            .filter(|directory| directory.upload == BrowserUpload::Link)
            .ok_or(Error::PluginNotFound)?
            .resolved_for_upload();

        let row = query(
            "UPDATE public.plugin_file_links \
             SET upload_id = COALESCE(upload_id, $4), claimed_at = COALESCE(claimed_at, now()), \
                 storage_path = COALESCE(storage_path, $6) \
             WHERE plugin_id = $1 AND purpose = 'upload' AND token_hash = $2 \
               AND consumed_at IS NULL \
               AND ((upload_id IS NULL AND expires_at > now()) \
                    OR (upload_id = $4 AND claimed_at > now() - make_interval(secs => $5))) \
               AND (upload_id IS NULL OR upload_id = $4) \
               AND max_size IS NOT NULL AND max_size >= $3 \
             RETURNING id, filename, max_size, storage_path, finalizing_at IS NOT NULL AS finalizing",
        )
        .bind(plugin_id)
        .bind(Sha256::digest(token.as_bytes()).to_vec())
        .bind(total_size)
        .bind(upload_id)
        .bind(plugin_upload_session_seconds())
        .bind(&resolved_directory.path)
        .fetch_optional(&self.pool)
        .await?;

        let row = row.ok_or(Error::PluginNotFound)?;
        let link_id: i64 = row.try_get("id")?;
        let filename: String = row.try_get("filename")?;
        let maximum_size =
            u64::try_from(row.try_get::<i64, _>("max_size")?).map_err(|_| Error::InvalidValue)?;
        let mut directory = resolved_directory;
        directory.path = row.try_get("storage_path")?;

        Ok((
            link_id,
            directory,
            filename,
            maximum_size,
            row.try_get("finalizing")?,
        ))
    }

    pub async fn begin_upload_link_finalization(
        &self,
        link_id: i64,
        upload_id: &str,
    ) -> Result<(), Error> {
        let result = query(
            "UPDATE public.plugin_file_links SET finalizing_at = COALESCE(finalizing_at, now()) \
             WHERE id = $1 AND purpose = 'upload' AND upload_id = $2 AND consumed_at IS NULL",
        )
        .bind(link_id)
        .bind(upload_id)
        .execute(&self.pool)
        .await?;

        (result.rows_affected() == 1)
            .then_some(())
            .ok_or(Error::PluginNotFound)
    }

    pub async fn complete_upload_link(&self, link_id: i64, upload_id: &str) -> Result<(), Error> {
        let result = query(
            "UPDATE public.plugin_file_links SET consumed_at = now() \
             WHERE id = $1 AND purpose = 'upload' AND upload_id = $2 \
               AND finalizing_at IS NOT NULL AND consumed_at IS NULL",
        )
        .bind(link_id)
        .bind(upload_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(Error::PluginNotFound)
        }
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

async fn load_plugins(
    pool: &PgPool,
    installed: Vec<InstalledPlugin>,
    runtime: &Runtime,
) -> Result<LoadedPlugins, Error> {
    let mut routes = Vec::new();
    let mut assets = Vec::new();
    let mut metadata = Vec::new();
    let mut registered = HashSet::new();
    let allow_root = settings().plugins.allow_root_routes;

    for plugin in installed {
        migrations::migrate_plugin(pool, &plugin).await?;

        let plugin_id = plugin.manifest.plugin.id.clone();
        let component = runtime.load(&plugin)?;

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
            admin: plugin
                .manifest
                .admin
                .clone()
                .map(|admin| resolve_admin_manifest(&plugin_id, admin)),
        });

        let cache = plugin.manifest.cache.as_ref().map(|cache| CachePolicy {
            ttl: Duration::from_secs(cache.ttl_seconds),
            max_entries: cache.max_entries,
        });

        for route in &plugin.manifest.routes {
            let path = resolve_route_path(&plugin_id, route, allow_root)?;
            let key = (route.method.to_ascii_uppercase(), route_shape(&path)?);

            if !registered.insert(key) {
                return Err(Error::Manifest(format!(
                    "duplicate plugin route {} {}",
                    route.method, path
                )));
            }

            let roles = route.roles()?;
            let cache_enabled = route.cache_enabled(cache.is_some())?;
            let route = Route::new(
                routes.len(),
                plugin_id.clone(),
                route.id.clone(),
                route.method.to_ascii_uppercase(),
                path,
                roles,
                cache_enabled.then_some(cache).flatten(),
            );

            routes.push(RegisteredRoute {
                route,
                plugin: component.clone(),
            });
        }
    }

    Ok(LoadedPlugins {
        routes,
        assets,
        metadata,
    })
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

fn resolve_route_path(
    plugin_id: &str,
    route: &RouteManifest,
    allow_root: bool,
) -> Result<String, Error> {
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

    if route.scope == RouteScope::Plugin {
        if route.path.starts_with("/files/") {
            return Err(Error::Manifest(format!(
                "plugin '{plugin_id}' route '{}' uses the reserved plugin file API path",
                route.path
            )));
        }
        if ["/api/plugins", "/api/p"].iter().any(|prefix| {
            route.path == *prefix
                || route
                    .path
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        }) {
            return Err(Error::Manifest(format!(
                "plugin '{plugin_id}' route '{}' must use a path relative to its API namespace",
                route.path
            )));
        }
        let namespace = format!("/api/p/{plugin_id}");
        let path = if route.path == "/" {
            namespace
        } else {
            format!("{namespace}{}", route.path)
        };
        route_shape(&path)?;
        return Ok(path);
    }

    if !allow_root {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' root route '{}' requires plugins.allow_root_routes = true",
            route.path
        )));
    }

    if [
        "/auth", "/api", "/admin", "/sse", "/uploads", "/p", "/files",
    ]
    .iter()
    .any(|prefix| {
        route.path == *prefix
            || route
                .path
                .strip_prefix(prefix)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }) {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' route '{}' uses a reserved prefix",
            route.path
        )));
    }

    route_shape(&route.path)?;

    Ok(route.path.clone())
}

fn resolve_admin_manifest(plugin_id: &str, mut admin: AdminManifest) -> AdminManifest {
    let namespace = format!("/admin/p/{plugin_id}");
    for item in &mut admin.menu {
        item.path = if item.path == "/" {
            namespace.clone()
        } else {
            format!("{namespace}{}", item.path)
        };
    }

    admin
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

pub(crate) fn plugin_timeout() -> Duration {
    Duration::from_secs(settings().plugins.runtime.timeout_seconds)
}

fn request_body_limit() -> usize {
    mb(settings().plugins.runtime.request_body_limit_mb) as usize
}

fn response_body_limit() -> usize {
    mb(settings().plugins.runtime.response_body_limit_mb) as usize
}

fn storage_quota() -> u64 {
    mb(settings().plugins.storage.quota_mb)
}

fn storage_write_limit() -> usize {
    mb(settings().plugins.storage.write_limit_mb) as usize
}

fn plugin_upload_session_seconds() -> i32 {
    i32::try_from(settings().uploads.session_lifetime_hours * 60 * 60).unwrap_or(i32::MAX)
}

fn valid_file_token(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use sha2::{Digest, Sha256};
    use sqlx::{PgPool, migrate::Migrator, query};

    use super::{
        AdminManifest, AdminMenuItem, BrowserUpload, Error, PluginManager, RouteManifest,
        StorageDirectory, StorageVisibility, bindings, resolve_admin_manifest, resolve_route_path,
        route_shape, validate_response, visible_admin,
    };
    use crate::manifest::RouteScope;

    const MIGRATOR: Migrator = sqlx::migrate!("./migrations");

    fn route(path: &str) -> RouteManifest {
        RouteManifest {
            id: "route".into(),
            method: "GET".into(),
            path: path.into(),
            scope: RouteScope::Plugin,
            access: "public".into(),
            cache: None,
        }
    }

    #[test]
    fn plugin_routes_are_resolved_inside_their_namespace() {
        assert_eq!(
            resolve_route_path("example", &route("/items"), false).unwrap(),
            "/api/p/example/items"
        );
        assert_eq!(
            resolve_route_path("example", &route("/"), false).unwrap(),
            "/api/p/example"
        );
        assert!(
            resolve_route_path("example", &route("/api/plugins/example/items"), false).is_err()
        );
        assert!(resolve_route_path("example", &route("/api/p/example/items"), false).is_err());
        assert_eq!(
            resolve_route_path("example", &route("/files"), false).unwrap(),
            "/api/p/example/files"
        );
        assert!(resolve_route_path("example", &route("/files/upload/{token}"), false).is_err());
        assert!(resolve_route_path("example", &route("/files-example"), false).is_ok());
    }

    #[test]
    fn root_routes_require_permission_and_cannot_use_reserved_prefixes() {
        let mut route = route("/feed.xml");
        route.scope = RouteScope::Root;
        assert!(resolve_route_path("example", &route, false).is_err());
        assert_eq!(
            resolve_route_path("example", &route, true).unwrap(),
            "/feed.xml"
        );
        route.path = "/admin/plugin".into();
        assert!(resolve_route_path("example", &route, true).is_err());
        route.path = "/api/other".into();
        assert!(resolve_route_path("example", &route, true).is_err());
        route.path = "/p/example/assets".into();
        assert!(resolve_route_path("example", &route, true).is_err());
        route.path = "/files".into();
        assert!(resolve_route_path("example", &route, true).is_err());
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
                    path: "/admin/p/example/statistics".into(),
                    icon: None,
                    access: Some("admin,stat".into()),
                },
                AdminMenuItem {
                    label: "Products".into(),
                    labels: BTreeMap::new(),
                    path: "/admin/p/example/products".into(),
                    icon: None,
                    access: Some("admin".into()),
                },
            ],
        };

        let visible = visible_admin(Some(admin.clone()), "example", &["stat".into()])
            .expect("stat role can load the admin component");
        assert_eq!(visible.menu.len(), 1);
        assert_eq!(visible.menu[0].path, "/admin/p/example/statistics");
        assert!(visible_admin(Some(admin), "example", &["author".into()]).is_none());
    }

    #[test]
    fn manifest_routes_keep_the_existing_public_default() {
        let route: RouteManifest =
            toml_edit::de::from_str("id = 'test'\nmethod = 'GET'\npath = '/'\n")
                .expect("route manifest parses");
        assert_eq!(route.scope, RouteScope::Plugin);
        assert!(route.roles().expect("roles parse").is_empty());
    }

    #[test]
    fn admin_menu_paths_are_resolved_for_metadata() {
        let admin = AdminManifest {
            entry: Some("admin.js".into()),
            element: Some("example-admin".into()),
            access: "admin".into(),
            styles: Vec::new(),
            menu: vec![AdminMenuItem {
                label: "Overview".into(),
                labels: BTreeMap::new(),
                path: "/overview".into(),
                icon: None,
                access: None,
            }],
        };
        let resolved = resolve_admin_manifest("example", admin);
        assert_eq!(resolved.menu[0].path, "/admin/p/example/overview");
    }

    #[test]
    fn epoch_interrupts_are_reported_as_timeouts() {
        assert!(matches!(
            Error::wasmtime(wasmtime::Trap::Interrupt.into()),
            Error::Timeout
        ));
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    #[ignore = "requires PostgreSQL via DATABASE_URL"]
    async fn upload_links_are_bound_to_one_session_and_consumed_once(pool: PgPool) {
        query(
            "INSERT INTO public.plugin_registry \
             (plugin_id, version, api_version, schema_name, manifest_checksum) \
             VALUES ('example', '0.1.0', 1, 'nur_plugin_example', $1)",
        )
        .bind(vec![0_u8])
        .execute(&pool)
        .await
        .expect("plugin registry row can be inserted");
        let token = "0123456789abcdef0123456789abcdef";
        query(
            "INSERT INTO public.plugin_file_links \
             (plugin_id, directory_id, purpose, token_hash, filename, max_size, expires_at) \
             VALUES ('example', 'documents', 'upload', $1, 'report.pdf', 1024, \
                     now() + interval '10 minutes')",
        )
        .bind(Sha256::digest(token.as_bytes()).to_vec())
        .execute(&pool)
        .await
        .expect("upload link can be inserted");
        let manager = PluginManager {
            routes: Vec::new(),
            assets: Vec::new(),
            metadata: Vec::new(),
            storage: None,
            storage_directories: vec![StorageDirectory {
                plugin_id: "example".into(),
                id: "documents".into(),
                path: "documents/{year}/{month}".into(),
                extensions: vec!["pdf".into()],
                visibility: StorageVisibility::Private,
                upload: BrowserUpload::Link,
                roles: vec!["admin".into()],
            }],
            pool,
            storage_quota: 1024,
            storage_write_limit: 1024,
            timeout: Duration::from_secs(1),
            request_body_limit: 1024,
            response_body_limit: 1024,
        };

        let download_token = "abcdefabcdefabcdefabcdefabcdefab";
        query(
            "INSERT INTO public.plugin_file_links \
             (plugin_id, directory_id, purpose, token_hash, filename, expires_at) \
             VALUES ('example', 'documents', 'download', $1, 'documents/2026/09/report.pdf', \
                     now() + interval '10 minutes')",
        )
        .bind(Sha256::digest(download_token.as_bytes()).to_vec())
        .execute(&manager.pool)
        .await
        .expect("download link can be inserted");
        let (download_id, _, _) = manager
            .download_link("example", download_token)
            .await
            .expect("looking up a download does not consume it");
        manager
            .download_link("example", download_token)
            .await
            .expect("the link remains active until the file has been opened");
        manager
            .consume_download_link(download_id)
            .await
            .expect("an opened download can consume the link");
        assert!(
            manager
                .download_link("example", download_token)
                .await
                .is_err()
        );

        let (link_id, directory, filename, maximum_size, finalizing) = manager
            .reserve_upload_link("example", token, "session-one", 512)
            .await
            .expect("first session can reserve the link");
        assert_eq!(filename, "report.pdf");
        assert_eq!(maximum_size, 1024);
        assert!(!finalizing);
        assert!(!directory.path.contains("{year}"));
        assert!(!directory.path.contains("{month}"));
        assert!(
            manager
                .reserve_upload_link("example", token, "session-two", 512)
                .await
                .is_err(),
            "a second session cannot take over the capability"
        );
        manager
            .begin_upload_link_finalization(link_id, "session-one")
            .await
            .expect("the owning session can begin finalization");
        let (_, _, _, _, finalizing) = manager
            .reserve_upload_link("example", token, "session-one", 512)
            .await
            .expect("the finalizing session remains resumable");
        assert!(finalizing);
        manager
            .complete_upload_link(link_id, "session-one")
            .await
            .expect("the owning session can consume the link");
        assert!(
            manager
                .reserve_upload_link("example", token, "session-one", 512)
                .await
                .is_err(),
            "a consumed upload capability cannot be reused"
        );
    }
}
