use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use semver::{Version, VersionReq};

use super::{
    AdminManifest, AdminMenuItem, CacheManifest, MailManifest, Manifest, PluginManifest,
    StorageManifest,
};
use crate::{
    API_VERSION, Error,
    transport::{FORWARDED_REQUEST_HEADERS, TRUSTED_PROXY_REQUEST_HEADERS},
};

pub(super) fn parse_access(
    access: &str,
    context: &str,
    allow_public: bool,
) -> Result<Vec<String>, Error> {
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

pub(super) fn validate_manifest(manifest: &Manifest) -> Result<(), Error> {
    let plugin = &manifest.plugin;

    validate_plugin_metadata(plugin)?;
    validate_cache(manifest.cache.as_ref(), &plugin.id)?;
    validate_mail_permissions(&manifest.mail, &plugin.id)?;
    validate_storage(&manifest.storage, &plugin.id)?;
    validate_cms_version(plugin)?;
    validate_routes(manifest)?;

    if let Some(admin) = &manifest.admin {
        validate_admin(admin, &plugin.id)?;
    }

    Ok(())
}

fn validate_plugin_metadata(plugin: &PluginManifest) -> Result<(), Error> {
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

    Ok(())
}

fn validate_cms_version(plugin: &PluginManifest) -> Result<(), Error> {
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

    Ok(())
}

pub(super) fn validate_cache(cache: Option<&CacheManifest>, plugin_id: &str) -> Result<(), Error> {
    let Some(cache) = cache else {
        return Ok(());
    };

    let vary_headers = cache
        .vary_headers
        .iter()
        .map(|header| header.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let unsupported_vary_header = vary_headers.iter().any(|header| {
        !FORWARDED_REQUEST_HEADERS.contains(&header.as_str())
            && !TRUSTED_PROXY_REQUEST_HEADERS.contains(&header.as_str())
    });

    if !(1..=86_400).contains(&cache.ttl_seconds)
        || !(1..=10_000).contains(&cache.max_entries)
        || cache.vary_headers.len() > 16
        || vary_headers.len() != cache.vary_headers.len()
        || unsupported_vary_header
    {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' cache settings are outside the supported limits"
        )));
    }

    Ok(())
}

fn validate_routes(manifest: &Manifest) -> Result<(), Error> {
    let plugin_id = &manifest.plugin.id;

    if manifest.routes.len() > 64 {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' declares more than 64 routes"
        )));
    }

    let mut route_ids = HashSet::new();

    for route in &manifest.routes {
        if !valid_route_id(&route.id) || !route_ids.insert(&route.id) {
            return Err(Error::Manifest(format!(
                "plugin '{plugin_id}' has an invalid or duplicate route id"
            )));
        }

        route.cache_enabled(manifest.cache.is_some())?;
    }

    Ok(())
}

fn validate_admin(admin: &AdminManifest, plugin_id: &str) -> Result<(), Error> {
    admin.roles(plugin_id)?;

    let unique_styles: HashSet<_> = admin.styles.iter().collect();

    if admin.styles.len() > 16
        || unique_styles.len() != admin.styles.len()
        || admin.styles.iter().any(|style| !valid_admin_style(style))
    {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' has invalid or duplicate admin styles"
        )));
    }

    match (&admin.entry, &admin.element) {
        (Some(entry), Some(element))
            if valid_admin_entry(entry) && valid_custom_element_name(element) => {}
        (Some(_), Some(_)) => {
            return Err(Error::Manifest(format!(
                "plugin '{plugin_id}' has an invalid admin entry or custom element name"
            )));
        }
        (None, None) if admin.menu.is_empty() && admin.styles.is_empty() => {}
        (None, None) => {
            return Err(Error::Manifest(format!(
                "plugin '{plugin_id}' declares admin menu items without an admin entry and custom element"
            )));
        }
        _ => {
            return Err(Error::Manifest(format!(
                "plugin '{plugin_id}' must declare both admin entry and custom element"
            )));
        }
    }

    if admin.menu.len() > 32 || admin.menu.iter().any(|item| !valid_admin_menu_item(item)) {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' has invalid admin menu metadata"
        )));
    }

    admin.validate_menu_access(plugin_id)?;

    Ok(())
}

pub(super) fn validate_storage(storage: &StorageManifest, plugin_id: &str) -> Result<(), Error> {
    if storage.directories.len() > 16 {
        return Err(Error::Manifest(format!(
            "plugin '{plugin_id}' declares more than 16 storage directories"
        )));
    }

    let mut ids = HashSet::new();
    let mut paths = HashSet::new();

    for directory in &storage.directories {
        if !valid_storage_id(&directory.id) || !ids.insert(&directory.id) {
            return Err(Error::Manifest(format!(
                "plugin '{plugin_id}' has an invalid or duplicate storage directory id"
            )));
        }

        if !valid_storage_path(&directory.path) || !paths.insert(&directory.path) {
            return Err(Error::Manifest(format!(
                "plugin '{plugin_id}' has an invalid or duplicate storage directory path"
            )));
        }

        let extensions: HashSet<_> = directory.extensions.iter().collect();

        if directory.extensions.is_empty()
            || directory.extensions.len() > 32
            || extensions.len() != directory.extensions.len()
            || directory
                .extensions
                .iter()
                .any(|extension| !valid_storage_extension(extension))
        {
            return Err(Error::Manifest(format!(
                "plugin '{plugin_id}' storage directory '{}' has invalid or duplicate file extensions",
                directory.id
            )));
        }

        directory.roles(plugin_id)?;
    }

    Ok(())
}

pub(super) fn validate_mail_permissions(mail: &MailManifest, plugin_id: &str) -> Result<(), Error> {
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

pub(super) fn validate_admin_assets(
    manifest: &Manifest,
    assets: Option<&Path>,
) -> Result<(), Error> {
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

pub(crate) fn valid_plugin_id(id: &str) -> bool {
    (3..=40).contains(&id.len())
        && id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(super) fn valid_plugin_name(name: &str) -> bool {
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

pub(super) fn valid_admin_entry(entry: &str) -> bool {
    valid_admin_asset_path(entry)
        && matches!(
            Path::new(entry)
                .extension()
                .and_then(|value| value.to_str()),
            Some("js" | "mjs")
        )
}

pub(super) fn valid_admin_style(style: &str) -> bool {
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

pub(super) fn valid_custom_element_name(name: &str) -> bool {
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

pub(super) fn valid_admin_menu_item(item: &AdminMenuItem) -> bool {
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
        && item.path.starts_with('/')
        && !["/admin/plugins", "/admin/p"].iter().any(|prefix| {
            item.path == *prefix
                || item
                    .path
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
        && !item.path.contains(['?', '#'])
        && !item.path.chars().any(char::is_control)
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

pub(super) fn valid_route_id(id: &str) -> bool {
    (1..=80).contains(&id.len())
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

pub(crate) fn valid_storage_id(id: &str) -> bool {
    (1..=80).contains(&id.len())
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

pub(crate) fn valid_storage_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && path.split('/').all(|segment| {
            matches!(segment, "{year}" | "{month}")
                || (!segment.is_empty()
                    && segment.len() <= 128
                    && !matches!(segment, "." | "..")
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    }))
        })
}

pub(crate) fn valid_storage_extension(extension: &str) -> bool {
    matches!(
        extension,
        "avif"
            | "csv"
            | "doc"
            | "docx"
            | "gif"
            | "jpg"
            | "jpeg"
            | "mp3"
            | "mp4"
            | "ods"
            | "odt"
            | "ogg"
            | "pdf"
            | "png"
            | "ppt"
            | "pptx"
            | "rtf"
            | "txt"
            | "wav"
            | "webm"
            | "webp"
            | "xls"
            | "xlsx"
            | "7z"
            | "bzip2"
            | "gz"
            | "tar"
            | "zip"
    )
}
