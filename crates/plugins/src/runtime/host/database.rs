use std::time::{Duration, Instant};

use colored::Colorize;
use tracing::{error, info};

use super::{HostState, PluginResult, TimedHostResult};
use crate::{
    db::plugin_database::{
        DatabaseHostError, execute_statements, validate_statement, validate_transaction_size,
    },
    runtime::bindings::{self, nur::cms::types::PluginError},
};

impl bindings::nur::cms::database::Host for HostState {
    fn execute(
        &mut self,
        statement: bindings::nur::cms::database::Statement,
    ) -> PluginResult<bindings::nur::cms::database::QueryResult> {
        self.consume_host_call()?;
        let validated = validate_statement(&statement).map_err(database_error)?;

        let schema = self.plugin_schema.clone();
        let started = Instant::now();
        let cache_key = if validated.cacheable() {
            let (generation, digest, cached) = self.database_cache.lookup(&schema, &statement);
            if let Some(cached) = cached {
                if self.metrics_enabled {
                    info!(
                        plugin = %self.plugin_id,
                        host_call = "database",
                        operation = "execute",
                        outcome = "cache_hit",
                        duration_ms = %format!("{:.2}", started.elapsed().as_secs_f64() * 1_000.0).yellow(),
                        "plugin host-call metrics"
                    );
                }
                return Ok(cached);
            }
            Some((generation, digest))
        } else {
            None
        };

        let limit = self.content_response_body_limit;
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
            Ok(Ok(mut results)) => {
                let result = results
                    .pop()
                    .ok_or_else(|| database_error("database query returned no result"))?;
                if let Some((generation, digest)) = cache_key {
                    self.database_cache
                        .insert(&schema, generation, digest, result.clone());
                } else {
                    self.database_cache.invalidate(&schema);
                }
                Ok(result)
            }
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
    ) -> PluginResult<Vec<bindings::nur::cms::database::QueryResult>> {
        self.consume_host_call()?;
        validate_transaction_size(&statements).map_err(database_error)?;
        let validated = statements
            .iter()
            .map(validate_statement)
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)?;
        let writes = validated.iter().any(|statement| !statement.read_only());

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
            Ok(Ok(result)) => {
                if writes {
                    self.database_cache.invalidate(&schema);
                }
                Ok(result)
            }
            Ok(Err(error)) => {
                error!(plugin = %self.plugin_id, %error, "plugin database transaction failed");
                Err(database_error("database transaction failed"))
            }
            Err(_) => Err(database_error("database transaction timed out")),
        }
    }
}

impl HostState {
    fn log_database_metrics<T>(
        &self,
        operation: &'static str,
        result: &TimedHostResult<T, DatabaseHostError>,
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
}

fn database_error(message: impl Into<String>) -> PluginError {
    PluginError::Failed(message.into())
}
