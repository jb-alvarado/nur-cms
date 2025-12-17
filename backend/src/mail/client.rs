use html_parser::Dom;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header,
    transport::smtp::authentication::Credentials,
};
use serde::{Deserialize, Serialize};
use tokio::time::Duration;
use tracing::error;
use voca_rs::Voca;

use crate::{CONFIG, db::models::MailTarget, utils::errors::ServiceError};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Msg {
    pub mail: String,
    pub name: String,
    pub subject: Option<String>,
    pub text: String,
    pub target: MailTarget,
}

impl Msg {
    pub fn new(
        mail: String,
        name: String,
        subject: Option<String>,
        text: String,
        target: MailTarget,
    ) -> Self {
        Self {
            mail,
            name,
            subject,
            text,
            target,
        }
    }

    pub fn content_type(&self) -> header::ContentType {
        match Dom::parse(&self.text) {
            Ok(dom) => {
                if (dom.children.len() == 1 && dom.children[0].text().is_some())
                    || !self.target.allow_html
                {
                    return header::ContentType::TEXT_PLAIN;
                }

                header::ContentType::TEXT_HTML
            }
            Err(_) => header::ContentType::TEXT_PLAIN,
        }
    }
}

pub async fn send(message: Message) -> Result<(), ServiceError> {
    let config = CONFIG.read().await.clone();
    let credentials = Credentials::new(
        config.mail_user.clone().unwrap_or_default(),
        config.mail_password.clone().unwrap_or_default(),
    );

    // create transporter based on starttls configuration
    let transporter = if config.mail_starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.mail_smtp.unwrap_or_default())
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&config.mail_smtp.unwrap_or_default())
    };

    let mailer = transporter?
        .credentials(credentials)
        .timeout(Some(Duration::new(4, 0)))
        .build();

    mailer.send(message.clone()).await?;

    Ok(())
}

/// Take Msg object and send it to the mail server
pub async fn message(msg: Msg) -> Result<(), ServiceError> {
    let config = CONFIG.read().await.clone();
    let subject = msg
        .subject
        .clone()
        .unwrap_or(msg.target.subject.clone().unwrap_or_default());
    let mut message = Message::builder()
        .subject(&subject)
        .from(config.mail_user.unwrap_or_default().parse()?)
        .reply_to(format!("{} <{}>", msg.name, msg.mail).parse()?);

    for recipient in &msg.target.recipients {
        let addr = recipient.trim();
        if addr.is_empty() {
            continue;
        }

        match addr.parse() {
            Ok(parsed) => message = message.to(parsed),
            Err(e) => error!("Invalid recipient address '{addr}': {e}"),
        }
    }

    let message_text = match msg.target.allow_html {
        true => msg.text.clone(),
        false => msg.text._strip_tags(),
    };

    let mail = message.header(msg.content_type()).body(message_text)?;

    send(mail).await?;

    Ok(())
}
