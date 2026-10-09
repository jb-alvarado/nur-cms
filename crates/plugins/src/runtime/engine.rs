use std::{sync::Arc, time::Duration};

use tokio::time::interval;
use tracing::{info, warn};
use wasmtime::{Cache, CacheConfig, Config, Engine};

use crate::Error;

pub(super) const EPOCH_INTERVAL_MS: u64 = 10;

pub(super) fn create_engine() -> Result<Arc<Engine>, Error> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.wasm_component_model(true);
    configure_compilation_cache(&mut config)?;

    Engine::new(&config).map(Arc::new).map_err(Error::wasmtime)
}

pub(super) fn start_epoch_ticker(engine: Arc<Engine>) {
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_millis(EPOCH_INTERVAL_MS));

        loop {
            interval.tick().await;
            engine.increment_epoch();
        }
    });
}

fn configure_compilation_cache(config: &mut Config) -> Result<(), Error> {
    let settings = &nur_core::config::settings().plugins.compilation_cache;

    if !settings.enabled {
        info!("plugin compilation cache is disabled");
        return Ok(());
    }

    let directory = settings.directory.clone();
    let explicitly_configured = directory.is_some();
    let mut cache_config = CacheConfig::new();

    if let Some(directory) = directory {
        cache_config.with_directory(directory);
    }

    cache_config.with_files_total_size_soft_limit(nur_core::config::mb(settings.max_size_mb));

    match Cache::new(cache_config) {
        Ok(cache) => {
            info!(directory = %cache.directory().display(), "enabled plugin compilation cache");
            config.cache(Some(cache));
            Ok(())
        }
        Err(error) if explicitly_configured => Err(Error::Plugin(format!(
            "failed to configure plugin compilation cache: {error}"
        ))),
        Err(error) => {
            warn!(%error, "plugin compilation cache is unavailable; continuing without it");
            Ok(())
        }
    }
}
