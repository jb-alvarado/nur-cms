use std::{
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use sqlx::PgPool;
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use super::{
    bindings::{self, nur::cms::types::PluginError},
    component::PluginComponent,
    mail_limiter::{MailPermissions, PublicMailRateLimiter},
};
use crate::{
    db::query_cache::PluginDatabaseCache,
    manifest::schema_name,
    storage::{PluginStorage, StorageDirectory},
};

mod configuration;
mod content;
mod database;
mod mail;
mod storage;

type PluginResult<T> = Result<T, PluginError>;
type TimedHostResult<T, E> = Result<Result<T, E>, tokio::time::error::Elapsed>;

const MAX_MAIL_CALLS_PER_REQUEST: u8 = 3;

pub(super) struct HostState {
    plugin_id: String,
    plugin_schema: String,
    pub(super) limits: StoreLimits,
    table: ResourceTable,
    wasi: WasiCtx,
    pool: PgPool,
    database_cache: Arc<PluginDatabaseCache>,
    tokio_handle: tokio::runtime::Handle,
    pub(super) host_calls_remaining: usize,
    host_call_timeout: Duration,
    content_response_body_limit: usize,
    metrics_enabled: bool,
    public_route: bool,
    route_id: String,
    client_ip: Option<IpAddr>,
    public_mail_rate_limiter: Arc<Mutex<PublicMailRateLimiter>>,
    public_mail_authorized: Option<bool>,
    mail_calls_remaining: u8,
    mail_permissions: MailPermissions,
    storage: Option<PluginStorage>,
    storage_directories: Arc<Vec<StorageDirectory>>,
    storage_write_limit: usize,
    storage_quota: u64,
}

impl HostState {
    pub(super) fn new(
        component: &PluginComponent,
        request: &bindings::nur::cms::types::Request,
        public_route: bool,
        client_ip: Option<IpAddr>,
    ) -> Self {
        let runtime = &component.runtime;

        Self {
            plugin_id: component.id.clone(),
            plugin_schema: schema_name(&component.id),
            limits: StoreLimitsBuilder::new()
                .memory_size(runtime.memory_limit)
                .instances(128)
                .tables(16)
                .memories(4)
                .trap_on_grow_failure(true)
                .build(),
            table: ResourceTable::new(),
            wasi: WasiCtxBuilder::new().build(),
            pool: runtime.pool.clone(),
            database_cache: Arc::clone(&runtime.database_cache),
            tokio_handle: runtime.tokio_handle.clone(),
            host_calls_remaining: runtime.max_host_calls,
            host_call_timeout: runtime.timeout,
            content_response_body_limit: runtime.content_response_body_limit,
            metrics_enabled: runtime.metrics_enabled,
            public_route,
            route_id: request.route_id.clone(),
            client_ip,
            public_mail_rate_limiter: Arc::clone(&runtime.public_mail_rate_limiter),
            public_mail_authorized: None,
            mail_calls_remaining: MAX_MAIL_CALLS_PER_REQUEST,
            mail_permissions: component.mail_permissions.clone(),
            storage: runtime.storage.clone(),
            storage_directories: Arc::clone(&component.storage_directories),
            storage_write_limit: runtime.storage_write_limit,
            storage_quota: runtime.storage_quota,
        }
    }

    fn consume_host_call(&mut self) -> PluginResult<()> {
        if self.host_calls_remaining == 0 {
            return Err(PluginError::Failed(
                "plugin host-call limit exceeded".into(),
            ));
        }

        self.host_calls_remaining -= 1;

        Ok(())
    }
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
