//! SMTP send helper (lettre): freezes message fields (plain text, optional rich HTML,
//! and attachments) at enqueue time and performs delivery on the dedicated SMTP worker.

use std::time::{Duration, UNIX_EPOCH};

use base64::Engine as _;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};

use crate::auth::{CredentialKind, Endpoint, ProviderAuth};
use crate::error::CoreError;
use crate::ids::UnixMs;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FrozenAttachment {
    pub filename: String,
    pub mime: String,
    pub data_base64: String,
}

/// The frozen snapshot of a human-approved message. Serialized into `job.payload` at
/// enqueue so later draft edits (or crashes) can never change what gets sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FrozenMessage {
    pub from_name: Option<String>,
    pub from_address: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    #[serde(default)]
    pub body_html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<FrozenAttachment>,
    pub message_id: String,
    pub date_ms: i64,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SmtpOutcome {
    Accepted,
    Transient(String),
    Permanent(String),
}

/// The draft fields captured (frozen) at enqueue time.
#[derive(Debug, Clone)]
pub(crate) struct FreezeInput {
    pub from_name: Option<String>,
    pub from_address: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
    pub attachments: Vec<FrozenAttachment>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
}

pub(crate) fn freeze_message(input: FreezeInput, now: UnixMs) -> FrozenMessage {
    let message_id = format!("<{}@ferrule.local>", uuid::Uuid::now_v7());
    FrozenMessage {
        from_name: input.from_name,
        from_address: input.from_address,
        to: input.to,
        cc: input.cc,
        bcc: input.bcc,
        subject: input.subject,
        body_text: input.body_text,
        body_html: input.body_html,
        attachments: input.attachments,
        message_id,
        date_ms: now.0,
        in_reply_to: input.in_reply_to,
        references: input.references,
    }
}

fn mailbox(address: &str, name: Option<&str>) -> Result<Mailbox, CoreError> {
    let invalid = || CoreError::InvalidInput {
        field: "address".to_string(),
        reason: format!("invalid email address: {address}"),
    };
    match name {
        Some(name) if !name.is_empty() => {
            let email = address.parse().map_err(|_| invalid())?;
            Ok(Mailbox::new(Some(name.to_string()), email))
        }
        _ => address.parse().map_err(|_| invalid()),
    }
}

pub(crate) fn build_message(frozen: &FrozenMessage) -> Result<Message, CoreError> {
    let mut builder =
        Message::builder().from(mailbox(&frozen.from_address, frozen.from_name.as_deref())?);
    for address in &frozen.to {
        builder = builder.to(mailbox(address, None)?);
    }
    for address in &frozen.cc {
        builder = builder.cc(mailbox(address, None)?);
    }
    for address in &frozen.bcc {
        builder = builder.bcc(mailbox(address, None)?);
    }
    builder = builder.subject(frozen.subject.clone());
    builder = builder.message_id(Some(frozen.message_id.clone()));
    if let Some(in_reply_to) = &frozen.in_reply_to {
        builder = builder.in_reply_to(in_reply_to.clone());
    }
    if let Some(references) = &frozen.references {
        builder = builder.references(references.clone());
    }
    builder = builder.date(UNIX_EPOCH + Duration::from_millis(frozen.date_ms.max(0) as u64));

    let has_html = frozen
        .body_html
        .as_deref()
        .map(|h| !h.trim().is_empty())
        .unwrap_or(false);

    if !has_html && frozen.attachments.is_empty() {
        return builder
            .body(frozen.body_text.clone())
            .map_err(|e| CoreError::Internal {
                reason: format!("build message: {e}"),
            });
    }

    let body_part = if let Some(html) = frozen.body_html.as_deref().filter(|h| !h.trim().is_empty())
    {
        MultiPart::alternative_plain_html(frozen.body_text.clone(), html.to_string())
    } else {
        MultiPart::mixed().singlepart(SinglePart::plain(frozen.body_text.clone()))
    };

    if frozen.attachments.is_empty() {
        return builder
            .multipart(body_part)
            .map_err(|e| CoreError::Internal {
                reason: format!("build multipart message: {e}"),
            });
    }

    let mut mixed = MultiPart::mixed().multipart(body_part);
    for att in &frozen.attachments {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&att.data_base64)
            .map_err(|e| CoreError::InvalidInput {
                field: "attachments".to_string(),
                reason: format!("invalid base64 attachment '{}': {e}", att.filename),
            })?;
        let content_type = ContentType::parse(&att.mime)
            .unwrap_or_else(|_| ContentType::parse("application/octet-stream").expect("valid mime"));
        mixed = mixed.singlepart(Attachment::new(att.filename.clone()).body(bytes, content_type));
    }

    builder.multipart(mixed).map_err(|e| CoreError::Internal {
        reason: format!("build message with attachments: {e}"),
    })
}

fn transport_for(
    auth: &ProviderAuth,
    endpoint: &Endpoint,
    username: &str,
    secret: &str,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, CoreError> {
    let credentials = Credentials::new(username.to_string(), secret.to_string());
    let mechanism = match auth.credential_kind() {
        CredentialKind::Password | CredentialKind::AiApiKey | CredentialKind::WebhookSecret => {
            vec![Mechanism::Login, Mechanism::Plain]
        }
        CredentialKind::OAuthRefreshToken | CredentialKind::OAuthClientSecret => {
            vec![Mechanism::Xoauth2]
        }
    };

    let builder = if endpoint.tls && endpoint.port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&endpoint.host).map_err(|e| {
            CoreError::Network {
                reason: e.to_string(),
                retryable: true,
            }
        })?
    } else if endpoint.tls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&endpoint.host).map_err(|e| {
            CoreError::Network {
                reason: e.to_string(),
                retryable: true,
            }
        })?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&endpoint.host)
    };

    Ok(builder
        .port(endpoint.port)
        .credentials(credentials)
        .authentication(mechanism)
        .timeout(Some(Duration::from_secs(20)))
        .build::<Tokio1Executor>())
}

/// Deliver a frozen message. Never panics; classifies failures for the outbox state
/// machine (`Transient` → retry with backoff, `Permanent` → fail permanently).
pub(crate) async fn send_frozen(
    auth: &ProviderAuth,
    endpoint: &Endpoint,
    username: &str,
    secret: &str,
    frozen: &FrozenMessage,
) -> SmtpOutcome {
    let transport = match transport_for(auth, endpoint, username, secret) {
        Ok(transport) => transport,
        Err(e) => return SmtpOutcome::Transient(e.to_string()),
    };
    let message = match build_message(frozen) {
        Ok(message) => message,
        Err(e) => return SmtpOutcome::Permanent(e.to_string()),
    };
    match transport.send(message).await {
        Ok(_) => SmtpOutcome::Accepted,
        Err(e) if e.is_permanent() => SmtpOutcome::Permanent(e.to_string()),
        Err(e) => SmtpOutcome::Transient(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_multipart_message_with_html_and_attachment() {
        let frozen = freeze_message(
            FreezeInput {
                from_name: Some("Mira".into()),
                from_address: "mira@example.com".into(),
                to: vec!["dana@example.com".into()],
                cc: vec![],
                bcc: vec![],
                subject: "With attachment".into(),
                body_text: "Hello plain".into(),
                body_html: Some("<p>Hello <b>rich</b></p>".into()),
                attachments: vec![FrozenAttachment {
                    filename: "note.txt".into(),
                    mime: "text/plain".into(),
                    data_base64: base64::engine::general_purpose::STANDARD.encode(b"hello file"),
                }],
                in_reply_to: None,
                references: None,
            },
            UnixMs(1_760_000_000_000),
        );
        let msg = build_message(&frozen).expect("build multipart");
        let raw = String::from_utf8_lossy(&msg.formatted()).to_string();
        assert!(raw.contains("multipart/mixed"));
        assert!(raw.contains("multipart/alternative"));
        assert!(raw.contains("note.txt"));
        assert!(raw.contains("Hello <b>rich</b>"));
    }
}
