use std::time::{Duration, Instant};

use colored::Colorize;
use tracing::{error, info};

use nur_core::{
    CONFIG,
    db::{
        fields::{ContentEntryFields, ContentNodeFields, OutputType},
        handles,
        queries::QueryObj,
    },
    utils::content_output::render_entry_nodes,
};

use super::{HostState, PluginResult, TimedHostResult};
use crate::runtime::bindings::{self, nur::cms::types::PluginError};

impl bindings::nur::cms::content::Host for HostState {
    fn published_entries(
        &mut self,
        query: String,
        output: bindings::nur::cms::content::OutputType,
    ) -> PluginResult<Vec<u8>> {
        self.consume_host_call()?;

        if query.len() > 8 * 1024 {
            return Err(PluginError::BadRequest("content query is too long".into()));
        }

        let mut params: QueryObj<ContentEntryFields> = match serde_urlencoded::from_str(&query) {
            Ok(params) => params,
            Err(_) => {
                return Err(PluginError::BadRequest("invalid content query".into()));
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

        let embeds_requested = params
            .fields
            .contains(&ContentEntryFields::Node(ContentNodeFields::Embeds));

        if params
            .fields
            .contains(&ContentEntryFields::Node(ContentNodeFields::Text))
            && !embeds_requested
            && matches!(output, OutputType::AST | OutputType::HTML)
        {
            params
                .fields
                .push(ContentEntryFields::Node(ContentNodeFields::Embeds));
        }

        let host_call_started = Instant::now();
        let result = self.tokio_handle.block_on(async {
            tokio::time::timeout(self.host_call_timeout, async {
                let max_image_variant_width = CONFIG.read().await.max_image_resolution();
                let mut entries = handles::select_content_entries(&self.pool, &params).await?;

                if params
                    .fields
                    .contains(&ContentEntryFields::Node(ContentNodeFields::Text))
                {
                    render_entry_nodes(
                        &mut entries.results,
                        &output,
                        params.character_limit,
                        embeds_requested,
                        max_image_variant_width,
                    )?;
                }

                serde_json::to_vec(&entries).map_err(nur_core::utils::errors::NurError::from)
            })
            .await
        });

        self.log_content_query_metrics(&result, host_call_started.elapsed());

        match result {
            Ok(Ok(entries)) if entries.len() <= self.content_response_body_limit => Ok(entries),
            Ok(Ok(_)) => Err(PluginError::Failed(
                "content response exceeds plugin limit".into(),
            )),
            Ok(Err(error)) => {
                error!(plugin = %self.plugin_id, %error, "plugin content query failed");
                Err(PluginError::Failed("content query failed".into()))
            }
            Err(_) => Err(PluginError::Failed("content query timed out".into())),
        }
    }
}

impl HostState {
    fn log_content_query_metrics(
        &self,
        result: &TimedHostResult<Vec<u8>, nur_core::utils::errors::NurError>,
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
