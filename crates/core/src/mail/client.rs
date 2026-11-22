use html_parser::Dom;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Attachment, Mailbox, MultiPart, SinglePart, header},
    transport::smtp::authentication::Credentials,
};
use serde::{Deserialize, Serialize};
use tokio::time::Duration;
use tracing::error;
use voca_rs::Voca;

use crate::{CONFIG, db::models::MailTarget, utils::errors::NurError};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Msg {
    pub mail: String,
    pub name: String,
    pub subject: Option<String>,
    pub text: String,
    pub target: MailTarget,
}

pub struct MailAttachment {
    filename: String,
    content_type: header::ContentType,
    data: Vec<u8>,
}

impl MailAttachment {
    pub fn new(filename: String, mime: &str, data: Vec<u8>) -> Result<Self, NurError> {
        let content_type = header::ContentType::parse(mime)
            .map_err(|_| NurError::BadRequest("invalid attachment content type".into()))?;

        Ok(Self {
            filename,
            content_type,
            data,
        })
    }
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

fn reply_to_mailbox(name: String, mail: &str) -> Result<Mailbox, NurError> {
    let mail: Mailbox = mail.parse()?;
    Ok(Mailbox::new(Some(name), mail.email))
}

pub async fn send(message: Message) -> Result<(), NurError> {
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
pub async fn message(msg: Msg) -> Result<(), NurError> {
    let sender = CONFIG.read().await.mail_user.clone().unwrap_or_default();
    let mail = build_message(msg, sender, Vec::new())?;

    send(mail).await
}

pub(crate) async fn message_with_attachments(
    msg: Msg,
    attachments: Vec<MailAttachment>,
) -> Result<(), NurError> {
    let sender = CONFIG.read().await.mail_user.clone().unwrap_or_default();
    let mail = build_message(msg, sender, attachments)?;

    send(mail).await
}

fn build_message(
    msg: Msg,
    sender: String,
    attachments: Vec<MailAttachment>,
) -> Result<Message, NurError> {
    let subject = msg
        .subject
        .clone()
        .unwrap_or(msg.target.subject.clone().unwrap_or_default());
    let mut message = Message::builder()
        .subject(&subject)
        .from(sender.parse()?)
        .reply_to(reply_to_mailbox(msg.name.clone(), &msg.mail)?);

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

    let mail = if attachments.is_empty() {
        message.header(msg.content_type()).body(message_text)?
    } else {
        let body = SinglePart::builder()
            .header(msg.content_type())
            .body(message_text);
        let mut parts = MultiPart::mixed().singlepart(body);

        for attachment in attachments {
            let part =
                Attachment::new(attachment.filename).body(attachment.data, attachment.content_type);

            parts = parts.singlepart(part);
        }

        message.multipart(parts)?
    };

    Ok(mail)
}

#[cfg(test)]
mod tests {
    use super::{MailAttachment, Msg, build_message, header, reply_to_mailbox};
    use crate::db::models::MailTarget;

    #[test]
    fn renders_multiple_mime_attachments_and_keeps_plain_mail() {
        let target = MailTarget {
            id: 1,
            name: "agreement".into(),
            subject: Some("Signed agreement".into()),
            recipients: vec!["office@example.org".into()],
            allow_html: false,
            allow_dynamic_recipient: false,
            total_count: None,
        };
        let msg = Msg::new(
            "sender@example.org".into(),
            "GRC".into(),
            None,
            "A signed agreement is attached.".into(),
            target,
        );
        let plain = build_message(msg.clone(), "sender@example.org".into(), Vec::new())
            .expect("valid plain mail");
        let mail = build_message(
            msg,
            "sender@example.org".into(),
            vec![
                MailAttachment {
                    filename: "agreement.pdf".into(),
                    content_type: header::ContentType::parse("application/pdf").expect("PDF MIME"),
                    data: b"%PDF-1.4\n%%EOF\n".to_vec(),
                },
                MailAttachment {
                    filename: "report.csv".into(),
                    content_type: header::ContentType::parse("text/csv").expect("CSV MIME"),
                    data: b"name,value\nexample,1\n".to_vec(),
                },
            ],
        )
        .expect("valid MIME mail");
        let mime = String::from_utf8(mail.formatted()).expect("UTF-8 mail headers");
        let plain = String::from_utf8(plain.formatted()).expect("UTF-8 mail headers");

        assert!(mime.contains("multipart/mixed"));
        assert!(mime.contains("application/pdf"));
        assert!(mime.contains("filename=\"agreement.pdf\""));
        assert!(mime.contains("text/csv"));
        assert!(mime.contains("filename=\"report.csv\""));
        assert!(!plain.contains("multipart/mixed"));
        assert!(plain.contains("A signed agreement is attached."));
        assert!(mime.contains("To: office@example.org"));
    }

    #[test]
    fn reply_to_accepts_a_frontend_name_with_mailbox_characters() {
        let reply_to = reply_to_mailbox("NUR <CMS>".into(), "noreply@example.org")
            .expect("valid reply-to mailbox");

        assert_eq!(reply_to.name.as_deref(), Some("NUR <CMS>"));
        assert_eq!(reply_to.email.to_string(), "noreply@example.org");
    }
}
