use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use self::validation::parse_access;
use crate::Error;

mod discovery;
mod validation;

pub(crate) use discovery::discover;
pub use validation::{contained_path, schema_name};
pub(crate) use validation::{
    valid_plugin_id, valid_storage_extension, valid_storage_id, valid_storage_path,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub plugin: PluginManifest,
    #[serde(default)]
    pub migrations: MigrationManifest,
    #[serde(default)]
    pub mail: MailManifest,
    #[serde(default)]
    pub storage: StorageManifest,
    #[serde(default)]
    pub routes: Vec<RouteManifest>,
    pub assets: Option<AssetsManifest>,
    pub cache: Option<CacheManifest>,
    pub admin: Option<AdminManifest>,
}

/// Files owned by a plugin but never represented as CMS media records.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageManifest {
    #[serde(default)]
    pub directories: Vec<StorageDirectoryManifest>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageDirectoryManifest {
    /// Stable manifest-local identifier used by the host and admin APIs.
    pub id: String,
    /// A relative directory below this plugin's public or private namespace.
    pub path: String,
    /// Lowercase file extensions that may be written to this directory.
    pub extensions: Vec<String>,
    #[serde(default)]
    pub visibility: StorageVisibility,
    #[serde(default)]
    pub upload: StorageUpload,
    /// Required only for authenticated browser uploads and administrative access.
    #[serde(default = "private_access")]
    pub access: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StorageVisibility {
    #[default]
    Public,
    Private,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StorageUpload {
    /// The plugin may write files through WIT, but no browser upload endpoint exists.
    #[default]
    None,
    /// A browser upload requires an authenticated user with `access`.
    Authenticated,
    /// A browser upload requires a single-use upload capability created by the plugin.
    Link,
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
    #[serde(default)]
    pub scope: RouteScope,
    #[serde(default = "public_access")]
    pub access: String,
    #[serde(default)]
    pub cache: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RouteScope {
    #[default]
    Plugin,
    Root,
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
    #[serde(default)]
    pub vary_headers: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdminManifest {
    pub entry: Option<String>,
    pub element: Option<String>,
    #[serde(default = "private_access")]
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

fn private_access() -> String {
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

impl StorageDirectoryManifest {
    pub fn roles(&self, plugin_id: &str) -> Result<Vec<String>, Error> {
        parse_access(
            &self.access,
            &format!("plugin '{plugin_id}' storage directory '{}'", self.id),
            false,
        )
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

#[cfg(test)]
mod tests;
