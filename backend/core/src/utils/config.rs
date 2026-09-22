use std::{
    collections::BTreeMap,
    env, fmt, fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
pub const CONFIG_FILE_NAME: &str = "nur-cms.toml";

static CONFIG: OnceLock<AppConfig> = OnceLock::new();

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub version: u32,
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub authentication: AuthenticationConfig,
    pub comments: CommentsConfig,
    pub uploads: UploadConfig,
    pub images: ImageConfig,
    pub video: VideoConfig,
    pub entry_cache: EntryCacheConfig,
    pub plugins: PluginConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_database_connections")]
    pub max_connections: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub listen: String,
    #[serde(default)]
    pub public_url: Option<String>,
    #[serde(default)]
    pub trusted_proxy_cidrs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticationConfig {
    pub disable_two_factor: bool,
    pub access_token_lifetime_minutes: i64,
    pub refresh_token_lifetime_days: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommentsConfig {
    pub moderation_token_lifetime_days: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UploadConfig {
    pub directory: PathBuf,
    pub max_size_mb: u64,
    pub chunk_size_mb: u64,
    pub max_active_per_user: usize,
    pub session_lifetime_hours: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImageConfig {
    pub max_pixels: u64,
    pub processing_concurrency: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VideoConfig {
    pub processing_concurrency: usize,
    pub processing_threads: usize,
    pub processing_timeout_minutes: u64,
    pub lease_seconds: u64,
    pub max_attempts: i32,
    pub max_duration_hours: u64,
    pub max_pixels: u64,
    pub max_output_size_mb: u64,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EntryCacheConfig {
    pub enabled: bool,
    pub capacity: u64,
    pub time_to_idle_minutes: u64,
    pub time_to_live_hours: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    #[serde(default)]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub additional_directories: Vec<PathBuf>,
    pub allow_root_routes: bool,
    pub allow_admin_components: bool,
    pub runtime: PluginRuntimeConfig,
    pub compilation_cache: PluginCompilationCacheConfig,
    pub storage: PluginStorageConfig,
    pub public_mail: PluginPublicMailConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRuntimeConfig {
    pub fuel: u64,
    #[serde(default)]
    pub fuel_overrides: BTreeMap<String, u64>,
    pub memory_limit_mb: u64,
    pub module_size_limit_mb: u64,
    pub timeout_seconds: u64,
    pub max_concurrency: usize,
    pub max_host_calls: usize,
    pub request_body_limit_mb: u64,
    pub response_body_limit_mb: u64,
    pub route_cache_limit_mb: u64,
    #[serde(default = "default_plugin_database_cache_limit_mb")]
    pub database_cache_limit_mb: u64,
    #[serde(default = "default_plugin_database_cache_ttl_seconds")]
    pub database_cache_ttl_seconds: u64,
    #[serde(default)]
    pub metrics_enabled: bool,
}

impl PluginRuntimeConfig {
    pub fn fuel_for(&self, plugin_id: &str) -> u64 {
        self.fuel_overrides
            .get(plugin_id)
            .copied()
            .unwrap_or(self.fuel)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCompilationCacheConfig {
    pub enabled: bool,
    #[serde(default)]
    pub directory: Option<PathBuf>,
    pub max_size_mb: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginStorageConfig {
    #[serde(default)]
    pub private_directory: Option<PathBuf>,
    pub write_limit_mb: u64,
    pub quota_mb: u64,
    pub file_links_max_age_hours: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPublicMailConfig {
    pub interval_minutes: u64,
    pub max_clients: usize,
}

#[derive(Debug)]
pub enum ConfigError {
    NotFound(Vec<PathBuf>),
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml_edit::de::Error,
    },
    Invalid(String),
    AlreadyInitialized,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(paths) => write!(
                formatter,
                "no configuration file found; searched {}",
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Read { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "failed to parse {}: {source}", path.display())
            }
            Self::Invalid(message) => write!(formatter, "invalid configuration: {message}"),
            Self::AlreadyInitialized => formatter.write_str("configuration is already initialized"),
        }
    }
}

impl std::error::Error for ConfigError {}

fn default_database_connections() -> u32 {
    50
}

const fn default_plugin_database_cache_limit_mb() -> u64 {
    32
}

const fn default_plugin_database_cache_ttl_seconds() -> u64 {
    300
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            database: DatabaseConfig {
                url: "postgres://postgres:nuR1234@127.0.0.1/nur_cms".into(),
                max_connections: 50,
            },
            server: ServerConfig {
                listen: "127.0.0.1:8777".into(),
                public_url: None,
                trusted_proxy_cidrs: Vec::new(),
            },
            authentication: AuthenticationConfig {
                disable_two_factor: false,
                access_token_lifetime_minutes: 15,
                refresh_token_lifetime_days: 30,
            },
            comments: CommentsConfig {
                moderation_token_lifetime_days: 14,
            },
            uploads: UploadConfig {
                directory: "/var/www/nur-cms/uploads".into(),
                max_size_mb: 800,
                chunk_size_mb: 10,
                max_active_per_user: 4,
                session_lifetime_hours: 48,
            },
            images: ImageConfig {
                max_pixels: 40_000_000,
                processing_concurrency: 2,
            },
            video: VideoConfig {
                processing_concurrency: 1,
                processing_threads: 2,
                processing_timeout_minutes: 60,
                lease_seconds: 120,
                max_attempts: 3,
                max_duration_hours: 8,
                max_pixels: 33_177_600,
                max_output_size_mb: 0,
                ffmpeg: "ffmpeg".into(),
                ffprobe: "ffprobe".into(),
            },
            entry_cache: EntryCacheConfig {
                enabled: true,
                capacity: 512,
                time_to_idle_minutes: 30,
                time_to_live_hours: 24,
            },
            plugins: PluginConfig {
                enabled: Vec::new(),
                additional_directories: Vec::new(),
                allow_root_routes: false,
                allow_admin_components: false,
                runtime: PluginRuntimeConfig {
                    fuel: 2_000_000,
                    fuel_overrides: BTreeMap::new(),
                    memory_limit_mb: 64,
                    module_size_limit_mb: 64,
                    timeout_seconds: 5,
                    max_concurrency: 8,
                    max_host_calls: 16,
                    request_body_limit_mb: 1,
                    response_body_limit_mb: 4,
                    route_cache_limit_mb: 64,
                    database_cache_limit_mb: default_plugin_database_cache_limit_mb(),
                    database_cache_ttl_seconds: default_plugin_database_cache_ttl_seconds(),
                    metrics_enabled: false,
                },
                compilation_cache: PluginCompilationCacheConfig {
                    enabled: true,
                    directory: None,
                    max_size_mb: 512,
                },
                storage: PluginStorageConfig {
                    private_directory: None,
                    write_limit_mb: 16,
                    quota_mb: 1_024,
                    file_links_max_age_hours: 48,
                },
                public_mail: PluginPublicMailConfig {
                    interval_minutes: 3,
                    max_clients: 10_000,
                },
            },
        }
    }
}

impl AppConfig {
    pub fn parse(path: &Path, source: &str) -> Result<Self, ConfigError> {
        let config: Self =
            toml_edit::de::from_str(source).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != CONFIG_VERSION {
            return Err(ConfigError::Invalid(format!(
                "unsupported configuration version {}; expected {CONFIG_VERSION}",
                self.version
            )));
        }
        if self.database.url.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "database.url must not be empty".into(),
            ));
        }
        if !(1..=1_000).contains(&self.database.max_connections) {
            return Err(ConfigError::Invalid(
                "database.max_connections must be between 1 and 1000".into(),
            ));
        }
        self.server
            .listen
            .parse::<std::net::SocketAddr>()
            .map_err(|_| {
                ConfigError::Invalid("server.listen must be an IP address and port".into())
            })?;
        if let Some(url) = &self.server.public_url
            && crate::utils::public_url::normalize_public_url(url).is_none()
        {
            return Err(ConfigError::Invalid("server.public_url must be HTTPS (or local HTTP) without credentials, query, or fragment".into()));
        }
        for cidr in &self.server.trusted_proxy_cidrs {
            cidr.parse::<ipnet::IpNet>()
                .map_err(|_| ConfigError::Invalid(format!("invalid trusted proxy CIDR: {cidr}")))?;
        }
        bounded(
            "authentication.access_token_lifetime_minutes",
            self.authentication.access_token_lifetime_minutes,
            5,
            1_440,
        )?;
        bounded(
            "authentication.refresh_token_lifetime_days",
            self.authentication.refresh_token_lifetime_days,
            1,
            365,
        )?;
        bounded(
            "comments.moderation_token_lifetime_days",
            self.comments.moderation_token_lifetime_days,
            1,
            30,
        )?;
        nonempty_path("uploads.directory", &self.uploads.directory)?;
        bounded(
            "uploads.max_size_mb",
            self.uploads.max_size_mb,
            1,
            1_048_576,
        )?;
        bounded(
            "uploads.chunk_size_mb",
            self.uploads.chunk_size_mb,
            1,
            16_384,
        )?;
        if self.uploads.chunk_size_mb > self.uploads.max_size_mb {
            return Err(ConfigError::Invalid(
                "uploads.chunk_size_mb must not exceed uploads.max_size_mb".into(),
            ));
        }
        bounded(
            "uploads.max_active_per_user",
            self.uploads.max_active_per_user,
            1,
            1_024,
        )?;
        bounded(
            "uploads.session_lifetime_hours",
            self.uploads.session_lifetime_hours,
            1,
            720,
        )?;
        bounded(
            "images.max_pixels",
            self.images.max_pixels,
            1,
            1_000_000_000,
        )?;
        bounded(
            "images.processing_concurrency",
            self.images.processing_concurrency,
            1,
            16,
        )?;
        bounded(
            "video.processing_concurrency",
            self.video.processing_concurrency,
            1,
            4,
        )?;
        bounded(
            "video.processing_threads",
            self.video.processing_threads,
            1,
            32,
        )?;
        bounded(
            "video.processing_timeout_minutes",
            self.video.processing_timeout_minutes,
            1,
            1_440,
        )?;
        bounded("video.lease_seconds", self.video.lease_seconds, 30, 3_600)?;
        bounded("video.max_attempts", self.video.max_attempts, 1, 10)?;
        bounded(
            "video.max_duration_hours",
            self.video.max_duration_hours,
            1,
            168,
        )?;
        bounded("video.max_pixels", self.video.max_pixels, 1, 132_710_400)?;
        bounded(
            "video.max_output_size_mb",
            self.video.max_output_size_mb,
            0,
            1_048_576,
        )?;
        nonempty_path("video.ffmpeg", &self.video.ffmpeg)?;
        nonempty_path("video.ffprobe", &self.video.ffprobe)?;
        bounded(
            "entry_cache.capacity",
            self.entry_cache.capacity,
            16,
            100_000,
        )?;
        bounded(
            "entry_cache.time_to_idle_minutes",
            self.entry_cache.time_to_idle_minutes,
            1,
            1_440,
        )?;
        bounded(
            "entry_cache.time_to_live_hours",
            self.entry_cache.time_to_live_hours,
            1,
            168,
        )?;
        if self.entry_cache.time_to_live_hours * 60 < self.entry_cache.time_to_idle_minutes {
            return Err(ConfigError::Invalid(
                "entry_cache.time_to_live_hours must not be shorter than time_to_idle_minutes"
                    .into(),
            ));
        }
        bounded(
            "plugins.runtime.fuel",
            self.plugins.runtime.fuel,
            10_000,
            100_000_000,
        )?;
        for (plugin_id, fuel) in &self.plugins.runtime.fuel_overrides {
            bounded(
                &format!("plugins.runtime.fuel_overrides.{plugin_id}"),
                *fuel,
                10_000,
                100_000_000,
            )?;
        }
        bounded(
            "plugins.runtime.memory_limit_mb",
            self.plugins.runtime.memory_limit_mb,
            1,
            512,
        )?;
        bounded(
            "plugins.runtime.module_size_limit_mb",
            self.plugins.runtime.module_size_limit_mb,
            1,
            512,
        )?;
        bounded(
            "plugins.runtime.timeout_seconds",
            self.plugins.runtime.timeout_seconds,
            1,
            60,
        )?;
        bounded(
            "plugins.runtime.max_concurrency",
            self.plugins.runtime.max_concurrency,
            1,
            64,
        )?;
        bounded(
            "plugins.runtime.max_host_calls",
            self.plugins.runtime.max_host_calls,
            1,
            128,
        )?;
        bounded(
            "plugins.runtime.request_body_limit_mb",
            self.plugins.runtime.request_body_limit_mb,
            1,
            16,
        )?;
        bounded(
            "plugins.runtime.response_body_limit_mb",
            self.plugins.runtime.response_body_limit_mb,
            1,
            64,
        )?;
        bounded(
            "plugins.runtime.route_cache_limit_mb",
            self.plugins.runtime.route_cache_limit_mb,
            1,
            1_024,
        )?;
        bounded(
            "plugins.runtime.database_cache_limit_mb",
            self.plugins.runtime.database_cache_limit_mb,
            1,
            1_024,
        )?;
        bounded(
            "plugins.runtime.database_cache_ttl_seconds",
            self.plugins.runtime.database_cache_ttl_seconds,
            1,
            86_400,
        )?;
        bounded(
            "plugins.compilation_cache.max_size_mb",
            self.plugins.compilation_cache.max_size_mb,
            16,
            16_384,
        )?;
        bounded(
            "plugins.storage.write_limit_mb",
            self.plugins.storage.write_limit_mb,
            1,
            256,
        )?;
        bounded(
            "plugins.storage.quota_mb",
            self.plugins.storage.quota_mb,
            1,
            1_048_576,
        )?;
        bounded(
            "plugins.storage.file_links_max_age_hours",
            self.plugins.storage.file_links_max_age_hours,
            1,
            720,
        )?;
        bounded(
            "plugins.public_mail.interval_minutes",
            self.plugins.public_mail.interval_minutes,
            1,
            1_440,
        )?;
        bounded(
            "plugins.public_mail.max_clients",
            self.plugins.public_mail.max_clients,
            128,
            1_000_000,
        )?;
        for directory in &self.plugins.additional_directories {
            nonempty_path("plugins.additional_directories", directory)?;
        }
        if let Some(directory) = &self.plugins.compilation_cache.directory {
            nonempty_path("plugins.compilation_cache.directory", directory)?;
        }
        if let Some(directory) = &self.plugins.storage.private_directory {
            nonempty_path("plugins.storage.private_directory", directory)?;
        }
        if self
            .plugins
            .enabled
            .iter()
            .any(|plugin| plugin.trim().is_empty())
        {
            return Err(ConfigError::Invalid(
                "plugins.enabled must not contain empty IDs".into(),
            ));
        }
        Ok(())
    }
}

fn nonempty_path(name: &str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() {
        Err(ConfigError::Invalid(format!("{name} must not be empty")))
    } else {
        Ok(())
    }
}

fn bounded<T>(name: &str, value: T, minimum: T, maximum: T) -> Result<(), ConfigError>
where
    T: PartialOrd + fmt::Display,
{
    if value < minimum || value > maximum {
        Err(ConfigError::Invalid(format!(
            "{name} must be between {minimum} and {maximum}"
        )))
    } else {
        Ok(())
    }
}

pub fn candidate_paths(explicit: Option<&Path>) -> Vec<PathBuf> {
    if let Some(path) = explicit {
        return vec![path.to_path_buf()];
    }
    let mut paths = vec![PathBuf::from(CONFIG_FILE_NAME)];
    if let Some(base) = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        })
    {
        paths.push(base.join("nur-cms").join(CONFIG_FILE_NAME));
    }
    paths.push(PathBuf::from("/etc/nur-cms").join(CONFIG_FILE_NAME));
    paths
}

pub fn resolve_path(explicit: Option<&Path>) -> Result<PathBuf, ConfigError> {
    let paths = candidate_paths(explicit);
    paths
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .ok_or(ConfigError::NotFound(paths))
}

pub fn load(explicit: Option<&Path>) -> Result<(AppConfig, PathBuf), ConfigError> {
    let selected = resolve_path(explicit)?;
    let source = fs::read_to_string(&selected).map_err(|source| ConfigError::Read {
        path: selected.clone(),
        source,
    })?;
    let (config, _, _) = migrate_source(&selected, &source)?;
    Ok((config, selected))
}

/// Applies known version migrations in memory and validates the final result.
///
/// Package installation persists the returned source through `config migrate`;
/// regular startup can still consume a supported older version read-only.
pub fn migrate_source(path: &Path, source: &str) -> Result<(AppConfig, String, bool), ConfigError> {
    #[derive(Deserialize)]
    struct Version {
        version: u32,
    }

    let mut version = toml_edit::de::from_str::<Version>(source)
        .map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?
        .version;
    if version > CONFIG_VERSION {
        return Err(ConfigError::Invalid(format!(
            "configuration version {version} is newer than supported version {CONFIG_VERSION}"
        )));
    }

    let mut migrated = source.to_owned();
    let mut changed = false;
    while version < CONFIG_VERSION {
        migrated = migrate_version(version, &migrated)?;
        version += 1;
        changed = true;
    }
    let config = AppConfig::parse(path, &migrated)?;
    Ok((config, migrated, changed))
}

fn migrate_version(version: u32, _source: &str) -> Result<String, ConfigError> {
    Err(ConfigError::Invalid(format!(
        "no migration is available from configuration version {version}"
    )))
}

pub fn install(config: AppConfig) -> Result<(), ConfigError> {
    CONFIG
        .set(config)
        .map_err(|_| ConfigError::AlreadyInitialized)
}

pub fn settings() -> &'static AppConfig {
    CONFIG.get_or_init(default_settings)
}

fn default_settings() -> AppConfig {
    #[cfg(test)]
    {
        let mut config = AppConfig::default();
        config.uploads.directory = "./uploads".into();
        config
    }
    #[cfg(not(test))]
    AppConfig::default()
}

pub const fn mb(value: u64) -> u64 {
    value * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, candidate_paths, migrate_source};
    use std::path::Path;

    #[test]
    fn explicit_path_disables_fallbacks() {
        assert_eq!(
            candidate_paths(Some(Path::new("instance.toml"))),
            [Path::new("instance.toml")]
        );
    }

    #[test]
    fn rejects_unknown_keys() {
        let source =
            toml_edit::ser::to_string_pretty(&AppConfig::default()).unwrap() + "\nunknown = true\n";
        assert!(AppConfig::parse(Path::new("test.toml"), &source).is_err());
    }

    #[test]
    fn current_config_migration_preserves_the_source() {
        let source = toml_edit::ser::to_string_pretty(&AppConfig::default()).unwrap();
        let (_, migrated, changed) = migrate_source(Path::new("test.toml"), &source).unwrap();
        assert!(!changed);
        assert_eq!(migrated, source);
    }

    #[test]
    fn rejects_configs_from_a_newer_version() {
        let source = toml_edit::ser::to_string_pretty(&AppConfig::default())
            .unwrap()
            .replacen("version = 1", "version = 2", 1);
        assert!(migrate_source(Path::new("test.toml"), &source).is_err());
    }

    #[test]
    fn plugin_fuel_overrides_are_scoped_and_validated() {
        let mut config = AppConfig::default();
        config
            .plugins
            .runtime
            .fuel_overrides
            .insert("blog".into(), 5_000_000);

        assert_eq!(config.plugins.runtime.fuel_for("blog"), 5_000_000);
        assert_eq!(config.plugins.runtime.fuel_for("another-plugin"), 2_000_000);
        assert!(config.validate().is_ok());

        config
            .plugins
            .runtime
            .fuel_overrides
            .insert("broken".into(), 9_999);
        assert!(config.validate().is_err());
    }
}
