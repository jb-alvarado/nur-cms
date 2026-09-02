use std::{
    collections::{BTreeMap, HashMap, HashSet},
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
    pub mail: MailManifest,
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
    pub name: Option<String>,
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

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailManifest {
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub dynamic_recipient_targets: Vec<String>,
    #[serde(default)]
    pub trusted_template_targets: Vec<String>,
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
    pub element: Option<String>,
    #[serde(default = "admin_access")]
    pub access: String,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub menu: Vec<AdminMenuItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdminMenuItem {
    pub label: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub path: String,
    pub icon: Option<String>,
    pub access: Option<String>,
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

fn admin_access() -> String {
    "admin,author".into()
}

impl RouteManifest {
    pub fn roles(&self) -> Result<Vec<String>, Error> {
        parse_access(&self.access, &format!("route '{}'", self.id), true)
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

impl AdminManifest {
    pub fn roles(&self, plugin_id: &str) -> Result<Vec<String>, Error> {
        parse_access(
            &self.access,
            &format!("plugin '{plugin_id}' admin component"),
            false,
        )
    }

    pub fn menu_roles(&self, item: &AdminMenuItem, plugin_id: &str) -> Result<Vec<String>, Error> {
        match &item.access {
            Some(access) => parse_access(
                access,
                &format!("plugin '{plugin_id}' admin menu item '{}'", item.path),
                false,
            ),
            None => self.roles(plugin_id),
        }
    }

    fn validate_menu_access(&self, plugin_id: &str) -> Result<(), Error> {
        let admin_roles = self.roles(plugin_id)?;
        for item in &self.menu {
            let menu_roles = self.menu_roles(item, plugin_id)?;
            if menu_roles.iter().any(|role| !admin_roles.contains(role)) {
                return Err(Error::Manifest(format!(
                    "plugin '{plugin_id}' admin menu item '{}' uses roles outside admin.access",
                    item.path
                )));
            }
        }
        Ok(())
    }
}

fn parse_access(access: &str, context: &str, allow_public: bool) -> Result<Vec<String>, Error> {
    let roles: Vec<String> = access
        .split(',')
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if roles.is_empty() {
        return Err(Error::Manifest(format!(
            "{context} has an empty access declaration"
        )));
    }
    if roles.iter().any(|role| role == "public") {
        if !allow_public || roles.len() != 1 {
            return Err(Error::Manifest(format!(
                "{context} cannot use public together with authenticated access"
            )));
        }
        return Ok(Vec::new());
    }
    if roles.iter().any(|role| !valid_role(role)) {
        return Err(Error::Manifest(format!(
            "{context} contains an invalid access role"
        )));
    }

    let mut unique = HashSet::new();
    Ok(roles
        .into_iter()
        .filter(|role| unique.insert(role.clone()))
        .collect())
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
            if manifest
                .admin
                .as_ref()
                .and_then(|admin| admin.entry.as_ref())
                .is_some()
                && env::var("NUR_PLUGIN_ALLOW_ADMIN_COMPONENTS").as_deref() != Ok("1")
            {
                return Err(Error::Manifest(format!(
                    "plugin '{}' declares browser-side admin code; set NUR_PLUGIN_ALLOW_ADMIN_COMPONENTS=1 to trust and enable it",
                    manifest.plugin.id
                )));
            }
            if manifest.plugin.id != *enabled_id {
                return Err(Error::Manifest(format!(
                    "plugin directory '{enabled_id}' contains manifest for '{}'",
                    manifest.plugin.id
                )));
            }
            let root = fs::canonicalize(root).map_err(Error::Io)?;
            let module = contained_path(
                &root,
                &manifest.plugin.module,
                &manifest.plugin.id,
                "module",
            )?;
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
                .map(|assets| {
                    contained_path(
                        &root,
                        &assets.directory,
                        &manifest.plugin.id,
                        "asset directory",
                    )
                })
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
            validate_admin_assets(&manifest, assets.as_deref())?;
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
    let mut elements = HashSet::new();
    for plugin in &plugins {
        if let Some(element) = plugin
            .manifest
            .admin
            .as_ref()
            .and_then(|admin| admin.element.as_ref())
            && !elements.insert(element)
        {
            return Err(Error::Manifest(format!(
                "admin custom element '{element}' is declared by more than one plugin"
            )));
        }
    }
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

pub fn contained_path(
    root: &Path,
    relative: &str,
    plugin_id: &str,
    kind: &str,
) -> Result<PathBuf, Error> {
    let expected_path = root.join(relative);
    let path = fs::canonicalize(&expected_path).map_err(|error| {
        Error::Manifest(format!(
            "plugin '{plugin_id}' {kind} cannot be accessed at '{}': {error}",
            expected_path.display()
        ))
    })?;
    if !path.starts_with(root) {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' {kind} must stay inside {}",
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
    if plugin
        .name
        .as_ref()
        .is_some_and(|name| !valid_plugin_name(name))
    {
        return Err(Error::Manifest(format!(
            "plugin '{}' has an invalid display name",
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
    validate_mail_permissions(&manifest.mail, &plugin.id)?;
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
    if let Some(admin) = &manifest.admin {
        admin.roles(&plugin.id)?;
        let unique_styles: HashSet<_> = admin.styles.iter().collect();
        if admin.styles.len() > 16
            || unique_styles.len() != admin.styles.len()
            || admin.styles.iter().any(|style| !valid_admin_style(style))
        {
            return Err(Error::Manifest(format!(
                "plugin '{}' has invalid or duplicate admin styles",
                plugin.id
            )));
        }
        match (&admin.entry, &admin.element) {
            (Some(entry), Some(element))
                if valid_admin_entry(entry) && valid_custom_element_name(element) => {}
            (Some(_), Some(_)) => {
                return Err(Error::Manifest(format!(
                    "plugin '{}' has an invalid admin entry or custom element name",
                    plugin.id
                )));
            }
            (None, None) if admin.menu.is_empty() && admin.styles.is_empty() => {}
            (None, None) => {
                return Err(Error::Manifest(format!(
                    "plugin '{}' declares admin menu items without an admin entry and custom element",
                    plugin.id
                )));
            }
            _ => {
                return Err(Error::Manifest(format!(
                    "plugin '{}' must declare both admin entry and custom element",
                    plugin.id
                )));
            }
        }
        if admin.menu.len() > 32
            || admin
                .menu
                .iter()
                .any(|item| !valid_admin_menu_item(item, &plugin.id))
        {
            return Err(Error::Manifest(format!(
                "plugin '{}' has invalid admin menu metadata",
                plugin.id
            )));
        }
        admin.validate_menu_access(&plugin.id)?;
    }
    Ok(())
}

fn validate_mail_permissions(mail: &MailManifest, plugin_id: &str) -> Result<(), Error> {
    if mail.targets.len() > 32
        || mail.dynamic_recipient_targets.len() > 32
        || mail.trusted_template_targets.len() > 32
    {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' declares more than 32 mail targets"
        )));
    }
    let targets: HashSet<_> = mail.targets.iter().collect();
    if targets.len() != mail.targets.len()
        || mail.targets.iter().any(|target| !valid_mail_target(target))
    {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' has an invalid or duplicate mail target"
        )));
    }
    let dynamic_targets: HashSet<_> = mail.dynamic_recipient_targets.iter().collect();
    if dynamic_targets.len() != mail.dynamic_recipient_targets.len()
        || mail
            .dynamic_recipient_targets
            .iter()
            .any(|target| !targets.contains(target))
    {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' dynamic recipient targets must be unique declared mail targets"
        )));
    }
    let trusted_template_targets: HashSet<_> = mail.trusted_template_targets.iter().collect();
    if trusted_template_targets.len() != mail.trusted_template_targets.len()
        || mail
            .trusted_template_targets
            .iter()
            .any(|target| !targets.contains(target))
    {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' trusted template targets must be unique declared mail targets"
        )));
    }
    Ok(())
}

fn valid_mail_target(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= 160
        && target.trim() == target
        && !target.chars().any(char::is_control)
}

fn validate_admin_assets(manifest: &Manifest, assets: Option<&Path>) -> Result<(), Error> {
    let Some(admin) = &manifest.admin else {
        return Ok(());
    };
    if admin.entry.is_none() && admin.styles.is_empty() {
        return Ok(());
    }
    let assets = assets.ok_or_else(|| {
        Error::Manifest(format!(
            "plugin '{}' declares an admin entry without an asset directory",
            manifest.plugin.id
        ))
    })?;
    for (path, kind) in admin
        .entry
        .iter()
        .map(|entry| (entry, "admin entry"))
        .chain(admin.styles.iter().map(|style| (style, "admin stylesheet")))
    {
        let path = contained_path(assets, path, &manifest.plugin.id, kind)?;
        if !path.is_file() {
            return Err(Error::Manifest(format!(
                "plugin '{}' {kind} does not exist: {}",
                manifest.plugin.id,
                path.display()
            )));
        }
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

fn valid_plugin_name(name: &str) -> bool {
    (1..=80).contains(&name.chars().count())
        && name.trim() == name
        && !name.chars().any(char::is_control)
}

fn valid_role(role: &str) -> bool {
    (1..=40).contains(&role.len())
        && role
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_admin_entry(entry: &str) -> bool {
    valid_admin_asset_path(entry)
        && matches!(
            Path::new(entry)
                .extension()
                .and_then(|value| value.to_str()),
            Some("js" | "mjs")
        )
}

fn valid_admin_style(style: &str) -> bool {
    valid_admin_asset_path(style)
        && Path::new(style)
            .extension()
            .and_then(|value| value.to_str())
            == Some("css")
}

fn valid_admin_asset_path(entry: &str) -> bool {
    !entry.is_empty()
        && entry.len() <= 512
        && entry.split('/').all(|segment| {
            !segment.is_empty()
                && segment.len() <= 128
                && !matches!(segment, "." | "..")
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
}

fn valid_custom_element_name(name: &str) -> bool {
    (3..=80).contains(&name.len())
        && name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name.contains('-')
        && !name.ends_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !matches!(
            name,
            "annotation-xml"
                | "color-profile"
                | "font-face"
                | "font-face-src"
                | "font-face-uri"
                | "font-face-format"
                | "font-face-name"
                | "missing-glyph"
        )
}

fn valid_admin_menu_item(item: &AdminMenuItem, plugin_id: &str) -> bool {
    let namespace = format!("/admin/plugins/{plugin_id}");
    !item.label.is_empty()
        && item.label.len() <= 80
        && !item.label.chars().any(char::is_control)
        && item.labels.len() <= 16
        && item.labels.iter().all(|(locale, label)| {
            (2..=16).contains(&locale.len())
                && locale
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                && !label.is_empty()
                && label.len() <= 80
                && !label.chars().any(char::is_control)
        })
        && item.path.len() <= 512
        && (item.path == namespace
            || item
                .path
                .strip_prefix(&namespace)
                .is_some_and(|suffix| suffix.starts_with('/')))
        && !item.path.contains("//")
        && !item
            .path
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
        && item.icon.as_ref().is_none_or(|icon| {
            (3..=80).contains(&icon.len())
                && icon.starts_with("bi-")
                && icon
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
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
    use std::collections::BTreeMap;

    use super::{
        AdminManifest, AdminMenuItem, CacheManifest, MailManifest, Manifest, RouteManifest,
        contained_path, schema_name, valid_admin_entry, valid_admin_menu_item, valid_admin_style,
        valid_custom_element_name, valid_plugin_id, valid_plugin_name, valid_route_id,
        validate_asset_tree, validate_mail_permissions, validate_manifest,
    };

    #[test]
    fn missing_plugin_paths_include_context() {
        let root = std::env::temp_dir();
        let relative = format!("nur-cms-missing-plugin-path-{}", std::process::id());
        let error = contained_path(&root, &relative, "example", "module")
            .expect_err("missing path is rejected")
            .to_string();

        assert!(error.contains("plugin 'example' module"));
        assert!(error.contains(&root.join(relative).display().to_string()));
    }

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
    fn admin_entries_and_custom_element_names_are_restricted() {
        assert!(valid_admin_entry("admin/echo.js"));
        assert!(valid_admin_entry("admin/echo.mjs"));
        assert!(!valid_admin_entry("admin/echo.css"));
        assert!(!valid_admin_entry("../echo.js"));
        assert!(valid_admin_style("admin/echo.css"));
        assert!(!valid_admin_style("admin/echo.js"));
        assert!(valid_custom_element_name("nur-cms-echo"));
        assert!(!valid_custom_element_name("NurCmsEcho"));
        assert!(!valid_custom_element_name("font-face"));
    }

    #[test]
    fn admin_access_requires_authenticated_roles() {
        let mut admin = AdminManifest {
            entry: Some("admin.js".into()),
            element: Some("nur-cms-test".into()),
            access: "admin,author".into(),
            styles: Vec::new(),
            menu: Vec::new(),
        };
        assert_eq!(admin.roles("test").unwrap(), ["admin", "author"]);

        admin.access = "public".into();
        assert!(admin.roles("test").is_err());
    }

    #[test]
    fn admin_menu_stays_inside_plugin_namespace() {
        let mut item = AdminMenuItem {
            label: "Example".into(),
            labels: BTreeMap::new(),
            path: "/admin/plugins/example/settings".into(),
            icon: Some("bi-puzzle".into()),
            access: None,
        };
        assert!(valid_admin_menu_item(&item, "example"));

        item.path = "/configuration".into();
        assert!(!valid_admin_menu_item(&item, "example"));
        item.path = "/admin/plugins/example/../other".into();
        assert!(!valid_admin_menu_item(&item, "example"));
    }

    #[test]
    fn admin_menu_access_inherits_and_cannot_expand_admin_access() {
        let mut admin = AdminManifest {
            entry: Some("admin.js".into()),
            element: Some("nur-cms-test".into()),
            access: "admin,stat".into(),
            styles: Vec::new(),
            menu: vec![AdminMenuItem {
                label: "Products".into(),
                labels: BTreeMap::from([("de".into(), "Produkte".into())]),
                path: "/admin/plugins/example/products".into(),
                icon: None,
                access: None,
            }],
        };

        assert_eq!(
            admin.menu_roles(&admin.menu[0], "example").unwrap(),
            ["admin", "stat"]
        );
        assert!(admin.validate_menu_access("example").is_ok());

        admin.menu[0].access = Some("author".into());
        assert!(admin.validate_menu_access("example").is_err());

        admin.menu[0].access = Some("invalid role".into());
        assert!(admin.validate_menu_access("example").is_err());
    }

    #[test]
    fn echo_example_uses_a_valid_current_manifest() {
        let manifest: Manifest =
            toml_edit::de::from_str(include_str!("../examples/echo/plugin.toml"))
                .expect("echo manifest can be deserialized");

        validate_manifest(&manifest).expect("echo manifest is valid");
    }

    #[test]
    fn vue_admin_example_uses_a_valid_current_manifest() {
        let manifest: Manifest =
            toml_edit::de::from_str(include_str!("../examples/vue-admin/plugin.toml"))
                .expect("Vue admin manifest can be deserialized");

        validate_manifest(&manifest).expect("Vue admin manifest is valid");
    }

    #[test]
    fn mail_permissions_are_explicit_and_dynamic_targets_are_a_subset() {
        let permissions = MailManifest {
            targets: vec!["contact".into(), "orders".into()],
            dynamic_recipient_targets: vec!["orders".into()],
            trusted_template_targets: vec!["orders".into()],
        };
        assert!(validate_mail_permissions(&permissions, "example").is_ok());

        let undeclared_dynamic_target = MailManifest {
            targets: vec!["contact".into()],
            dynamic_recipient_targets: vec!["orders".into()],
            trusted_template_targets: Vec::new(),
        };
        assert!(validate_mail_permissions(&undeclared_dynamic_target, "example").is_err());

        let duplicate_target = MailManifest {
            targets: vec!["contact".into(), "contact".into()],
            dynamic_recipient_targets: Vec::new(),
            trusted_template_targets: Vec::new(),
        };
        assert!(validate_mail_permissions(&duplicate_target, "example").is_err());

        let undeclared_trusted_template_target = MailManifest {
            targets: vec!["contact".into()],
            dynamic_recipient_targets: Vec::new(),
            trusted_template_targets: vec!["orders".into()],
        };
        assert!(validate_mail_permissions(&undeclared_trusted_template_target, "example").is_err());

        let duplicate_trusted_template_target = MailManifest {
            targets: vec!["contact".into()],
            dynamic_recipient_targets: Vec::new(),
            trusted_template_targets: vec!["contact".into(), "contact".into()],
        };
        assert!(validate_mail_permissions(&duplicate_trusted_template_target, "example").is_err());
    }

    #[test]
    fn plugin_ids_produce_distinct_safe_schema_names() {
        assert!(valid_plugin_id("my-plugin"));
        assert!(!valid_plugin_id("my_plugin"));
        assert_eq!(schema_name("my-plugin"), "nur_plugin_my_plugin");
    }

    #[test]
    fn plugin_display_names_are_bounded_and_safe_for_the_menu() {
        assert!(valid_plugin_name("Community Site"));
        assert!(!valid_plugin_name(" Community Site"));
        assert!(!valid_plugin_name("line\nbreak"));
        assert!(!valid_plugin_name(&"x".repeat(81)));
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
