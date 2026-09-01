use sqlx::PgPool;

use crate::{
    db::handles,
    mail::client::{Msg, message},
    utils::{
        errors::NurError,
        spam_detection::{TextScore, evaluate_text, validate_email_address},
    },
};

const MAX_MAIL_NAME_CHARS: usize = 160;
const MAX_MAIL_SUBJECT_CHARS: usize = 255;
const MAX_MAIL_TEXT_CHARS: usize = 20_000;
const MAX_MAIL_TARGET_CHARS: usize = 160;

#[derive(Clone, Debug)]
pub struct MailRequest {
    pub reply_to: String,
    pub name: String,
    pub subject: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PreparedMail {
    pub reply_to: String,
    pub name: String,
    pub subject: Option<String>,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginMailError {
    UnknownTarget,
    DynamicRecipientNotAllowed,
    InvalidMessage,
    Spam,
    DeliveryFailed,
}

pub struct PreparedPluginMail {
    target: crate::db::models::MailTarget,
    mail: PreparedMail,
}

pub async fn prepare_mail(request: MailRequest) -> Result<PreparedMail, NurError> {
    let evaluation = evaluate_text(&request.text, None);
    prepare_mail_with_evaluation(request, evaluation).await
}

pub async fn prepare_mail_with_evaluation(
    request: MailRequest,
    evaluation: TextScore,
) -> Result<PreparedMail, NurError> {
    if request.name.trim().is_empty()
        || request.name.chars().count() > MAX_MAIL_NAME_CHARS
        || contains_header_control(&request.name)
        || request.subject.as_ref().is_some_and(|value| {
            value.chars().count() > MAX_MAIL_SUBJECT_CHARS || contains_header_control(value)
        })
        || request.text.chars().count() > MAX_MAIL_TEXT_CHARS
    {
        return Err(NurError::BadRequest("Invalid mail request.".into()));
    }
    let reply_to = validate_email_address(request.reply_to).await?;
    if !evaluation.passed {
        return Err(NurError::Conflict(
            "This message is not allowed!".to_string(),
        ));
    }

    Ok(PreparedMail {
        reply_to,
        name: request.name,
        subject: request.subject,
        text: request.text,
    })
}

fn contains_header_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

pub async fn deliver_mail(
    pool: &PgPool,
    target_name: &str,
    prepared: PreparedMail,
) -> Result<(), NurError> {
    validate_mail_target(target_name)?;
    let target = handles::select_mail_target(pool, target_name).await?;
    message(Msg::new(
        prepared.reply_to,
        prepared.name,
        prepared.subject,
        prepared.text,
        target,
    ))
    .await
}

pub fn validate_mail_target(target_name: &str) -> Result<(), NurError> {
    if target_name.trim().is_empty() || target_name.chars().count() > MAX_MAIL_TARGET_CHARS {
        return Err(NurError::BadRequest("Invalid mail target.".into()));
    }
    Ok(())
}

pub async fn send_mail(
    pool: &PgPool,
    target_name: &str,
    request: MailRequest,
) -> Result<(), NurError> {
    let prepared = prepare_mail(request).await?;
    deliver_mail(pool, target_name, prepared).await
}

/// Sends a plugin message using CMS validation and delivery without exposing
/// target details or transport errors across the Wasm boundary.
pub async fn prepare_plugin_mail(
    pool: &PgPool,
    target_name: &str,
    recipient: Option<String>,
    request: MailRequest,
) -> Result<PreparedPluginMail, PluginMailError> {
    validate_mail_target(target_name).map_err(|_| PluginMailError::InvalidMessage)?;
    let mut target = handles::select_mail_target(pool, target_name)
        .await
        .map_err(|error| match error {
            NurError::NotFound => PluginMailError::UnknownTarget,
            _ => PluginMailError::DeliveryFailed,
        })?;
    let prepared = prepare_mail(request).await.map_err(|error| match error {
        NurError::BadRequest(_) => PluginMailError::InvalidMessage,
        NurError::Conflict(_) => PluginMailError::Spam,
        _ => PluginMailError::DeliveryFailed,
    })?;

    apply_dynamic_recipient(&mut target, recipient).await?;

    Ok(PreparedPluginMail {
        target,
        mail: prepared,
    })
}

pub async fn deliver_plugin_mail(prepared: PreparedPluginMail) -> Result<(), PluginMailError> {
    message(Msg::new(
        prepared.mail.reply_to,
        prepared.mail.name,
        prepared.mail.subject,
        prepared.mail.text,
        prepared.target,
    ))
    .await
    .map_err(|_| PluginMailError::DeliveryFailed)
}

async fn apply_dynamic_recipient(
    target: &mut crate::db::models::MailTarget,
    recipient: Option<String>,
) -> Result<(), PluginMailError> {
    let Some(recipient) = recipient else {
        return Ok(());
    };
    if !target.allow_dynamic_recipient {
        return Err(PluginMailError::DynamicRecipientNotAllowed);
    }
    let recipient = validate_email_address(recipient)
        .await
        .map_err(|_| PluginMailError::InvalidMessage)?;
    target.recipients = vec![recipient];
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;

    use sqlx::PgPool;

    use super::{MailRequest, PluginMailError, apply_dynamic_recipient, prepare_mail, send_mail};
    use crate::{db::models::MailTarget, utils::errors::NurError};

    #[tokio::test]
    async fn rejects_invalid_reply_to() {
        let result = prepare_mail(MailRequest {
            reply_to: "not an email".into(),
            name: "Example".into(),
            subject: None,
            text: "This is a sufficiently long, normal message with several words and punctuation."
                .into(),
        })
        .await;
        assert!(matches!(result, Err(NurError::BadRequest(_))));
    }

    #[tokio::test]
    async fn rejects_spam() {
        let result = prepare_mail(MailRequest {
            reply_to: "sender@example.org".into(),
            name: "Example".into(),
            subject: None,
            text: "asdf".into(),
        })
        .await;
        assert!(matches!(result, Err(NurError::Conflict(_))));
    }

    #[tokio::test]
    async fn applies_character_limits_consistently_to_unicode() {
        let result = prepare_mail(MailRequest {
            reply_to: "sender@example.org".into(),
            name: "ä".repeat(160),
            subject: None,
            text: "This is a sufficiently long, normal message with several words and punctuation."
                .into(),
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn rejects_control_characters_in_mail_headers() {
        for (name, subject) in [
            ("Example\r\nBcc: victim@example.org", None),
            ("Example", Some("Subject\nBcc: victim@example.org")),
        ] {
            let result = prepare_mail(MailRequest {
                reply_to: "sender@example.org".into(),
                name: name.into(),
                subject: subject.map(str::to_owned),
                text: "This is a sufficiently long, normal message with several words and punctuation."
                    .into(),
            })
            .await;
            assert!(matches!(result, Err(NurError::BadRequest(_))));
        }
    }

    #[tokio::test]
    async fn fixed_target_recipients_remain_unchanged() {
        let mut target = MailTarget {
            recipients: vec!["merchant@example.org".into()],
            ..Default::default()
        };

        apply_dynamic_recipient(&mut target, None)
            .await
            .expect("fixed recipients are valid");

        assert_eq!(target.recipients, ["merchant@example.org"]);
    }

    #[tokio::test]
    async fn dynamic_recipient_requires_explicit_target_permission() {
        let mut target = MailTarget {
            recipients: vec!["merchant@example.org".into()],
            ..Default::default()
        };

        let result =
            apply_dynamic_recipient(&mut target, Some("customer@example.org".into())).await;

        assert_eq!(result, Err(PluginMailError::DynamicRecipientNotAllowed));
        assert_eq!(target.recipients, ["merchant@example.org"]);
    }

    #[tokio::test]
    async fn dynamic_recipient_exclusively_replaces_fixed_recipients() {
        let mut target = MailTarget {
            recipients: vec!["merchant@example.org".into(), "sales@example.org".into()],
            allow_dynamic_recipient: true,
            ..Default::default()
        };

        apply_dynamic_recipient(&mut target, Some(" Customer@Example.ORG ".into()))
            .await
            .expect("dynamic recipient is valid");

        assert_eq!(target.recipients, ["Customer@example.org"]);
    }

    #[tokio::test]
    async fn dynamic_recipient_rejects_lists_and_header_injection() {
        for recipient in [
            "first@example.org,second@example.org",
            "customer@example.org\r\nBcc: victim@example.org",
        ] {
            let mut target = MailTarget {
                recipients: vec!["merchant@example.org".into()],
                allow_dynamic_recipient: true,
                ..Default::default()
            };

            assert_eq!(
                apply_dynamic_recipient(&mut target, Some(recipient.into())).await,
                Err(PluginMailError::InvalidMessage)
            );
            assert_eq!(target.recipients, ["merchant@example.org"]);
        }
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL via DATABASE_URL"]
    async fn rejects_unknown_mail_target_without_sending() {
        let pool = PgPool::connect(&env::var("DATABASE_URL").expect("DATABASE_URL is configured"))
            .await
            .expect("database is reachable");
        let result = send_mail(
            &pool,
            "missing-plugin-mail-target",
            MailRequest {
                reply_to: "sender@example.org".into(),
                name: "Example".into(),
                subject: None,
                text: "This is a sufficiently long, normal message with several words and punctuation."
                    .into(),
            },
        )
        .await;
        assert!(matches!(result, Err(NurError::NotFound)));
    }
}
