use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use colored::Colorize;
use tracing::{error, info};

use nur_core::mail::service::{
    MailRequest, PluginMailContentKind, PluginMailError, deliver_plugin_mail, prepare_plugin_mail,
};

use super::{HostState, PluginResult, TimedHostResult};
use crate::runtime::{
    bindings::{self, nur::cms::types::PluginError},
    mail_limiter::{allow_mail_for_request, allow_public_mail_for_client},
};

impl bindings::nur::cms::mail::Host for HostState {
    fn send(&mut self, message: bindings::nur::cms::mail::Message) -> PluginResult<()> {
        self.consume_host_call()?;

        let trusted_template = matches!(
            message.content_kind,
            bindings::nur::cms::mail::ContentKind::TrustedTemplateHtml
        );

        if !self.mail_permissions.allows(
            &message.target,
            message.recipient.is_some(),
            trusted_template,
        ) {
            return Err(PluginError::Forbidden);
        }

        let request = MailRequest {
            reply_to: message.reply_to,
            subject: message.subject,
            name: message.name,
            text: message.text,
        };
        let content_kind = match message.content_kind {
            bindings::nur::cms::mail::ContentKind::UserInput => PluginMailContentKind::UserInput,
            bindings::nur::cms::mail::ContentKind::TrustedTemplateHtml => {
                PluginMailContentKind::TrustedTemplateHtml
            }
        };
        let target = message.target;
        let recipient = message.recipient;

        let started = Instant::now();
        let prepared = self.tokio_handle.block_on(async {
            tokio::time::timeout(
                self.host_call_timeout,
                prepare_plugin_mail(&self.pool, &target, recipient, content_kind, request),
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
                return Err(PluginError::Failed("mail delivery timed out".into()));
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
            Err(_) => Err(PluginError::Failed("mail delivery timed out".into())),
        }
    }
}

impl HostState {
    fn plugin_mail_error(&self, error: PluginMailError) -> PluginError {
        match error {
            PluginMailError::UnknownTarget => PluginError::BadRequest("unknown mail target".into()),
            PluginMailError::DynamicRecipientNotAllowed => {
                PluginError::BadRequest("dynamic recipient is not allowed".into())
            }
            PluginMailError::InvalidMessage => {
                PluginError::BadRequest("invalid mail message".into())
            }
            PluginMailError::Spam => PluginError::BadRequest("mail message rejected".into()),
            PluginMailError::DeliveryFailed => {
                error!(plugin = %self.plugin_id, ?error, "plugin mail delivery failed");
                PluginError::Failed("mail delivery failed".into())
            }
        }
    }

    fn authorize_mail_send(&mut self) -> PluginResult<()> {
        let limiter = Arc::clone(&self.public_mail_rate_limiter);
        let plugin_id = self.plugin_id.clone();
        let route_id = self.route_id.clone();
        let client_ip = self.client_ip;

        if allow_mail_for_request(
            &mut self.mail_calls_remaining,
            self.public_route,
            &mut self.public_mail_authorized,
            || {
                allow_public_mail_for_client(
                    &limiter,
                    &plugin_id,
                    &route_id,
                    client_ip,
                    Instant::now(),
                )
            },
        ) {
            Ok(())
        } else {
            Err(PluginError::RateLimited)
        }
    }
}

impl HostState {
    fn log_mail_metrics<T>(&self, result: &TimedHostResult<T, PluginMailError>, elapsed: Duration) {
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
}
