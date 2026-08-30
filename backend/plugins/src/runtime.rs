use std::{sync::Arc, time::Duration};

use tokio::sync::Semaphore;
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, Linker},
};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::{Error, manifest::InstalledPlugin};

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
}

#[derive(Clone)]
pub struct PluginComponent {
    pub id: String,
    component: Component,
    runtime: Runtime,
}

struct HostState {
    limits: StoreLimits,
    table: ResourceTable,
    wasi: WasiCtx,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl Runtime {
    pub fn new() -> Result<Self, Error> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.wasm_component_model(true);
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
            timeout: Duration::from_millis(env_u64("NUR_PLUGIN_TIMEOUT_MS", 5_000, 100, 60_000)),
            semaphore: Arc::new(Semaphore::new(env_usize(
                "NUR_PLUGIN_MAX_CONCURRENCY",
                8,
                1,
                64,
            ))),
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

impl PluginComponent {
    pub async fn call(
        &self,
        request: bindings::nur::cms::types::Request,
    ) -> Result<bindings::nur::cms::types::Response, Error> {
        let permit = Arc::clone(&self.runtime.semaphore)
            .acquire_owned()
            .await
            .map_err(|_| Error::Plugin("plugin runtime is shutting down".into()))?;
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
                limits: StoreLimitsBuilder::new()
                    .memory_size(self.runtime.memory_limit)
                    .instances(128)
                    .tables(16)
                    .memories(4)
                    .trap_on_grow_failure(true)
                    .build(),
                table: ResourceTable::new(),
                wasi: WasiCtxBuilder::new().build(),
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
    use std::path::PathBuf;

    use super::{PluginComponent, Runtime, bindings};

    #[tokio::test]
    async fn invokes_built_echo_component_when_available() {
        let module = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples/echo/target/wasm32-wasip2/release/nur_cms_echo_plugin.wasm");
        if !module.is_file() {
            return;
        }

        let runtime = Runtime::new().expect("runtime initializes");
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
}
