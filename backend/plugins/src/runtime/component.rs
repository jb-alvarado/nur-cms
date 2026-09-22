use std::{
    fs,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use colored::Colorize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tracing::info;
use wasmtime::{
    Engine, Store,
    component::{Component, Linker},
};

use super::{
    bindings::{
        self,
        nur::cms::types::{PluginError, Request, Response},
    },
    engine::{EPOCH_INTERVAL_MS, create_engine, start_epoch_ticker},
    host::HostState,
    mail_limiter::{MailPermissions, PublicMailRateLimiter},
};
use crate::{
    Error,
    manifest::InstalledPlugin,
    plugin_timeout,
    storage::{PluginStorage, StorageDirectory},
};

#[derive(Clone)]
pub struct Runtime {
    pub(super) engine: Arc<Engine>,
    pub(super) fuel: u64,
    pub(super) memory_limit: usize,
    pub(super) timeout: Duration,
    pub(super) semaphore: Arc<Semaphore>,
    pub(super) pool: sqlx::PgPool,
    pub(super) tokio_handle: tokio::runtime::Handle,
    pub(super) max_host_calls: usize,
    pub(super) content_response_body_limit: usize,
    pub(super) metrics_enabled: bool,
    pub(super) public_mail_rate_limiter: Arc<Mutex<PublicMailRateLimiter>>,
    pub(super) storage: Option<PluginStorage>,
    pub(super) storage_write_limit: usize,
    pub(super) storage_quota: u64,
}

#[derive(Clone)]
pub struct PluginComponent {
    pub id: String,
    pub(super) component: Component,
    pub(super) runtime: Runtime,
    pub(super) mail_permissions: MailPermissions,
    pub(super) storage_directories: Arc<Vec<StorageDirectory>>,
}

impl Runtime {
    pub fn new(pool: sqlx::PgPool, storage: Option<PluginStorage>) -> Result<Self, Error> {
        let engine = create_engine()?;
        start_epoch_ticker(Arc::clone(&engine));

        let settings = &nur_core::config::settings().plugins;
        let runtime = &settings.runtime;
        let storage_settings = &settings.storage;
        let public_mail = &settings.public_mail;

        Ok(Self {
            engine,
            fuel: runtime.fuel,
            memory_limit: nur_core::config::mb(runtime.memory_limit_mb) as usize,
            timeout: plugin_timeout(),
            semaphore: Arc::new(Semaphore::new(runtime.max_concurrency)),
            pool,
            tokio_handle: tokio::runtime::Handle::current(),
            max_host_calls: runtime.max_host_calls,
            content_response_body_limit: nur_core::config::mb(runtime.response_body_limit_mb)
                as usize,
            metrics_enabled: runtime.metrics_enabled,
            public_mail_rate_limiter: Arc::new(Mutex::new(PublicMailRateLimiter::new(
                Duration::from_secs(public_mail.interval_minutes * 60),
                public_mail.max_clients,
            ))),
            storage,
            storage_write_limit: nur_core::config::mb(storage_settings.write_limit_mb) as usize,
            storage_quota: nur_core::config::mb(storage_settings.quota_mb),
        })
    }

    pub fn load(&self, plugin: &InstalledPlugin) -> Result<PluginComponent, Error> {
        let module_limit = nur_core::config::mb(
            nur_core::config::settings()
                .plugins
                .runtime
                .module_size_limit_mb,
        );

        if fs::metadata(&plugin.module).map_err(Error::Io)?.len() > module_limit {
            return Err(Error::Plugin(format!(
                "plugin '{}' module exceeds the configured size limit",
                plugin.manifest.plugin.id
            )));
        }

        let component = Component::from_file(&self.engine, &plugin.module)
            .map_err(|error| Error::Plugin(format!("{}: {error}", plugin.module.display())))?;

        let mut runtime = self.clone();
        runtime.fuel = nur_core::config::settings()
            .plugins
            .runtime
            .fuel_for(&plugin.manifest.plugin.id);

        Ok(PluginComponent {
            id: plugin.manifest.plugin.id.clone(),
            component,
            runtime,
            mail_permissions: MailPermissions::from_plugin(plugin),
            storage_directories: plugin_storage_directories(plugin)?,
        })
    }
}

fn plugin_storage_directories(
    plugin: &InstalledPlugin,
) -> Result<Arc<Vec<StorageDirectory>>, Error> {
    plugin
        .manifest
        .storage
        .directories
        .iter()
        .map(|directory| StorageDirectory::from_manifest(&plugin.manifest.plugin.id, directory))
        .collect::<Result<Vec<_>, _>>()
        .map(Arc::new)
}

impl PluginComponent {
    pub async fn call(
        &self,
        request: Request,
        public_route: bool,
        client_ip: Option<IpAddr>,
    ) -> Result<Response, Error> {
        let permit = acquire_runtime_permit(Arc::clone(&self.runtime.semaphore))?;
        let component = self.clone();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            component.call_sync(request, public_route, client_ip)
        });

        tokio::time::timeout(self.runtime.timeout + Duration::from_millis(100), task)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(Error::Join)?
    }

    fn call_sync(
        &self,
        request: Request,
        public_route: bool,
        client_ip: Option<IpAddr>,
    ) -> Result<Response, Error> {
        let call_started = Instant::now();
        let mut linker = Linker::new(&self.runtime.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(Error::wasmtime)?;

        let host_state = HostState::new(self, &request, public_route, client_ip);
        let mut store = Store::new(&self.runtime.engine, host_state);

        store.limiter(|state| &mut state.limits);
        store.set_fuel(self.runtime.fuel).map_err(Error::wasmtime)?;

        let ticks = self
            .runtime
            .timeout
            .as_millis()
            .div_ceil(u128::from(EPOCH_INTERVAL_MS));
        store.set_epoch_deadline(u64::try_from(ticks).unwrap_or(u64::MAX));

        let instantiate_started = Instant::now();

        bindings::CmsPlugin::add_to_linker::<HostState, wasmtime::component::HasSelf<HostState>>(
            &mut linker,
            |state| state,
        )
        .map_err(Error::wasmtime)?;

        let instance = match bindings::CmsPlugin::instantiate(&mut store, &self.component, &linker)
        {
            Ok(instance) => instance,
            Err(error) => {
                self.log_metrics(
                    "instantiate",
                    instantiate_started.elapsed(),
                    call_started.elapsed(),
                    self.runtime.fuel,
                    &store,
                );
                return Err(Error::wasmtime(error));
            }
        };

        self.log_metrics(
            "instantiate",
            instantiate_started.elapsed(),
            call_started.elapsed(),
            self.runtime.fuel,
            &store,
        );

        let fuel_after_instantiation = store.get_fuel().unwrap_or(0);
        let handler_started = Instant::now();
        let result = instance
            .nur_cms_http_handler()
            .call_handle(&mut store, &request)
            .map_err(Error::wasmtime)
            .and_then(|result| result.map_err(plugin_call_error));

        self.log_metrics(
            "handler",
            handler_started.elapsed(),
            call_started.elapsed(),
            fuel_after_instantiation,
            &store,
        );

        result
    }

    fn log_metrics(
        &self,
        phase: &'static str,
        phase_elapsed: Duration,
        total_elapsed: Duration,
        fuel_at_phase_start: u64,
        store: &Store<HostState>,
    ) {
        if !self.runtime.metrics_enabled {
            return;
        }

        let fuel_remaining = store.get_fuel().unwrap_or(0);
        let host_calls = self
            .runtime
            .max_host_calls
            .saturating_sub(store.data().host_calls_remaining);
        let phase_ms = format!("{:.2}", phase_elapsed.as_secs_f64() * 1_000.0).yellow();
        let total_ms = format!("{:.2}", total_elapsed.as_secs_f64() * 1_000.0).yellow();
        let fuel_budget = self.runtime.fuel.to_string().yellow();
        let phase_fuel_used = fuel_at_phase_start
            .saturating_sub(fuel_remaining)
            .to_string()
            .yellow();
        let total_fuel_used = self
            .runtime
            .fuel
            .saturating_sub(fuel_remaining)
            .to_string()
            .yellow();
        let fuel_remaining = fuel_remaining.to_string().yellow();
        let host_calls = host_calls.to_string().yellow();

        info!(
            plugin = %self.id,
            phase,
            phase_ms = %phase_ms,
            total_ms = %total_ms,
            fuel_budget = %fuel_budget,
            phase_fuel_used = %phase_fuel_used,
            total_fuel_used = %total_fuel_used,
            fuel_remaining = %fuel_remaining,
            host_calls = %host_calls,
            "plugin runtime metrics"
        );
    }
}

fn plugin_call_error(error: PluginError) -> Error {
    match error {
        PluginError::BadRequest(message) => Error::PluginBadRequest(message),
        PluginError::RateLimited => Error::RateLimited,
        PluginError::Forbidden => Error::PluginForbidden,
        PluginError::NotFound => Error::PluginNotFound,
        PluginError::Failed(message) => Error::Plugin(message),
    }
}

pub(super) fn acquire_runtime_permit(
    semaphore: Arc<Semaphore>,
) -> Result<OwnedSemaphorePermit, Error> {
    semaphore.try_acquire_owned().map_err(|error| match error {
        TryAcquireError::NoPermits => Error::Busy,
        TryAcquireError::Closed => Error::Plugin("plugin runtime is shutting down".into()),
    })
}
