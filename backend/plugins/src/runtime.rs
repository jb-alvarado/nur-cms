use colored::Colorize;
use nur_core::{
    db::{
        fields::{ContentEntryFields, ContentNodeFields, OutputType},
        handles,
        queries::QueryObj,
    },
    mail::service::{MailRequest, PluginMailError, deliver_plugin_mail, prepare_plugin_mail},
    utils::{content_output::render_entry_nodes, public_url::configured_public_url},
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::IpAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tracing::{error, info, warn};
use wasmtime::{
    Cache, CacheConfig, Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, Linker},
};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::{Error, manifest::InstalledPlugin, plugin_timeout};

mod database;

use database::{
    DatabaseHostError, execute_statements, validate_statement, validate_transaction_size,
};

pub mod bindings {
    wasmtime::component::bindgen!({
        path: "wit/nur-cms-plugin",
        world: "cms-plugin",
    });
}

const EPOCH_INTERVAL_MS: u64 = 10;
const MAX_MAIL_CALLS_PER_REQUEST: u8 = 3;

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
    metrics_enabled: bool,
    public_mail_rate_limiter: Arc<Mutex<PublicMailRateLimiter>>,
}

#[derive(Clone)]
pub struct PluginComponent {
    pub id: String,
    component: Component,
    runtime: Runtime,
    mail_permissions: MailPermissions,
}

#[derive(Clone, Default)]
struct MailPermissions {
    targets: Arc<HashSet<String>>,
    dynamic_recipient_targets: Arc<HashSet<String>>,
}

impl MailPermissions {
    fn allows(&self, target: &str, dynamic_recipient: bool) -> bool {
        self.targets.contains(target)
            && (!dynamic_recipient || self.dynamic_recipient_targets.contains(target))
    }
}

struct HostState {
    plugin_id: String,
    plugin_schema: String,
    limits: StoreLimits,
    table: ResourceTable,
    wasi: WasiCtx,
    pool: sqlx::PgPool,
    tokio_handle: tokio::runtime::Handle,
    host_calls_remaining: usize,
    host_call_timeout: Duration,
    content_response_body_limit: usize,
    metrics_enabled: bool,
    public_route: bool,
    route_id: String,
    client_ip: IpAddr,
    public_mail_rate_limiter: Arc<Mutex<PublicMailRateLimiter>>,
    public_mail_authorized: Option<bool>,
    mail_calls_remaining: u8,
    mail_permissions: MailPermissions,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PublicMailKey {
    plugin_id: String,
    route_id: String,
    client_ip: IpAddr,
}

struct PublicMailRateLimiter {
    sent: HashMap<PublicMailKey, Instant>,
    expirations: VecDeque<(Instant, PublicMailKey)>,
    window: Duration,
    max_clients: usize,
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
            metrics_enabled: env_bool("NUR_PLUGIN_METRICS", false),
            public_mail_rate_limiter: Arc::new(Mutex::new(PublicMailRateLimiter {
                sent: HashMap::new(),
                expirations: VecDeque::new(),
                window: Duration::from_secs(env_u64(
                    "NUR_PLUGIN_PUBLIC_MAIL_INTERVAL_SECONDS",
                    180,
                    1,
                    86_400,
                )),
                max_clients: env_usize(
                    "NUR_PLUGIN_PUBLIC_MAIL_MAX_CLIENTS",
                    10_000,
                    128,
                    1_000_000,
                ),
            })),
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
            mail_permissions: MailPermissions {
                targets: Arc::new(plugin.manifest.mail.targets.iter().cloned().collect()),
                dynamic_recipient_targets: Arc::new(
                    plugin
                        .manifest
                        .mail
                        .dynamic_recipient_targets
                        .iter()
                        .cloned()
                        .collect(),
                ),
            },
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
        public_route: bool,
        client_ip: IpAddr,
    ) -> Result<bindings::nur::cms::types::Response, Error> {
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
        request: bindings::nur::cms::types::Request,
        public_route: bool,
        client_ip: IpAddr,
    ) -> Result<bindings::nur::cms::types::Response, Error> {
        let call_started = Instant::now();
        let mut linker = Linker::new(&self.runtime.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(Error::wasmtime)?;
        let mut store = Store::new(
            &self.runtime.engine,
            HostState {
                plugin_id: self.id.clone(),
                plugin_schema: crate::manifest::schema_name(&self.id),
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
                metrics_enabled: self.runtime.metrics_enabled,
                public_route,
                route_id: request.route_id.clone(),
                client_ip,
                public_mail_rate_limiter: Arc::clone(&self.runtime.public_mail_rate_limiter),
                public_mail_authorized: None,
                mail_calls_remaining: MAX_MAIL_CALLS_PER_REQUEST,
                mail_permissions: self.mail_permissions.clone(),
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
            .and_then(|result| {
                result.map_err(|error| match error {
                    bindings::nur::cms::types::PluginError::BadRequest(message) => {
                        Error::PluginBadRequest(message)
                    }
                    bindings::nur::cms::types::PluginError::RateLimited => Error::RateLimited,
                    bindings::nur::cms::types::PluginError::Forbidden => Error::PluginForbidden,
                    bindings::nur::cms::types::PluginError::NotFound => Error::PluginNotFound,
                    bindings::nur::cms::types::PluginError::Failed(message) => {
                        Error::Plugin(message)
                    }
                })
            });
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

        let host_call_started = Instant::now();
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
        self.log_content_query_metrics(&result, host_call_started.elapsed());
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

impl bindings::nur::cms::database::Host for HostState {
    fn execute(
        &mut self,
        statement: bindings::nur::cms::database::Statement,
    ) -> Result<bindings::nur::cms::database::QueryResult, bindings::nur::cms::types::PluginError>
    {
        self.consume_host_call()?;
        let validated = validate_statement(&statement).map_err(database_error)?;

        let schema = self.plugin_schema.clone();
        let limit = self.content_response_body_limit;
        let started = Instant::now();
        let statements = [statement];
        let validated = [validated];
        let result = self.tokio_handle.block_on(async {
            tokio::time::timeout(
                self.host_call_timeout,
                execute_statements(
                    &self.pool,
                    &schema,
                    &statements,
                    &validated,
                    limit,
                    self.host_call_timeout,
                ),
            )
            .await
        });
        self.log_database_metrics("execute", &result, started.elapsed());
        match result {
            Ok(Ok(mut results)) => results
                .pop()
                .ok_or_else(|| database_error("database query returned no result")),
            Ok(Err(error)) => {
                error!(plugin = %self.plugin_id, %error, "plugin database query failed");
                Err(database_error("database query failed"))
            }
            Err(_) => Err(database_error("database query timed out")),
        }
    }

    fn transaction(
        &mut self,
        statements: Vec<bindings::nur::cms::database::Statement>,
    ) -> Result<
        Vec<bindings::nur::cms::database::QueryResult>,
        bindings::nur::cms::types::PluginError,
    > {
        self.consume_host_call()?;
        validate_transaction_size(&statements).map_err(database_error)?;
        let validated = statements
            .iter()
            .map(validate_statement)
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)?;

        let schema = self.plugin_schema.clone();
        let limit = self.content_response_body_limit;
        let started = Instant::now();
        let result = self.tokio_handle.block_on(async {
            tokio::time::timeout(
                self.host_call_timeout,
                execute_statements(
                    &self.pool,
                    &schema,
                    &statements,
                    &validated,
                    limit,
                    self.host_call_timeout,
                ),
            )
            .await
        });
        self.log_database_metrics("transaction", &result, started.elapsed());
        match result {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) => {
                error!(plugin = %self.plugin_id, %error, "plugin database transaction failed");
                Err(database_error("database transaction failed"))
            }
            Err(_) => Err(database_error("database transaction timed out")),
        }
    }
}

impl bindings::nur::cms::configuration::Host for HostState {
    fn public_url(&mut self) -> Option<String> {
        configured_public_url()
    }
}

impl bindings::nur::cms::mail::Host for HostState {
    fn send(
        &mut self,
        message: bindings::nur::cms::mail::Message,
    ) -> Result<(), bindings::nur::cms::types::PluginError> {
        self.consume_host_call()?;

        if !self
            .mail_permissions
            .allows(&message.target, message.recipient.is_some())
        {
            return Err(bindings::nur::cms::types::PluginError::Forbidden);
        }

        let request = MailRequest {
            reply_to: message.reply_to,
            subject: message.subject,
            name: message.name,
            text: message.text,
        };
        let target = message.target;
        let recipient = message.recipient;
        let started = Instant::now();
        let prepared = self.tokio_handle.block_on(async {
            tokio::time::timeout(
                self.host_call_timeout,
                prepare_plugin_mail(&self.pool, &target, recipient, request),
            )
            .await
        });
        let prepared = match prepared {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(error)) => {
                self.log_mail_metrics(
                    &Ok::<Result<(), _>, tokio::time::error::Elapsed>(Err(error)),
                    started.elapsed(),
                );
                return Err(self.plugin_mail_error(error));
            }
            Err(error) => {
                self.log_mail_metrics(
                    &Err::<Result<(), PluginMailError>, _>(error),
                    started.elapsed(),
                );
                return Err(bindings::nur::cms::types::PluginError::Failed(
                    "mail delivery timed out".into(),
                ));
            }
        };

        self.authorize_mail_send()?;
        let remaining = self.host_call_timeout.saturating_sub(started.elapsed());
        let result = self.tokio_handle.block_on(async {
            tokio::time::timeout(remaining, deliver_plugin_mail(prepared)).await
        });
        self.log_mail_metrics(&result, started.elapsed());
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(self.plugin_mail_error(error)),
            Err(_) => Err(bindings::nur::cms::types::PluginError::Failed(
                "mail delivery timed out".into(),
            )),
        }
    }
}

impl HostState {
    fn plugin_mail_error(&self, error: PluginMailError) -> bindings::nur::cms::types::PluginError {
        match error {
            PluginMailError::UnknownTarget => {
                bindings::nur::cms::types::PluginError::BadRequest("unknown mail target".into())
            }
            PluginMailError::DynamicRecipientNotAllowed => {
                bindings::nur::cms::types::PluginError::BadRequest(
                    "dynamic recipient is not allowed".into(),
                )
            }
            PluginMailError::InvalidMessage => {
                bindings::nur::cms::types::PluginError::BadRequest("invalid mail message".into())
            }
            PluginMailError::Spam => {
                bindings::nur::cms::types::PluginError::BadRequest("mail message rejected".into())
            }
            PluginMailError::DeliveryFailed => {
                error!(plugin = %self.plugin_id, ?error, "plugin mail delivery failed");
                bindings::nur::cms::types::PluginError::Failed("mail delivery failed".into())
            }
        }
    }

    fn consume_host_call(&mut self) -> Result<(), bindings::nur::cms::types::PluginError> {
        if self.host_calls_remaining == 0 {
            return Err(bindings::nur::cms::types::PluginError::Failed(
                "plugin host-call limit exceeded".into(),
            ));
        }
        self.host_calls_remaining -= 1;
        Ok(())
    }

    fn authorize_mail_send(&mut self) -> Result<(), bindings::nur::cms::types::PluginError> {
        let limiter = Arc::clone(&self.public_mail_rate_limiter);
        let plugin_id = self.plugin_id.clone();
        let route_id = self.route_id.clone();
        let client_ip = self.client_ip;
        if allow_mail_for_request(
            &mut self.mail_calls_remaining,
            self.public_route,
            &mut self.public_mail_authorized,
            || {
                let Ok(mut limiter) = limiter.lock() else {
                    return false;
                };
                allow_public_mail(
                    &mut limiter,
                    &plugin_id,
                    &route_id,
                    client_ip,
                    Instant::now(),
                )
            },
        ) {
            Ok(())
        } else {
            Err(bindings::nur::cms::types::PluginError::RateLimited)
        }
    }

    fn log_database_metrics<T>(
        &self,
        operation: &'static str,
        result: &Result<Result<T, DatabaseHostError>, tokio::time::error::Elapsed>,
        elapsed: Duration,
    ) {
        if !self.metrics_enabled {
            return;
        }
        let outcome = match result {
            Ok(Ok(_)) => "ok",
            Ok(Err(_)) => "error",
            Err(_) => "timeout",
        };
        info!(
            plugin = %self.plugin_id,
            host_call = "database",
            operation,
            outcome,
            duration_ms = %format!("{:.2}", elapsed.as_secs_f64() * 1_000.0).yellow(),
            "plugin host-call metrics"
        );
    }

    fn log_mail_metrics<T>(
        &self,
        result: &Result<Result<T, PluginMailError>, tokio::time::error::Elapsed>,
        elapsed: Duration,
    ) {
        if !self.metrics_enabled {
            return;
        }
        let outcome = match result {
            Ok(Ok(_)) => "ok",
            Ok(Err(_)) => "error",
            Err(_) => "timeout",
        };
        info!(
            plugin = %self.plugin_id,
            host_call = "mail",
            outcome,
            duration_ms = %format!("{:.2}", elapsed.as_secs_f64() * 1_000.0).yellow(),
            "plugin host-call metrics"
        );
    }

    fn log_content_query_metrics(
        &self,
        result: &Result<
            Result<Vec<u8>, nur_core::utils::errors::NurError>,
            tokio::time::error::Elapsed,
        >,
        elapsed: Duration,
    ) {
        if !self.metrics_enabled {
            return;
        }

        let (outcome, response_bytes) = match result {
            Ok(Ok(entries)) if entries.len() <= self.content_response_body_limit => {
                ("ok", entries.len())
            }
            Ok(Ok(entries)) => ("response-too-large", entries.len()),
            Ok(Err(_)) => ("error", 0),
            Err(_) => ("timeout", 0),
        };
        info!(
            plugin = %self.plugin_id,
            host_call = "published_entries",
            outcome,
            duration_ms = %format!("{:.2}", elapsed.as_secs_f64() * 1_000.0).yellow(),
            response_bytes = %response_bytes.to_string().yellow(),
            "plugin host-call metrics"
        );
    }
}

fn allow_mail_for_request(
    calls_remaining: &mut u8,
    public_route: bool,
    public_authorized: &mut Option<bool>,
    reserve_public_limit: impl FnOnce() -> bool,
) -> bool {
    let Some(remaining) = calls_remaining.checked_sub(1) else {
        return false;
    };
    *calls_remaining = remaining;
    if !public_route {
        return true;
    }

    *public_authorized.get_or_insert_with(reserve_public_limit)
}

fn allow_public_mail(
    limiter: &mut PublicMailRateLimiter,
    plugin_id: &str,
    route_id: &str,
    client_ip: IpAddr,
    now: Instant,
) -> bool {
    while limiter
        .expirations
        .front()
        .is_some_and(|(expires_at, _)| *expires_at <= now)
    {
        let Some((expires_at, key)) = limiter.expirations.pop_front() else {
            break;
        };
        if limiter.sent.get(&key) == Some(&expires_at) {
            limiter.sent.remove(&key);
        }
    }
    let key = PublicMailKey {
        plugin_id: plugin_id.into(),
        route_id: route_id.into(),
        client_ip,
    };
    if limiter.sent.contains_key(&key) || limiter.sent.len() >= limiter.max_clients {
        return false;
    }
    let expires_at = now.checked_add(limiter.window).unwrap_or(now);
    limiter.sent.insert(key.clone(), expires_at);
    limiter.expirations.push_back((expires_at, key));
    true
}

fn database_error(message: impl Into<String>) -> bindings::nur::cms::types::PluginError {
    bindings::nur::cms::types::PluginError::Failed(message.into())
}

fn env_u64(key: &str, default: u64, minimum: u64, maximum: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .and_then(|value| match value.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
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
    use std::{
        collections::{HashMap, HashSet, VecDeque},
        fs,
        net::{IpAddr, Ipv4Addr},
        path::PathBuf,
        sync::{Arc, Barrier, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use tokio::sync::Semaphore;

    use crate::Error;

    use super::{
        MailPermissions, PluginComponent, PublicMailRateLimiter, Runtime, acquire_runtime_permit,
        allow_mail_for_request, allow_public_mail, bindings,
    };

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

    #[test]
    fn mail_permissions_are_scoped_by_target_and_recipient_mode() {
        let permissions = MailPermissions {
            targets: Arc::new(HashSet::from(["contact".into(), "orders".into()])),
            dynamic_recipient_targets: Arc::new(HashSet::from(["orders".into()])),
        };

        assert!(permissions.allows("contact", false));
        assert!(!permissions.allows("contact", true));
        assert!(permissions.allows("orders", true));
        assert!(!permissions.allows("unknown", false));
    }

    #[test]
    fn public_mail_is_limited_once_per_ip() {
        let mut limiter = PublicMailRateLimiter {
            sent: HashMap::new(),
            expirations: VecDeque::new(),
            window: Duration::from_secs(180),
            max_clients: 10_000,
        };
        let now = Instant::now();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(allow_public_mail(&mut limiter, "echo", "mail", ip, now));
        assert!(!allow_public_mail(&mut limiter, "echo", "mail", ip, now));
        assert!(allow_public_mail(
            &mut limiter,
            "other-plugin",
            "mail",
            ip,
            now,
        ));
        assert!(allow_public_mail(
            &mut limiter,
            "echo",
            "other-mail-route",
            ip,
            now,
        ));
        assert!(allow_public_mail(
            &mut limiter,
            "echo",
            "mail",
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
            now,
        ));
        assert!(allow_public_mail(
            &mut limiter,
            "echo",
            "mail",
            ip,
            now + Duration::from_secs(180),
        ));
    }

    #[test]
    fn one_request_can_send_three_messages_after_one_public_reservation() {
        let mut remaining = 3;
        let mut authorized = None;
        let mut reservations = 0;

        for _ in 0..3 {
            assert!(allow_mail_for_request(
                &mut remaining,
                true,
                &mut authorized,
                || {
                    reservations += 1;
                    true
                },
            ));
        }
        assert!(!allow_mail_for_request(
            &mut remaining,
            true,
            &mut authorized,
            || true,
        ));
        assert_eq!(reservations, 1);
    }

    #[test]
    fn protected_routes_have_the_same_per_request_mail_limit() {
        let mut remaining = 3;
        let mut authorized = None;
        for _ in 0..3 {
            assert!(allow_mail_for_request(
                &mut remaining,
                false,
                &mut authorized,
                || false,
            ));
        }
        assert!(!allow_mail_for_request(
            &mut remaining,
            false,
            &mut authorized,
            || false,
        ));
    }

    #[test]
    fn rejected_public_reservation_is_reused_without_retrying_the_limiter() {
        let mut remaining = 3;
        let mut authorized = None;
        let mut reservations = 0;

        for _ in 0..2 {
            assert!(!allow_mail_for_request(
                &mut remaining,
                true,
                &mut authorized,
                || {
                    reservations += 1;
                    false
                },
            ));
        }

        assert_eq!(reservations, 1);
    }

    #[test]
    fn parallel_requests_cannot_bypass_the_public_limit() {
        let limiter = Arc::new(Mutex::new(PublicMailRateLimiter {
            sent: HashMap::new(),
            expirations: VecDeque::new(),
            window: Duration::from_secs(180),
            max_clients: 10_000,
        }));
        let barrier = Arc::new(Barrier::new(8));
        let now = Instant::now();
        let handles = (0..8)
            .map(|_| {
                let limiter = Arc::clone(&limiter);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    allow_public_mail(
                        &mut limiter.lock().expect("limiter lock is available"),
                        "echo",
                        "mail",
                        IpAddr::V4(Ipv4Addr::LOCALHOST),
                        now,
                    )
                })
            })
            .collect::<Vec<_>>();
        let allowed = handles
            .into_iter()
            .map(|handle| handle.join().expect("limiter worker completes"))
            .filter(|allowed| *allowed)
            .count();

        assert_eq!(allowed, 1);
    }

    #[test]
    fn public_mail_limiter_fails_closed_at_capacity() {
        let mut limiter = PublicMailRateLimiter {
            sent: HashMap::new(),
            expirations: VecDeque::new(),
            window: Duration::from_secs(180),
            max_clients: 1,
        };
        let now = Instant::now();

        assert!(allow_public_mail(
            &mut limiter,
            "echo",
            "mail",
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            now,
        ));
        assert!(!allow_public_mail(
            &mut limiter,
            "echo",
            "mail",
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
            now,
        ));
        assert!(allow_public_mail(
            &mut limiter,
            "echo",
            "mail",
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
            now + Duration::from_secs(180),
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
            mail_permissions: Default::default(),
        };
        let response = plugin
            .call(
                bindings::nur::cms::types::Request {
                    route_id: "root".into(),
                    method: "GET".into(),
                    path: "/plugin-echo".into(),
                    path_params: Vec::new(),
                    query: None,
                    headers: Vec::new(),
                    body: Vec::new(),
                    identity: None,
                },
                true,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
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
            mail_permissions: Default::default(),
        };
        let result = plugin
            .call(
                bindings::nur::cms::types::Request {
                    route_id: "missing".into(),
                    method: "GET".into(),
                    path: "/missing".into(),
                    path_params: Vec::new(),
                    query: None,
                    headers: Vec::new(),
                    body: Vec::new(),
                    identity: None,
                },
                true,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
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
