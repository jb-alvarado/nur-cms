use std::{path::Path, str};

use nur_core::mail::client::MailAttachment;

use crate::runtime::{
    bindings::nur::cms::{
        mail_attachments::{Attachment, AttachmentType},
        types::PluginError,
    },
    host::PluginResult,
};

const MAX_ATTACHMENTS: usize = 5;
const MAX_ATTACHMENT_BYTES: usize = 1024 * 1024;

pub(super) fn validate(attachments: Vec<Attachment>) -> PluginResult<Vec<MailAttachment>> {
    let total = attachments
        .iter()
        .fold(0usize, |bytes, item| bytes.saturating_add(item.data.len()));

    if attachments.len() > MAX_ATTACHMENTS || total > MAX_ATTACHMENT_BYTES {
        return Err(PluginError::BadRequest(
            "mail attachment limit exceeded".into(),
        ));
    }

    attachments.into_iter().map(validate_attachment).collect()
}

fn validate_attachment(attachment: Attachment) -> PluginResult<MailAttachment> {
    let Attachment {
        filename,
        kind,
        data,
    } = attachment;
    let (extensions, mime): (&[&str], &str) = match kind {
        AttachmentType::Pdf => (&["pdf"], "application/pdf"),
        AttachmentType::Png => (&["png"], "image/png"),
        AttachmentType::Jpeg => (&["jpg", "jpeg"], "image/jpeg"),
        AttachmentType::PlainText => (&["txt"], "text/plain; charset=utf-8"),
        AttachmentType::Csv => (&["csv"], "text/csv; charset=utf-8"),
        AttachmentType::Json => (&["json"], "application/json"),
    };

    if !valid_filename(&filename, extensions) || !matches_type(kind, &data) {
        return Err(invalid_attachment());
    }

    MailAttachment::new(filename, mime, data).map_err(|_| invalid_attachment())
}

fn valid_filename(filename: &str, extensions: &[&str]) -> bool {
    filename.len() <= 160
        && !filename.starts_with('.')
        && filename == filename.trim()
        && filename.chars().all(|character| {
            character.is_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.' | '(' | ')')
        })
        && Path::new(filename)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extensions
                    .iter()
                    .any(|allowed| extension.eq_ignore_ascii_case(allowed))
            })
}

fn matches_type(kind: AttachmentType, data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }

    match kind {
        AttachmentType::Pdf => {
            data.starts_with(b"%PDF-") && data.trim_ascii_end().ends_with(b"%%EOF")
        }
        AttachmentType::Png => data.starts_with(b"\x89PNG\r\n\x1a\n"),
        AttachmentType::Jpeg => data.starts_with(b"\xff\xd8\xff") && data.ends_with(b"\xff\xd9"),
        AttachmentType::PlainText | AttachmentType::Csv => {
            !data.contains(&0) && str::from_utf8(data).is_ok()
        }
        AttachmentType::Json => serde_json::from_slice::<serde_json::Value>(data).is_ok(),
    }
}

fn invalid_attachment() -> PluginError {
    PluginError::BadRequest("invalid mail attachment filename or content".into())
}

#[cfg(test)]
mod tests {
    use super::{Attachment, AttachmentType, MAX_ATTACHMENT_BYTES, validate};

    fn attachment(filename: &str, kind: AttachmentType, data: &[u8]) -> Attachment {
        Attachment {
            filename: filename.into(),
            kind,
            data: data.into(),
        }
    }

    #[test]
    fn validates_multiple_kinds_with_matching_filenames() {
        let attachments = vec![
            attachment("Müller.pdf", AttachmentType::Pdf, b"%PDF-1.4\n%%EOF\n"),
            attachment("table.CSV", AttachmentType::Csv, b"key,value\na,1\n"),
            attachment("data.json", AttachmentType::Json, br#"{"hello":"world"}"#),
            attachment("icon.png", AttachmentType::Png, b"\x89PNG\r\n\x1a\n"),
            attachment("photo.jpg", AttachmentType::Jpeg, b"\xff\xd8\xff\xff\xd9"),
        ];

        assert!(validate(attachments).is_ok());
    }

    #[test]
    fn rejects_unsafe_names_mismatched_content_and_excess_limits() {
        let text = b"test";
        let oversized = vec![b'a'; MAX_ATTACHMENT_BYTES / 2 + 1];

        for invalid in [
            attachment("../data.txt", AttachmentType::PlainText, text),
            attachment("line\r\nbreak.txt", AttachmentType::PlainText, text),
            attachment("data.exe", AttachmentType::PlainText, text),
            attachment("fake.pdf", AttachmentType::Pdf, text),
            attachment("bad.json", AttachmentType::Json, text),
            attachment("bad-utf8.json", AttachmentType::Json, b"{\"key\":\"\xff\"}"),
        ] {
            assert!(validate(vec![invalid]).is_err());
        }

        assert!(
            validate(vec![
                attachment("a.txt", AttachmentType::PlainText, &oversized),
                attachment("b.txt", AttachmentType::PlainText, &oversized),
            ])
            .is_err()
        );
        assert!(
            validate(
                (0..6)
                    .map(|_| attachment("a.txt", AttachmentType::PlainText, text))
                    .collect()
            )
            .is_err()
        );
    }
}
