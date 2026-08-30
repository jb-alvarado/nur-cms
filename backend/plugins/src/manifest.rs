use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{API_VERSION, Error};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub plugin: PluginManifest,
    #[serde(default)]
    pub migrations: MigrationManifest,
    #[serde(default)]
    pub routes: Vec<RouteManifest>,
    pub assets: Option<AssetsManifest>,
    pub cache: Option<CacheManifest>,
    pub admin: Option<AdminManifest>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub api_version: u32,
    pub cms_version: String,
    pub module: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationManifest {
    pub directory: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteManifest {
    pub id: String,
    pub method: String,
    pub path: String,
    #[serde(default = "public_access")]
    pub access: String,
    #[serde(default)]
    pub cache: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetsManifest {
    pub directory: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheManifest {
    pub ttl_seconds: u64,
    pub max_entries: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdminManifest {
    pub entry: Option<String>,
    #[serde(default)]
    pub menu: Vec<AdminMenuItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdminMenuItem {
    pub label: String,
    pub path: String,
    pub icon: Option<String>,
}

#[derive(Clone, Debug)]
pub struct InstalledPlugin {
    pub manifest: Manifest,
    pub root: PathBuf,
    pub module: PathBuf,
    pub assets: Option<PathBuf>,
    pub manifest_checksum: Vec<u8>,
}

fn public_access() -> String {
    "public".into()
}

impl RouteManifest {
    pub fn roles(&self) -> Result<Vec<String>, Error> {
        let roles: Vec<String> = self
            .access
            .split(',')
            .map(str::trim)
            .filter(|role| !role.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        if roles.is_empty() {
            return Err(Error::Manifest(format!(
                "route '{}' has an empty access declaration",
                self.id
            )));
        }
        if roles.iter().any(|role| role == "public") {
            if roles.len() != 1 {
                return Err(Error::Manifest(format!(
                    "route '{}' cannot combine public with authenticated roles",
                    self.id
                )));
            }
            return Ok(Vec::new());
        }
        if roles.iter().any(|role| !valid_role(role)) {
            return Err(Error::Manifest(format!(
                "route '{}' contains an invalid access role",
                self.id
            )));
        }

        let mut unique = HashSet::new();
        Ok(roles
            .into_iter()
            .filter(|role| unique.insert(role.clone()))
            .collect())
    }

    pub fn cache_enabled(&self, plugin_cache_enabled: bool) -> Result<bool, Error> {
        let public_get = self.roles()?.is_empty() && matches!(self.method.as_str(), "GET" | "HEAD");
        match self.cache {
            Some(true) if !plugin_cache_enabled || !public_get => Err(Error::Manifest(format!(
                "route '{}' can be cached only when it is a public GET or HEAD route and the plugin has a [cache] section",
                self.id
            ))),
            Some(true) => Ok(true),
            Some(false) => Ok(false),
            None => Ok(plugin_cache_enabled && public_get),
        }
    }
}

pub fn discover() -> Result<Vec<InstalledPlugin>, Error> {
    let enabled = enabled_plugins();
    if enabled.is_empty() {
        return Ok(Vec::new());
    }
    if enabled.len() > 32 {
        return Err(Error::Manifest(
            "no more than 32 plugins can be enabled".into(),
        ));
    }
    if let Some(id) = enabled.iter().find(|id| !valid_plugin_id(id)) {
        return Err(Error::Manifest(format!(
            "invalid enabled plugin id '{id}'; use 3-40 lowercase letters, digits, and hyphens"
        )));
    }

    let mut discovered = HashMap::new();
    for plugin_root in plugin_roots() {
        if !plugin_root.is_dir() {
            continue;
        }
        for enabled_id in &enabled {
            let root = plugin_root.join(enabled_id);
            let manifest_path = root.join("plugin.toml");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest_size = fs::metadata(&manifest_path).map_err(Error::Io)?.len();
            if manifest_size > 256 * 1024 {
                return Err(Error::Manifest(format!(
                    "plugin manifest is too large: {}",
                    manifest_path.display()
                )));
            }
            let bytes = fs::read(&manifest_path).map_err(Error::Io)?;
            let source = std::str::from_utf8(&bytes).map_err(|error| {
                Error::Manifest(format!("{}: {error}", manifest_path.display()))
            })?;
            let manifest: Manifest = toml_edit::de::from_str(source).map_err(|error| {
                Error::Manifest(format!("{}: {error}", manifest_path.display()))
            })?;
            validate_manifest(&manifest)?;
            if manifest.plugin.id != *enabled_id {
                return Err(Error::Manifest(format!(
                    "plugin directory '{enabled_id}' contains manifest for '{}'",
                    manifest.plugin.id
                )));
            }
            let root = fs::canonicalize(root).map_err(Error::Io)?;
            let module = contained_path(&root, &manifest.plugin.module, "module")?;
            if !module.is_file() {
                return Err(Error::Manifest(format!(
                    "plugin '{}' module does not exist: {}",
                    manifest.plugin.id,
                    module.display()
                )));
            }
            let assets = manifest
                .assets
                .as_ref()
                .map(|assets| contained_path(&root, &assets.directory, "asset directory"))
                .transpose()?;
            if assets.as_ref().is_some_and(|assets| !assets.is_dir()) {
                return Err(Error::Manifest(format!(
                    "plugin '{}' asset directory does not exist",
                    manifest.plugin.id
                )));
            }
            if let Some(assets) = &assets {
                validate_asset_tree(assets, &manifest.plugin.id)?;
            }
            let plugin = InstalledPlugin {
                manifest,
                root,
                module,
                assets,
                manifest_checksum: Sha256::digest(bytes).to_vec(),
            };
            let id = plugin.manifest.plugin.id.clone();
            if discovered.insert(id.clone(), plugin).is_some() {
                return Err(Error::Manifest(format!(
                    "plugin '{id}' occurs in more than one plugin root"
                )));
            }
        }
    }

    let mut missing: Vec<_> = enabled
        .iter()
        .filter(|id| !discovered.contains_key(*id))
        .cloned()
        .collect();
    missing.sort();
    if !missing.is_empty() {
        return Err(Error::Manifest(format!(
            "enabled plugins were not found: {}",
            missing.join(", ")
        )));
    }

    let mut plugins: Vec<_> = discovered.into_values().collect();
    plugins.sort_by(|left, right| left.manifest.plugin.id.cmp(&right.manifest.plugin.id));
    Ok(plugins)
}

fn validate_asset_tree(root: &Path, plugin_id: &str) -> Result<(), Error> {
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).map_err(Error::Io)? {
            let entry = entry.map_err(Error::Io)?;
            let file_type = entry.file_type().map_err(Error::Io)?;
            if file_type.is_symlink() {
                return Err(Error::Manifest(format!(
                    "plugin '{plugin_id}' asset directory contains a symbolic link: {}",
                    entry.path().display()
                )));
            }
            if file_type.is_dir() {
                directories.push(entry.path());
            } else if !file_type.is_file() {
                return Err(Error::Manifest(format!(
                    "plugin '{plugin_id}' asset directory contains an unsupported file type: {}",
                    entry.path().display()
                )));
            }
        }
    }

    Ok(())
}

pub fn schema_name(plugin_id: &str) -> String {
    format!("nur_plugin_{}", plugin_id.replace('-', "_"))
}

pub fn contained_path(root: &Path, relative: &str, kind: &str) -> Result<PathBuf, Error> {
    let path = fs::canonicalize(root.join(relative)).map_err(Error::Io)?;
    if !path.starts_with(root) {
        return Err(Error::Manifest(format!(
            "plugin {kind} must stay inside {}",
            root.display()
        )));
    }
    Ok(path)
}

fn validate_manifest(manifest: &Manifest) -> Result<(), Error> {
    let plugin = &manifest.plugin;
    if !valid_plugin_id(&plugin.id) {
        return Err(Error::Manifest(format!(
            "invalid plugin id '{}'; use 3-40 lowercase letters, digits, and hyphens",
            plugin.id
        )));
    }
    Version::parse(&plugin.version).map_err(|error| {
        Error::Manifest(format!(
            "plugin '{}' has invalid version: {error}",
            plugin.id
        ))
    })?;
    if plugin.api_version != API_VERSION {
        return Err(Error::Manifest(format!(
            "plugin '{}' requires unsupported API version {}",
            plugin.id, plugin.api_version
        )));
    }
    if let Some(cache) = &manifest.cache
        && (!(1..=86_400).contains(&cache.ttl_seconds)
            || !(1..=10_000).contains(&cache.max_entries))
    {
        return Err(Error::Manifest(format!(
            "plugin '{}' cache settings are outside the supported limits",
            plugin.id
        )));
    }
    let requirement = VersionReq::parse(&plugin.cms_version).map_err(|error| {
        Error::Manifest(format!(
            "plugin '{}' has invalid cms_version: {error}",
            plugin.id
        ))
    })?;
    let cms_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| Error::Manifest(error.to_string()))?;
    if !requirement.matches(&cms_version) {
        return Err(Error::Manifest(format!(
            "plugin '{}' does not support nur-cms {cms_version}",
            plugin.id
        )));
    }

    let mut route_ids = HashSet::new();
    if manifest.routes.len() > 64 {
        return Err(Error::Manifest(format!(
            "plugin '{}' declares more than 64 routes",
            plugin.id
        )));
    }
    for route in &manifest.routes {
        if !valid_route_id(&route.id) || !route_ids.insert(&route.id) {
            return Err(Error::Manifest(format!(
                "plugin '{}' has an invalid or duplicate route id",
                plugin.id
            )));
        }
        route.cache_enabled(manifest.cache.is_some())?;
    }
    if manifest.assets.is_some()
        && manifest.routes.iter().any(|route| {
            let assets_path = format!("/plugins/{}/assets", plugin.id);
            route.path == assets_path
                || route
                    .path
                    .strip_prefix(&assets_path)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
    {
        return Err(Error::Manifest(format!(
            "plugin '{}' declares a route inside its reserved asset path",
            plugin.id
        )));
    }
    Ok(())
}

fn valid_plugin_id(id: &str) -> bool {
    (3..=40).contains(&id.len())
        && id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_role(role: &str) -> bool {
    (1..=40).contains(&role.len())
        && role
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_route_id(id: &str) -> bool {
    (1..=80).contains(&id.len())
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

fn enabled_plugins() -> HashSet<String> {
    env::var("NUR_PLUGINS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn plugin_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = env::var_os("NUR_PLUGIN_DIR")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default();
    if cfg!(debug_assertions) {
        roots.push(PathBuf::from("backend/plugins/examples"));
    }
    #[cfg(target_os = "linux")]
    roots.extend([
        PathBuf::from("/usr/share/nur-cms/plugins"),
        PathBuf::from("/var/lib/nur-cms/plugins"),
    ]);

    let mut unique = HashSet::new();
    roots.retain(|root| {
        let key = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        unique.insert(key)
    });
    roots
}

#[cfg(test)]
mod tests {
    use super::{
        CacheManifest, RouteManifest, schema_name, valid_plugin_id, valid_route_id,
        validate_asset_tree,
    };

    fn route(access: &str) -> RouteManifest {
        RouteManifest {
            id: "test".into(),
            method: "GET".into(),
            path: "/api/plugins/test".into(),
            access: access.into(),
            cache: None,
        }
    }

    #[test]
    fn accepts_single_and_multiple_roles() {
        assert_eq!(route("author").roles().unwrap(), ["author"]);
        assert_eq!(route("admin,author").roles().unwrap(), ["admin", "author"]);
        assert!(route("public").roles().unwrap().is_empty());
    }

    #[test]
    fn rejects_public_combined_with_roles() {
        assert!(route("public,author").roles().is_err());
    }

    #[test]
    fn plugin_ids_produce_distinct_safe_schema_names() {
        assert!(valid_plugin_id("my-plugin"));
        assert!(!valid_plugin_id("my_plugin"));
        assert_eq!(schema_name("my-plugin"), "nur_plugin_my_plugin");
    }

    #[test]
    fn route_ids_are_bounded_and_log_safe() {
        assert!(valid_route_id("public-feed.v1"));
        assert!(!valid_route_id("line\nbreak"));
        assert!(!valid_route_id(&"x".repeat(81)));
    }

    #[cfg(unix)]
    #[test]
    fn asset_trees_reject_symbolic_links() {
        use std::{fs, os::unix::fs::symlink, time::SystemTime};

        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system clock is after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nur-cms-plugin-assets-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("test asset directory is created");
        fs::write(root.join("site.css"), "body {}").expect("regular test asset is created");
        assert!(validate_asset_tree(&root, "example").is_ok());

        symlink("/etc/passwd", root.join("outside")).expect("test symbolic link is created");
        assert!(validate_asset_tree(&root, "example").is_err());

        fs::remove_dir_all(root).expect("test asset directory is removed");
    }

    #[test]
    fn plugin_cache_has_bounded_settings() {
        let cache = CacheManifest {
            ttl_seconds: 300,
            max_entries: 128,
        };
        assert!((1..=86_400).contains(&cache.ttl_seconds));
        assert!((1..=10_000).contains(&cache.max_entries));
    }

    #[test]
    fn caches_only_eligible_routes_by_default() {
        assert!(route("public").cache_enabled(true).unwrap());
        assert!(!route("author").cache_enabled(true).unwrap());

        let mut post_route = route("public");
        post_route.method = "POST".into();
        assert!(!post_route.cache_enabled(true).unwrap());

        let mut protected_cached = route("author");
        protected_cached.cache = Some(true);
        assert!(protected_cached.cache_enabled(true).is_err());
    }
}
