use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    str::from_utf8,
};

use sha2::{Digest, Sha256};

use nur_core::config::settings;

use super::{
    InstalledPlugin, Manifest, contained_path,
    validation::{valid_plugin_id, validate_admin_assets, validate_manifest},
};
use crate::Error;

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
            let Some(plugin) = load_installed_plugin(&plugin_root, enabled_id)? else {
                continue;
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

    validate_unique_admin_elements(&plugins)?;

    Ok(plugins)
}

fn load_installed_plugin(
    plugin_root: &Path,
    enabled_id: &str,
) -> Result<Option<InstalledPlugin>, Error> {
    let root = plugin_root.join(enabled_id);
    let manifest_path = root.join("plugin.toml");

    if !manifest_path.is_file() {
        return Ok(None);
    }

    let bytes = read_manifest(&manifest_path)?;
    let source = from_utf8(&bytes)
        .map_err(|error| Error::Manifest(format!("{}: {error}", manifest_path.display())))?;
    let manifest: Manifest = toml_edit::de::from_str(source)
        .map_err(|error| Error::Manifest(format!("{}: {error}", manifest_path.display())))?;

    validate_manifest(&manifest)?;
    validate_admin_component_permission(&manifest)?;

    if manifest.plugin.id != enabled_id {
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

    let assets = resolve_assets(&root, &manifest)?;
    validate_admin_assets(&manifest, assets.as_deref())?;

    Ok(Some(InstalledPlugin {
        manifest,
        root,
        module,
        assets,
        manifest_checksum: Sha256::digest(bytes).to_vec(),
    }))
}

fn read_manifest(path: &Path) -> Result<Vec<u8>, Error> {
    if fs::metadata(path).map_err(Error::Io)?.len() > 256 * 1024 {
        return Err(Error::Manifest(format!(
            "plugin manifest is too large: {}",
            path.display()
        )));
    }

    fs::read(path).map_err(Error::Io)
}

fn validate_admin_component_permission(manifest: &Manifest) -> Result<(), Error> {
    let has_admin_entry = manifest
        .admin
        .as_ref()
        .and_then(|admin| admin.entry.as_ref())
        .is_some();

    if has_admin_entry && !settings().plugins.allow_admin_components {
        return Err(Error::Manifest(format!(
            "plugin '{}' declares browser-side admin code; set plugins.allow_admin_components = true to trust and enable it",
            manifest.plugin.id
        )));
    }

    Ok(())
}

fn resolve_assets(root: &Path, manifest: &Manifest) -> Result<Option<PathBuf>, Error> {
    let assets = manifest
        .assets
        .as_ref()
        .map(|assets| {
            contained_path(
                root,
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

    Ok(assets)
}

fn validate_unique_admin_elements(plugins: &[InstalledPlugin]) -> Result<(), Error> {
    let mut elements = HashSet::new();

    for plugin in plugins {
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

    Ok(())
}

pub(super) fn validate_asset_tree(root: &Path, plugin_id: &str) -> Result<(), Error> {
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

fn enabled_plugins() -> HashSet<String> {
    settings().plugins.enabled.iter().cloned().collect()
}

fn plugin_roots() -> Vec<PathBuf> {
    let mut roots = settings().plugins.additional_directories.clone();
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
