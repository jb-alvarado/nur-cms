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

const MAX_PUBLISHED_ENTRY_REFERENCES: usize = 50_000;

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

        self.log_content_query_metrics("published_entries", &result, host_call_started.elapsed());

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

    fn published_entry_references(
        &mut self,
        content_types: Vec<String>,
        locales: Vec<String>,
        group_id: Option<i64>,
    ) -> PluginResult<Vec<u8>> {
        self.consume_host_call()?;

        if content_types.is_empty()
            || content_types.len() > 16
            || content_types.iter().any(|value| !valid_slug(value))
            || locales.is_empty()
            || locales.len() > 32
            || locales.iter().any(|value| !valid_locale(value))
            || group_id.is_some_and(|value| value <= 0)
        {
            return Err(PluginError::BadRequest(
                "invalid published entry reference query".into(),
            ));
        }

        let host_call_started = Instant::now();
        let result = self.tokio_handle.block_on(async {
            tokio::time::timeout(self.host_call_timeout, async {
                let references = handles::select_published_entry_references(
                    &self.pool,
                    &content_types,
                    &locales,
                    group_id,
                    (MAX_PUBLISHED_ENTRY_REFERENCES + 1) as i64,
                )
                .await?;

                if references.len() > MAX_PUBLISHED_ENTRY_REFERENCES {
                    return Err(nur_core::utils::errors::NurError::UnprocessableEntity(
                        "published entry reference result exceeds 50000 rows".into(),
                    ));
                }

                serde_json::to_vec(&references).map_err(nur_core::utils::errors::NurError::from)
            })
            .await
        });

        self.log_content_query_metrics(
            "published_entry_references",
            &result,
            host_call_started.elapsed(),
        );

        match result {
            Ok(Ok(references)) if references.len() <= self.content_response_body_limit => {
                Ok(references)
            }
            Ok(Ok(_)) => Err(PluginError::Failed(
                "content response exceeds plugin limit".into(),
            )),
            Ok(Err(error)) => {
                error!(plugin = %self.plugin_id, %error, "plugin content reference query failed");
                Err(PluginError::Failed("content query failed".into()))
            }
            Err(_) => Err(PluginError::Failed("content query timed out".into())),
        }
    }

    fn published_entry_facets(&mut self, query: String) -> PluginResult<Vec<u8>> {
        self.consume_host_call()?;

        if query.len() > 8 * 1024 {
            return Err(PluginError::BadRequest("content query is too long".into()));
        }

        let params: handles::ContentEntryFacetQuery = match serde_urlencoded::from_str(&query) {
            Ok(params) => params,
            Err(_) => {
                return Err(PluginError::BadRequest("invalid content query".into()));
            }
        };

        let host_call_started = Instant::now();
        let result = self.tokio_handle.block_on(async {
            tokio::time::timeout(self.host_call_timeout, async {
                let facets = handles::select_content_entry_facets(&self.pool, &params).await?;
                serde_json::to_vec(&facets).map_err(nur_core::utils::errors::NurError::from)
            })
            .await
        });

        self.log_content_query_metrics(
            "published_entry_facets",
            &result,
            host_call_started.elapsed(),
        );

        match result {
            Ok(Ok(facets)) if facets.len() <= self.content_response_body_limit => Ok(facets),
            Ok(Ok(_)) => Err(PluginError::Failed(
                "content response exceeds plugin limit".into(),
            )),
            Ok(Err(error)) => {
                error!(plugin = %self.plugin_id, %error, "plugin content facet query failed");
                Err(PluginError::Failed("content query failed".into()))
            }
            Err(_) => Err(PluginError::Failed("content query timed out".into())),
        }
    }
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_locale(value: &str) -> bool {
    if !(2..=35).contains(&value.len()) {
        return false;
    }
    let mut parts = value.split('-');
    let Some(language) = parts.next() else {
        return false;
    };
    (2..=8).contains(&language.len())
        && language.bytes().all(|byte| byte.is_ascii_alphabetic())
        && parts.all(|part| {
            (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

impl HostState {
    fn log_content_query_metrics(
        &self,
        operation: &str,
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
            host_call = operation,
            outcome,
            duration_ms = %format!("{:.2}", elapsed.as_secs_f64() * 1_000.0).yellow(),
            response_bytes = %response_bytes.to_string().yellow(),
            "plugin host-call metrics"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{valid_locale, valid_slug};

    #[test]
    fn validates_reference_filters() {
        assert!(valid_slug("article"));
        assert!(!valid_slug("Article"));
        assert!(valid_locale("de-DE"));
        assert!(!valid_locale("de_DE"));
    }
}
