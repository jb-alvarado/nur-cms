use nur_core::{
    db::{
        fields::{ContentEntryFields, ContentNodeFields, OutputType},
        handles,
        queries::QueryObj,
    },
    utils::content_output::render_entry_nodes,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tracing::{error, info, warn};
use wasmtime::{
    Cache, CacheConfig, Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, Linker},
};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::{Error, manifest::InstalledPlugin, plugin_timeout};

pub mod bindings {
    wasmtime::component::bindgen!({
        path: "wit/nur-cms-plugin",
        world: "cms-plugin",
    });
}

const EPOCH_INTERVAL_MS: u64 = 10;

#[derive(Clone)]
pub struct Runtime {
    engine: Arc<Engine>,
    fuel: u64,
    memory_limit: usize,
    timeout: Duration,
    semaphore: Arc<Semaphore>,
    pool: sqlx::PgPool,
    tokio_handle: tokio::runtime::Handle,
    max_host_calls: usize,
    content_response_body_limit: usize,
}

#[derive(Clone)]
pub struct PluginComponent {
    pub id: String,
    component: Component,
    runtime: Runtime,
}

struct HostState {
    plugin_id: String,
    limits: StoreLimits,
    table: ResourceTable,
    wasi: WasiCtx,
    pool: sqlx::PgPool,
    tokio_handle: tokio::runtime::Handle,
    host_calls_remaining: usize,
    host_call_timeout: Duration,
    content_response_body_limit: usize,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl bindings::nur::cms::types::Host for HostState {}

impl Runtime {
    pub fn new(pool: sqlx::PgPool) -> Result<Self, Error> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.wasm_component_model(true);
        configure_compilation_cache(&mut config)?;
        let engine = Arc::new(Engine::new(&config).map_err(Error::wasmtime)?);
        let epoch_engine = Arc::clone(&engine);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(EPOCH_INTERVAL_MS));
            loop {
                interval.tick().await;
                epoch_engine.increment_epoch();
            }
        });

        Ok(Self {
            engine,
            fuel: env_u64("NUR_PLUGIN_FUEL", 1_000_000, 10_000, 100_000_000),
            memory_limit: env_usize(
                "NUR_PLUGIN_MEMORY_LIMIT",
                64 * 1024 * 1024,
                1024 * 1024,
                512 * 1024 * 1024,
            ),
            timeout: plugin_timeout(),
            semaphore: Arc::new(Semaphore::new(env_usize(
                "NUR_PLUGIN_MAX_CONCURRENCY",
                8,
                1,
                64,
            ))),
            pool,
            tokio_handle: tokio::runtime::Handle::current(),
            max_host_calls: env_usize("NUR_PLUGIN_MAX_HOST_CALLS", 16, 1, 128),
            content_response_body_limit: env_usize(
                "NUR_PLUGIN_RESPONSE_BODY_LIMIT",
                4 * 1024 * 1024,
                1024,
                64 * 1024 * 1024,
            ),
        })
    }

    pub fn load(&self, plugin: &InstalledPlugin) -> Result<PluginComponent, Error> {
        let module_limit = env_u64(
            "NUR_PLUGIN_MODULE_SIZE_LIMIT",
            64 * 1024 * 1024,
            1024,
            512 * 1024 * 1024,
        );
        if std::fs::metadata(&plugin.module).map_err(Error::Io)?.len() > module_limit {
            return Err(Error::Plugin(format!(
                "plugin '{}' module exceeds the configured size limit",
                plugin.manifest.plugin.id
            )));
        }
        let component = Component::from_file(&self.engine, &plugin.module)
            .map_err(|error| Error::Plugin(format!("{}: {error}", plugin.module.display())))?;
        Ok(PluginComponent {
            id: plugin.manifest.plugin.id.clone(),
            component,
            runtime: self.clone(),
        })
    }
}

fn configure_compilation_cache(config: &mut Config) -> Result<(), Error> {
    if std::env::var("NUR_PLUGIN_COMPILATION_CACHE").as_deref() == Ok("0") {
        info!("plugin compilation cache is disabled");
        return Ok(());
    }

    let directory = std::env::var_os("NUR_PLUGIN_COMPILATION_CACHE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let explicitly_configured = directory.is_some();
    let mut cache_config = CacheConfig::new();
    if let Some(directory) = directory {
        cache_config.with_directory(directory);
    }
    cache_config.with_files_total_size_soft_limit(env_u64(
        "NUR_PLUGIN_COMPILATION_CACHE_SIZE",
        512 * 1024 * 1024,
        16 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
    ));

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

impl PluginComponent {
    pub async fn call(
        &self,
        request: bindings::nur::cms::types::Request,
    ) -> Result<bindings::nur::cms::types::Response, Error> {
        let permit = acquire_runtime_permit(Arc::clone(&self.runtime.semaphore))?;
        let component = self.clone();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            component.call_sync(request)
        });
        tokio::time::timeout(self.runtime.timeout + Duration::from_millis(100), task)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(Error::Join)?
    }

    fn call_sync(
        &self,
        request: bindings::nur::cms::types::Request,
    ) -> Result<bindings::nur::cms::types::Response, Error> {
        let mut linker = Linker::new(&self.runtime.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(Error::wasmtime)?;
        let mut store = Store::new(
            &self.runtime.engine,
            HostState {
                plugin_id: self.id.clone(),
                limits: StoreLimitsBuilder::new()
                    .memory_size(self.runtime.memory_limit)
                    .instances(128)
                    .tables(16)
                    .memories(4)
                    .trap_on_grow_failure(true)
                    .build(),
                table: ResourceTable::new(),
                wasi: WasiCtxBuilder::new().build(),
                pool: self.runtime.pool.clone(),
                tokio_handle: self.runtime.tokio_handle.clone(),
                host_calls_remaining: self.runtime.max_host_calls,
                host_call_timeout: self.runtime.timeout,
                content_response_body_limit: self.runtime.content_response_body_limit,
            },
        );
        store.limiter(|state| &mut state.limits);
        store.set_fuel(self.runtime.fuel).map_err(Error::wasmtime)?;
        let ticks = self
            .runtime
            .timeout
            .as_millis()
            .div_ceil(u128::from(EPOCH_INTERVAL_MS));
        store.set_epoch_deadline(u64::try_from(ticks).unwrap_or(u64::MAX));

        bindings::CmsPlugin::add_to_linker::<HostState, wasmtime::component::HasSelf<HostState>>(
            &mut linker,
            |state| state,
        )
        .map_err(Error::wasmtime)?;
        let instance = bindings::CmsPlugin::instantiate(&mut store, &self.component, &linker)
            .map_err(Error::wasmtime)?;
        instance
            .nur_cms_http_handler()
            .call_handle(&mut store, &request)
            .map_err(Error::wasmtime)?
            .map_err(|error| match error {
                bindings::nur::cms::types::PluginError::BadRequest(message) => {
                    Error::PluginBadRequest(message)
                }
                bindings::nur::cms::types::PluginError::Forbidden => Error::PluginForbidden,
                bindings::nur::cms::types::PluginError::NotFound => Error::PluginNotFound,
                bindings::nur::cms::types::PluginError::Failed(message) => Error::Plugin(message),
            })
    }
}

fn acquire_runtime_permit(semaphore: Arc<Semaphore>) -> Result<OwnedSemaphorePermit, Error> {
    semaphore.try_acquire_owned().map_err(|error| match error {
        TryAcquireError::NoPermits => Error::Busy,
        TryAcquireError::Closed => Error::Plugin("plugin runtime is shutting down".into()),
    })
}

impl bindings::nur::cms::content::Host for HostState {
    fn published_entries(
        &mut self,
        query: String,
        output: bindings::nur::cms::content::OutputType,
    ) -> Result<Vec<u8>, bindings::nur::cms::types::PluginError> {
        if self.host_calls_remaining == 0 {
            return Err(bindings::nur::cms::types::PluginError::Failed(
                "plugin host-call limit exceeded".into(),
            ));
        }
        self.host_calls_remaining -= 1;

        if query.len() > 8 * 1024 {
            return Err(bindings::nur::cms::types::PluginError::BadRequest(
                "content query is too long".into(),
            ));
        }

        let mut params: QueryObj<ContentEntryFields> = match serde_urlencoded::from_str(&query) {
            Ok(params) => params,
            Err(_) => {
                return Err(bindings::nur::cms::types::PluginError::BadRequest(
                    "invalid content query".into(),
                ));
            }
        };
        params.path = "/api/content/entries".into();
        params.query = query;
        params.search_status = Some("published".into());
        let output = match output {
            bindings::nur::cms::content::OutputType::Markdown => OutputType::Markdown,
            bindings::nur::cms::content::OutputType::Ast => OutputType::AST,
            bindings::nur::cms::content::OutputType::Html => OutputType::HTML,
        };
        if params
            .fields
            .contains(&ContentEntryFields::Node(ContentNodeFields::Text))
            && !params
                .fields
                .contains(&ContentEntryFields::Node(ContentNodeFields::Embeds))
            && output == OutputType::AST
        {
            params
                .fields
                .push(ContentEntryFields::Node(ContentNodeFields::Embeds));
        }

        let result = self.tokio_handle.block_on(async {
            tokio::time::timeout(self.host_call_timeout, async {
                let mut entries = handles::select_content_entries(&self.pool, &params).await?;
                if params
                    .fields
                    .contains(&ContentEntryFields::Node(ContentNodeFields::Text))
                {
                    render_entry_nodes(&mut entries.results, &output, params.character_limit)?;
                }
                serde_json::to_vec(&entries).map_err(nur_core::utils::errors::NurError::from)
            })
            .await
        });
        match result {
            Ok(Ok(entries)) if entries.len() <= self.content_response_body_limit => Ok(entries),
            Ok(Ok(_)) => Err(bindings::nur::cms::types::PluginError::Failed(
                "content response exceeds plugin limit".into(),
            )),
            Ok(Err(error)) => {
                error!(plugin = %self.plugin_id, %error, "plugin content query failed");
                Err(bindings::nur::cms::types::PluginError::Failed(
                    "content query failed".into(),
                ))
            }
            Err(_) => Err(bindings::nur::cms::types::PluginError::Failed(
                "content query timed out".into(),
            )),
        }
    }
}

fn env_u64(key: &str, default: u64, minimum: u64, maximum: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize, minimum: usize, maximum: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::Arc};

    use tokio::sync::Semaphore;

    use crate::Error;

    use super::{PluginComponent, Runtime, acquire_runtime_permit, bindings};

    #[test]
    fn rejects_work_when_runtime_capacity_is_exhausted() {
        let semaphore = Arc::new(Semaphore::new(1));
        let _permit =
            acquire_runtime_permit(Arc::clone(&semaphore)).expect("capacity is available");

        assert!(matches!(
            acquire_runtime_permit(semaphore),
            Err(Error::Busy)
        ));
    }

    #[tokio::test]
    #[ignore = "requires a prebuilt wasm32-wasip2 echo example"]
    async fn invokes_built_echo_component_when_available() {
        let module = example_component("echo", "nur_cms_echo_plugin");
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/nur_cms")
            .expect("test pool initializes");
        let runtime = Runtime::new(pool).expect("runtime initializes");
        let component = wasmtime::component::Component::from_file(&runtime.engine, module)
            .expect("example component loads");
        let plugin = PluginComponent {
            id: "echo".into(),
            component,
            runtime,
        };
        let response = plugin
            .call(bindings::nur::cms::types::Request {
                route_id: "root".into(),
                method: "GET".into(),
                path: "/plugin-echo".into(),
                path_params: Vec::new(),
                query: None,
                headers: Vec::new(),
                body: Vec::new(),
                identity: None,
            })
            .await
            .expect("example request succeeds");

        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"Hello from a nur-cms root plugin route");
    }

    #[tokio::test]
    #[ignore = "requires a prebuilt wasm32-wasip2 community-site example"]
    async fn loads_a_component_with_the_content_import_when_available() {
        let module = example_component("community-site", "nur_cms_community_site_plugin");
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/nur_cms")
            .expect("test pool initializes");
        let runtime = Runtime::new(pool).expect("runtime initializes");
        let component = wasmtime::component::Component::from_file(&runtime.engine, module)
            .expect("example component loads");
        let plugin = PluginComponent {
            id: "community-site".into(),
            component,
            runtime,
        };
        let result = plugin
            .call(bindings::nur::cms::types::Request {
                route_id: "missing".into(),
                method: "GET".into(),
                path: "/missing".into(),
                path_params: Vec::new(),
                query: None,
                headers: Vec::new(),
                body: Vec::new(),
                identity: None,
            })
            .await;

        assert!(matches!(result, Err(Error::PluginNotFound)));
    }

    fn example_component(example: &str, artifact: &str) -> PathBuf {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(example)
            .join("target/wasm32-wasip2");
        let release = target.join("release").join(format!("{artifact}.wasm"));
        if release.is_file() {
            return release;
        }

        fs::read_dir(target.join("debug/deps"))
            .expect("WASIp2 example test component is built")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("wasm")
                    && path
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with(artifact))
            })
            .expect("WASIp2 example test component exists")
    }
}
