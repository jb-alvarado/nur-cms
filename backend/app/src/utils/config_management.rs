use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use nur_core::config::{self, AppConfig};

use crate::utils::extend_args::{AppCommand, ConfigCommand, ConfigPathArgs, CreateConfigArgs};

const TEMPLATE: &str = include_str!("../../../../assets/nur-cms.toml");
const ENV_ONLY: [&str; 2] = ["NUR_DEV_AUTO_ADMIN", "NUR_DEV_SEED_DATABASE"];

pub fn run(
    command: &AppCommand,
    global_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        AppCommand::Config { command } => match command {
            ConfigCommand::Create(args) => create(args),
            ConfigCommand::Check(args) => check(args, global_path),
            ConfigCommand::Migrate(args) => migrate(args, global_path),
        },
    }
}

fn selected_path(args: &ConfigPathArgs, global_path: Option<&Path>) -> Option<PathBuf> {
    args.path
        .clone()
        .or_else(|| global_path.map(Path::to_path_buf))
}

fn check(
    args: &ConfigPathArgs,
    global_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_, path) = config::load(selected_path(args, global_path).as_deref())?;
    println!("Configuration is valid: {}", path.display());
    Ok(())
}

fn migrate(
    args: &ConfigPathArgs,
    global_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config::resolve_path(selected_path(args, global_path).as_deref())?;
    let source = fs::read_to_string(&path)?;
    let (_, migrated, changed) = config::migrate_source(&path, &source)?;
    if !changed {
        println!(
            "Configuration is already at version {}: {}",
            config::CONFIG_VERSION,
            path.display()
        );
        return Ok(());
    }
    write_atomic(&path, migrated.as_bytes(), true, true)?;
    println!(
        "Migrated configuration to version {}: {}",
        config::CONFIG_VERSION,
        path.display()
    );
    Ok(())
}

fn create(args: &CreateConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    let env_source = choose_env_source(args)?;
    let (contents, report) = if let Some(source) = env_source {
        let (values, report_unknown) = match source {
            EnvSource::File(path) => (read_dotenv(&path)?, true),
            EnvSource::Process => (env::vars().collect(), false),
        };
        let mut settings = AppConfig::default();
        let report = import_legacy(&mut settings, &values, report_unknown)?;
        settings.validate()?;
        (toml_edit::ser::to_string_pretty(&settings)?, Some(report))
    } else {
        AppConfig::parse(Path::new("embedded nur-cms.toml"), TEMPLATE)?;
        (TEMPLATE.to_owned(), None)
    };

    write_atomic(&args.path, contents.as_bytes(), args.force, true)?;
    println!("Created {}", args.path.display());
    if let Some(report) = report {
        println!("Imported {} known setting(s).", report.imported.len());
        if !report.environment_only.is_empty() {
            println!(
                "Kept as environment-only: {}",
                report
                    .environment_only
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !report.unknown.is_empty() {
            println!(
                "Ignored unknown variables: {}",
                report.unknown.into_iter().collect::<Vec<_>>().join(", ")
            );
        }
    }
    Ok(())
}

enum EnvSource {
    File(PathBuf),
    Process,
}

fn choose_env_source(args: &CreateConfigArgs) -> io::Result<Option<EnvSource>> {
    if let Some(path) = &args.from_env {
        return Ok(Some(EnvSource::File(path.clone())));
    }
    if args.from_environment {
        return Ok(Some(EnvSource::Process));
    }
    if args.no_env {
        return Ok(None);
    }
    let path = PathBuf::from(".env");
    if !path.is_file() {
        return Ok(None);
    }
    if !io::stdin().is_terminal() {
        eprintln!("Found .env; use --from-env .env to import it.");
        return Ok(None);
    }
    eprint!("Import known values from .env? [Y/n] ");
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok((answer.trim().is_empty()
        || answer.trim().eq_ignore_ascii_case("y")
        || answer.trim().eq_ignore_ascii_case("yes"))
    .then_some(EnvSource::File(path)))
}

fn read_dotenv(path: &Path) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let mut values = BTreeMap::new();
    for item in dotenvy::from_path_iter(path)? {
        let (key, value) = item?;
        values.insert(key, value);
    }
    Ok(values)
}

#[derive(Default)]
struct ImportReport {
    imported: BTreeSet<String>,
    environment_only: BTreeSet<String>,
    unknown: BTreeSet<String>,
}

fn import_legacy(
    settings: &mut AppConfig,
    values: &BTreeMap<String, String>,
    report_unknown: bool,
) -> Result<ImportReport, Box<dyn std::error::Error>> {
    let mut report = ImportReport::default();
    for key in values.keys() {
        if ENV_ONLY.contains(&key.as_str()) {
            report.environment_only.insert(key.clone());
        } else if report_unknown && !KNOWN_ENV.contains(&key.as_str()) {
            report.unknown.insert(key.clone());
        }
    }
    macro_rules! import {
        ($name:literal, $target:expr, $parser:expr) => {
            if let Some(value) = values.get($name) {
                $target = $parser(value).map_err(|message| format!("{}: {message}", $name))?;
                report.imported.insert($name.into());
            }
        };
    }
    import!("DATABASE_URL", settings.database.url, nonempty);
    import!("MAX_CONNECTIONS", settings.database.max_connections, parse);
    import!("LISTEN", settings.server.listen, nonempty);
    import!(
        "NUR_PUBLIC_URL",
        settings.server.public_url,
        |value: &str| Ok::<_, String>(Some(nonempty(value)?))
    );
    import!(
        "TRUSTED_PROXY_CIDRS",
        settings.server.trusted_proxy_cidrs,
        csv
    );
    import!(
        "DISABLE_TWO_FACTOR",
        settings.authentication.disable_two_factor,
        boolean
    );
    if let Some(value) = values.get("ACCESS_LIFETIME_MINUTES") {
        settings.authentication.access_token_lifetime_minutes =
            parse(value).map_err(|message| format!("ACCESS_LIFETIME_MINUTES: {message}"))?;
        report.imported.insert("ACCESS_LIFETIME_MINUTES".into());
    } else if let Some(value) = values.get("ACCESS_LIFETIME") {
        let days: i64 = parse(value).map_err(|message| format!("ACCESS_LIFETIME: {message}"))?;
        settings.authentication.access_token_lifetime_minutes = days
            .checked_mul(1_440)
            .ok_or("ACCESS_LIFETIME is too large")?;
        report.imported.insert("ACCESS_LIFETIME".into());
    }
    import!(
        "REFRESH_LIFETIME",
        settings.authentication.refresh_token_lifetime_days,
        parse
    );
    import!(
        "NUR_COMMENT_MODERATION_TOKEN_TTL_DAYS",
        settings.comments.moderation_token_lifetime_days,
        parse
    );
    import!("STORAGE", settings.uploads.directory, |value: &str| Ok::<
        _,
        String,
    >(
        PathBuf::from(nonempty(value)?)
    ));
    import!("MAX_UPLOAD_SIZE", settings.uploads.max_size_mb, bytes_to_mb);
    import!(
        "MAX_CHUNK_SIZE",
        settings.uploads.chunk_size_mb,
        bytes_to_mb
    );
    import!(
        "MAX_ACTIVE_UPLOADS_PER_USER",
        settings.uploads.max_active_per_user,
        parse
    );
    import!(
        "UPLOAD_TTL_SECONDS",
        settings.uploads.session_lifetime_hours,
        seconds_to_hours
    );
    import!("MAX_IMAGE_PIXELS", settings.images.max_pixels, parse);
    import!(
        "IMAGE_PROCESSING_CONCURRENCY",
        settings.images.processing_concurrency,
        parse
    );
    import!(
        "VIDEO_PROCESSING_CONCURRENCY",
        settings.video.processing_concurrency,
        parse
    );
    import!(
        "VIDEO_PROCESSING_THREADS",
        settings.video.processing_threads,
        parse
    );
    import!(
        "VIDEO_PROCESSING_TIMEOUT_SECONDS",
        settings.video.processing_timeout_minutes,
        seconds_to_minutes
    );
    import!(
        "VIDEO_PROCESSING_LEASE_SECONDS",
        settings.video.lease_seconds,
        parse
    );
    import!(
        "VIDEO_PROCESSING_MAX_ATTEMPTS",
        settings.video.max_attempts,
        parse
    );
    import!(
        "VIDEO_PROCESSING_MAX_DURATION_SECONDS",
        settings.video.max_duration_hours,
        seconds_to_hours
    );
    import!(
        "VIDEO_PROCESSING_MAX_PIXELS",
        settings.video.max_pixels,
        parse
    );
    import!(
        "VIDEO_PROCESSING_MAX_OUTPUT_SIZE",
        settings.video.max_output_size_mb,
        bytes_to_mb_allow_zero
    );
    import!("NUR_FFMPEG_BIN", settings.video.ffmpeg, |value: &str| Ok::<
        _,
        String,
    >(
        PathBuf::from(nonempty(value)?)
    ));
    import!(
        "NUR_FFPROBE_BIN",
        settings.video.ffprobe,
        |value: &str| Ok::<_, String>(PathBuf::from(nonempty(value)?))
    );
    import!("NUR_ENTRY_CACHE", settings.entry_cache.enabled, boolean);
    import!(
        "NUR_ENTRY_CACHE_CAPACITY",
        settings.entry_cache.capacity,
        parse
    );
    import!(
        "NUR_ENTRY_CACHE_TTI_SECONDS",
        settings.entry_cache.time_to_idle_minutes,
        seconds_to_minutes
    );
    import!(
        "NUR_ENTRY_CACHE_TTL_SECONDS",
        settings.entry_cache.time_to_live_hours,
        seconds_to_hours
    );
    import!("NUR_PLUGINS", settings.plugins.enabled, csv);
    if let Some(value) = values.get("NUR_PLUGIN_DIR") {
        settings.plugins.additional_directories = env::split_paths(value).collect();
        report.imported.insert("NUR_PLUGIN_DIR".into());
    }
    import!(
        "NUR_PLUGIN_ALLOW_ROOT_ROUTES",
        settings.plugins.allow_root_routes,
        boolean
    );
    import!(
        "NUR_PLUGIN_ALLOW_ADMIN_COMPONENTS",
        settings.plugins.allow_admin_components,
        boolean
    );
    import!("NUR_PLUGIN_FUEL", settings.plugins.runtime.fuel, parse);
    import!(
        "NUR_PLUGIN_MEMORY_LIMIT",
        settings.plugins.runtime.memory_limit_mb,
        bytes_to_mb
    );
    import!(
        "NUR_PLUGIN_MODULE_SIZE_LIMIT",
        settings.plugins.runtime.module_size_limit_mb,
        bytes_to_mb
    );
    import!(
        "NUR_PLUGIN_TIMEOUT_MS",
        settings.plugins.runtime.timeout_seconds,
        milliseconds_to_seconds
    );
    import!(
        "NUR_PLUGIN_MAX_CONCURRENCY",
        settings.plugins.runtime.max_concurrency,
        parse
    );
    import!(
        "NUR_PLUGIN_MAX_HOST_CALLS",
        settings.plugins.runtime.max_host_calls,
        parse
    );
    import!(
        "NUR_PLUGIN_REQUEST_BODY_LIMIT",
        settings.plugins.runtime.request_body_limit_mb,
        bytes_to_mb
    );
    import!(
        "NUR_PLUGIN_RESPONSE_BODY_LIMIT",
        settings.plugins.runtime.response_body_limit_mb,
        bytes_to_mb
    );
    import!(
        "NUR_PLUGIN_CACHE_MEMORY_LIMIT",
        settings.plugins.runtime.route_cache_limit_mb,
        bytes_to_mb
    );
    import!(
        "NUR_PLUGIN_METRICS",
        settings.plugins.runtime.metrics_enabled,
        boolean
    );
    import!(
        "NUR_PLUGIN_COMPILATION_CACHE",
        settings.plugins.compilation_cache.enabled,
        boolean
    );
    import!(
        "NUR_PLUGIN_COMPILATION_CACHE_DIR",
        settings.plugins.compilation_cache.directory,
        |value: &str| Ok::<_, String>(Some(PathBuf::from(nonempty(value)?)))
    );
    import!(
        "NUR_PLUGIN_COMPILATION_CACHE_SIZE",
        settings.plugins.compilation_cache.max_size_mb,
        bytes_to_mb
    );
    import!(
        "NUR_PLUGIN_STORAGE",
        settings.plugins.storage.private_directory,
        |value: &str| Ok::<_, String>(Some(PathBuf::from(nonempty(value)?)))
    );
    import!(
        "NUR_PLUGIN_STORAGE_WRITE_LIMIT",
        settings.plugins.storage.write_limit_mb,
        bytes_to_mb
    );
    import!(
        "NUR_PLUGIN_STORAGE_QUOTA",
        settings.plugins.storage.quota_mb,
        bytes_to_mb
    );
    import!(
        "NUR_PLUGIN_FILE_LINK_MAX_AGE_HOURS",
        settings.plugins.storage.file_links_max_age_hours,
        parse
    );
    import!(
        "NUR_PLUGIN_PUBLIC_MAIL_INTERVAL_SECONDS",
        settings.plugins.public_mail.interval_minutes,
        seconds_to_minutes
    );
    import!(
        "NUR_PLUGIN_PUBLIC_MAIL_MAX_CLIENTS",
        settings.plugins.public_mail.max_clients,
        parse
    );
    Ok(report)
}

const KNOWN_ENV: &[&str] = &[
    "DATABASE_URL",
    "MAX_CONNECTIONS",
    "LISTEN",
    "NUR_PUBLIC_URL",
    "TRUSTED_PROXY_CIDRS",
    "DISABLE_TWO_FACTOR",
    "ACCESS_LIFETIME_MINUTES",
    "ACCESS_LIFETIME",
    "REFRESH_LIFETIME",
    "NUR_COMMENT_MODERATION_TOKEN_TTL_DAYS",
    "STORAGE",
    "MAX_UPLOAD_SIZE",
    "MAX_CHUNK_SIZE",
    "MAX_ACTIVE_UPLOADS_PER_USER",
    "UPLOAD_TTL_SECONDS",
    "MAX_IMAGE_PIXELS",
    "IMAGE_PROCESSING_CONCURRENCY",
    "VIDEO_PROCESSING_CONCURRENCY",
    "VIDEO_PROCESSING_THREADS",
    "VIDEO_PROCESSING_TIMEOUT_SECONDS",
    "VIDEO_PROCESSING_LEASE_SECONDS",
    "VIDEO_PROCESSING_MAX_ATTEMPTS",
    "VIDEO_PROCESSING_MAX_DURATION_SECONDS",
    "VIDEO_PROCESSING_MAX_PIXELS",
    "VIDEO_PROCESSING_MAX_OUTPUT_SIZE",
    "NUR_FFMPEG_BIN",
    "NUR_FFPROBE_BIN",
    "NUR_ENTRY_CACHE",
    "NUR_ENTRY_CACHE_CAPACITY",
    "NUR_ENTRY_CACHE_TTI_SECONDS",
    "NUR_ENTRY_CACHE_TTL_SECONDS",
    "NUR_PLUGINS",
    "NUR_PLUGIN_DIR",
    "NUR_PLUGIN_ALLOW_ROOT_ROUTES",
    "NUR_PLUGIN_ALLOW_ADMIN_COMPONENTS",
    "NUR_PLUGIN_FUEL",
    "NUR_PLUGIN_MEMORY_LIMIT",
    "NUR_PLUGIN_MODULE_SIZE_LIMIT",
    "NUR_PLUGIN_TIMEOUT_MS",
    "NUR_PLUGIN_MAX_CONCURRENCY",
    "NUR_PLUGIN_MAX_HOST_CALLS",
    "NUR_PLUGIN_REQUEST_BODY_LIMIT",
    "NUR_PLUGIN_RESPONSE_BODY_LIMIT",
    "NUR_PLUGIN_CACHE_MEMORY_LIMIT",
    "NUR_PLUGIN_METRICS",
    "NUR_PLUGIN_COMPILATION_CACHE",
    "NUR_PLUGIN_COMPILATION_CACHE_DIR",
    "NUR_PLUGIN_COMPILATION_CACHE_SIZE",
    "NUR_PLUGIN_STORAGE",
    "NUR_PLUGIN_STORAGE_WRITE_LIMIT",
    "NUR_PLUGIN_STORAGE_QUOTA",
    "NUR_PLUGIN_FILE_LINK_MAX_AGE_HOURS",
    "NUR_PLUGIN_PUBLIC_MAIL_INTERVAL_SECONDS",
    "NUR_PLUGIN_PUBLIC_MAIL_MAX_CLIENTS",
];

fn nonempty(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("must not be empty".into())
    } else {
        Ok(value.trim().into())
    }
}
fn parse<T: std::str::FromStr>(value: &str) -> Result<T, String> {
    value.parse().map_err(|_| "invalid value".into())
}
fn boolean(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err("expected a boolean".into()),
    }
}
fn csv(value: &str) -> Result<Vec<String>, String> {
    Ok(value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect())
}
fn exact_unit(value: &str, divisor: u64, unit: &str) -> Result<u64, String> {
    let value: u64 = parse(value)?;
    if value.is_multiple_of(divisor) {
        Ok(value / divisor)
    } else {
        Err(format!("{value} cannot be represented exactly in {unit}"))
    }
}
fn bytes_to_mb(value: &str) -> Result<u64, String> {
    exact_unit(value, 1024 * 1024, "MB")
}
fn bytes_to_mb_allow_zero(value: &str) -> Result<u64, String> {
    bytes_to_mb(value)
}
fn seconds_to_minutes(value: &str) -> Result<u64, String> {
    exact_unit(value, 60, "minutes")
}
fn seconds_to_hours(value: &str) -> Result<u64, String> {
    exact_unit(value, 60 * 60, "hours")
}
fn milliseconds_to_seconds(value: &str) -> Result<u64, String> {
    exact_unit(value, 1_000, "seconds")
}

fn write_atomic(path: &Path, contents: &[u8], force: bool, backup: bool) -> io::Result<()> {
    if path.exists() && !force {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "{} already exists; use --force to replace it",
                path.display()
            ),
        ));
    }
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let existing_metadata = path.exists().then(|| fs::metadata(path)).transpose()?;
    if path.exists() && backup {
        let backup_path = path.with_extension(format!(
            "{}bak",
            path.extension()
                .and_then(|value| value.to_str())
                .map(|value| format!("{value}."))
                .unwrap_or_default()
        ));
        fs::copy(path, backup_path)?;
    }
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("nur-cms.toml"),
        std::process::id()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o640);
    }
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(contents)?;
        preserve_metadata(&temporary, existing_metadata.as_ref())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(unix)]
fn preserve_metadata(path: &Path, metadata: Option<&fs::Metadata>) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, chown};

    let Some(metadata) = metadata else {
        return Ok(());
    };
    let current = fs::metadata(path)?;
    if current.uid() != metadata.uid() || current.gid() != metadata.gid() {
        chown(path, Some(metadata.uid()), Some(metadata.gid()))?;
    }
    fs::set_permissions(path, fs::Permissions::from_mode(metadata.mode() & 0o7777))
}

#[cfg(not(unix))]
fn preserve_metadata(_path: &Path, _metadata: Option<&fs::Metadata>) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use nur_core::config::{self, AppConfig};

    use super::{TEMPLATE, import_legacy};

    #[test]
    fn packaged_template_is_valid() {
        config::migrate_source("template.toml".as_ref(), TEMPLATE)
            .expect("valid packaged template");
    }

    #[test]
    fn legacy_import_converts_units_and_excludes_development_switches() {
        let values = BTreeMap::from([
            ("MAX_UPLOAD_SIZE".into(), "838860800".into()),
            ("NUR_PLUGIN_TIMEOUT_MS".into(), "5000".into()),
            ("NUR_DEV_AUTO_ADMIN".into(), "1".into()),
        ]);
        let mut config = AppConfig::default();
        let report = import_legacy(&mut config, &values, true).expect("valid legacy values");

        assert_eq!(config.uploads.max_size_mb, 800);
        assert_eq!(config.plugins.runtime.timeout_seconds, 5);
        assert!(report.environment_only.contains("NUR_DEV_AUTO_ADMIN"));
        assert!(!report.imported.contains("NUR_DEV_AUTO_ADMIN"));
    }

    #[test]
    fn legacy_import_rejects_lossy_unit_conversion() {
        let values = BTreeMap::from([("MAX_UPLOAD_SIZE".into(), "1048577".into())]);
        let error = import_legacy(&mut AppConfig::default(), &values, true)
            .err()
            .expect("non-exact conversion must fail");
        assert!(error.to_string().contains("cannot be represented exactly"));
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replacement_preserves_permissions_and_owner() {
        use std::{
            fs,
            os::unix::fs::{MetadataExt, PermissionsExt},
            time::{SystemTime, UNIX_EPOCH},
        };

        use super::write_atomic;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "nur-cms-config-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("nur-cms.toml");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let before = fs::metadata(&path).unwrap();

        write_atomic(&path, b"new", true, false).unwrap();

        let after = fs::metadata(&path).unwrap();
        assert_eq!(after.mode() & 0o7777, 0o600);
        assert_eq!(after.uid(), before.uid());
        assert_eq!(after.gid(), before.gid());
        assert_eq!(fs::read(&path).unwrap(), b"new");
        fs::remove_dir_all(directory).unwrap();
    }
}
