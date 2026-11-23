//! Private transport layer: IMAP fetch/verify, SMTP send, and RFC822 parsing.

pub(crate) mod imap;
pub(crate) mod smtp;

use regex::Regex;

use crate::auth::{ProviderAuth, ProviderKind};
use crate::ids::UnixMs;
use crate::types::{InboxCategory, UnsubscribeInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    Gmail,
    Outlook,
    Generic,
}

impl Dialect {
    pub(crate) fn from_auth(auth: &ProviderAuth) -> Self {
        match auth.provider_kind() {
            ProviderKind::Gmail => Self::Gmail,
            ProviderKind::Outlook => Self::Outlook,
            ProviderKind::Imap => Self::Generic,
        }
    }

    pub(crate) fn primary_sync_folder(self) -> &'static str {
        match self {
            Self::Gmail => "[Gmail]/All Mail",
            Self::Outlook | Self::Generic => "INBOX",
        }
    }
}

/// One parsed remote message, ready for [`crate::store::ingest_messages`].
#[derive(Debug, Clone)]
pub(crate) struct IngestMessage {
    pub message_id_hdr: String,
    pub in_reply_to: Option<String>,
    pub thread_key: String,
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub date: Option<UnixMs>,
    pub snippet: String,
    pub is_read: bool,
    pub body_text: String,
    pub raw_html: Option<String>,
    pub unsub: Option<UnsubscribeInfo>,
    pub category: InboxCategory,
}

/// Read one header (unfolded, last occurrence wins) out of raw RFC822 bytes.
pub(crate) fn raw_header(raw: &[u8], name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let mut matched: Option<String> = None;
    let mut current_is_match = false;
    let mut current = String::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.starts_with(' ') || line.starts_with('\t') {
            if current_is_match {
                current.push(' ');
                current.push_str(line.trim());
            }
            continue;
        }
        if current_is_match {
            matched = Some(std::mem::take(&mut current));
            current_is_match = false;
        }
        if let Some((header, value)) = line.split_once(':') {
            if header.trim().eq_ignore_ascii_case(name) {
                current_is_match = true;
                current = value.trim().to_string();
            }
        }
    }
    if current_is_match {
        matched = Some(current);
    }
    matched.filter(|v| !v.is_empty())
}

fn header_ids(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|token| token.trim_matches(|c| c == '<' || c == '>').to_string())
        .filter(|token| !token.is_empty())
        .collect()
}

fn first_id(value: &str) -> Option<String> {
    header_ids(value).into_iter().next()
}

/// Parse raw RFC822 bytes into an ingest row. Returns `None` only when the bytes are
/// not a parseable message at all.
pub(crate) fn parse_raw(
    raw: &[u8],
    gm_thrid: Option<String>,
    is_read: bool,
    fallback_date: UnixMs,
) -> Option<IngestMessage> {
    let parsed = mail_parser::MessageParser::default().parse(raw)?;

    let message_id_hdr = parsed
        .message_id()
        .map(str::to_string)
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| {
            let uuid = uuid::Uuid::new_v5(&PARSE_NAMESPACE, raw);
            format!("<{uuid}@galapaghost.local>")
        });

    let in_reply_to = raw_header(raw, "In-Reply-To").and_then(|v| first_id(&v));
    let references = raw_header(raw, "References");
    let thread_key = gm_thrid
        // References is ordered oldest → newest; the leftmost id is the thread root.
        .or_else(|| references.as_deref().and_then(first_id))
        .or_else(|| in_reply_to.clone())
        .unwrap_or_else(|| message_id_hdr.clone());

    let (from_name, from_email) = parsed
        .from()
        .and_then(|addr| addr.first())
        .map(|addr| {
            (
                addr.name.as_ref().map(|n| n.to_string()),
                addr.address.clone().unwrap_or_default().to_string(),
            )
        })
        .unwrap_or((None, String::new()));

    let to = parsed
        .to()
        .map(|addr| {
            addr.iter()
                .filter_map(|a| a.address.as_ref().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let cc = parsed
        .cc()
        .map(|addr| {
            addr.iter()
                .filter_map(|a| a.address.as_ref().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let subject = parsed.subject().unwrap_or("").to_string();
    let date = parsed
        .date()
        .map(|d| UnixMs(d.to_timestamp() * 1000))
        .or(Some(fallback_date));

    let html = if memchr_contains(raw, b"text/html") {
        parsed.body_html(0).map(|c| c.into_owned())
    } else {
        None
    };
    let body_text = match parsed.body_text(0) {
        Some(text) => text.into_owned(),
        None => html.as_deref().map(html_to_text).unwrap_or_default(),
    };
    let snippet = snippet_of(&body_text);

    let list_unsub = raw_header(raw, "List-Unsubscribe");
    let unsub = list_unsub
        .as_deref()
        .and_then(parse_unsubscribe)
        .map(|target| {
            let one_click = raw_header(raw, "List-Unsubscribe-Post")
                .map(|v| v.contains("One-Click"))
                .unwrap_or(false);
            UnsubscribeInfo { one_click, target }
        });

    let list_mail =
        raw_header(raw, "List-ID").is_some() || raw_header(raw, "List-Subscribe").is_some();
    let category = if unsub.is_some() || list_mail {
        InboxCategory::Newsletters
    } else if receipt_re().is_match(&subject) {
        InboxCategory::Receipts
    } else {
        InboxCategory::Primary
    };

    Some(IngestMessage {
        message_id_hdr,
        in_reply_to,
        thread_key,
        from_name,
        from_email,
        to,
        cc,
        subject,
        date,
        snippet,
        is_read,
        body_text,
        raw_html: html,
        unsub,
        category,
    })
}

const PARSE_NAMESPACE: uuid::Uuid = uuid::Uuid::from_bytes([
    0x67, 0x6c, 0x61, 0x70, 0x61, 0x67, 0x68, 0x6f, 0x73, 0x74, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

fn receipt_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(receipt|invoice|payment|billing|statement|refund|renewal|shipment|shipping|order (?:confirmation|placed|shipped)|your (?:order|subscription|payment))\b",
        )
        .expect("static receipt regex")
    })
}

fn memchr_contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Extract the first unsubscribe target, preferring https over mailto.
fn parse_unsubscribe(value: &str) -> Option<String> {
    let mut mailto = None;
    let mut https = None;
    for candidate in value.split('<').skip(1) {
        let inner = candidate.split('>').next().unwrap_or("");
        let lower = inner.to_ascii_lowercase();
        if lower.starts_with("https://") || lower.starts_with("http://") {
            https = Some(inner.to_string());
        } else if lower.starts_with("mailto:") {
            mailto = Some(inner.to_string());
        }
    }
    https.or(mailto)
}

/// Crude but safe HTML → text for messages that ship no text/plain part.
pub(crate) fn html_to_text(html: &str) -> String {
    static TAGS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let tags = TAGS.get_or_init(|| Regex::new(r"(?s)<[^>]*>").expect("static tag regex"));
    let stripped = tags.replace_all(html, " ");
    let decoded = stripped
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn snippet_of(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = collapsed.chars();
    let head: String = chars.by_ref().take(140).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_header_unfolds_and_picks_last_occurrence() {
        let raw =
            b"From: a@b.c\r\nSubject: Hello\r\n\tworld\r\nReferences: <root@x> <mid@x>\r\n\r\nbody";
        assert_eq!(raw_header(raw, "Subject").as_deref(), Some("Hello world"));
        assert_eq!(
            raw_header(raw, "References").as_deref(),
            Some("<root@x> <mid@x>")
        );
        assert_eq!(raw_header(raw, "Missing"), None);
        assert_eq!(raw_header(raw, "From").as_deref(), Some("a@b.c"));
    }

    #[test]
    fn thread_key_prefers_gmail_then_references_then_message_id() {
        let raw = b"Message-ID: <m1@x>\r\nReferences: <root@x> <m1@x>\r\n\r\nbody";
        let parsed = parse_raw(raw, Some("gmail-thread".into()), false, UnixMs(0)).unwrap();
        assert_eq!(parsed.thread_key, "gmail-thread");

        let parsed = parse_raw(raw, None, false, UnixMs(0)).unwrap();
        assert_eq!(parsed.thread_key, "root@x");

        let bare = b"Message-ID: <solo@x>\r\n\r\nbody";
        let parsed = parse_raw(bare, None, false, UnixMs(0)).unwrap();
        assert_eq!(parsed.thread_key, "solo@x");
    }

    #[test]
    fn parse_raw_extracts_unsubscribe_and_category() {
        let raw = b"Message-ID: <n1@dispatch.design>\r\n\
From: Design Dispatch <hello@dispatch.design>\r\n\
Subject: Design Dispatch #142\r\n\
List-Unsubscribe: <https://dispatch.design/unsubscribe?u=1>, <mailto:leave@dispatch.design>\r\n\
List-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n\
Date: Mon, 3 Nov 2025 10:00:00 +0000\r\n\
Content-Type: text/html\r\n\r\n\
<p>Type that breathes &amp; shines.</p>";
        let parsed = parse_raw(raw, None, false, UnixMs(0)).unwrap();
        let unsub = parsed.unsub.unwrap();
        assert!(unsub.one_click);
        assert_eq!(unsub.target, "https://dispatch.design/unsubscribe?u=1");
        assert_eq!(parsed.category, InboxCategory::Newsletters);
        assert!(parsed.raw_html.is_some());
        assert!(parsed.body_text.contains("Type that breathes & shines."));
        assert!(parsed.date.unwrap().0 > 1_600_000_000_000);
    }

    #[test]
    fn receipt_subject_classifies_as_receipts() {
        let raw = b"Message-ID: <r1@figma.com>\r\nFrom: billing@figma.com\r\nSubject: Your Figma receipt\r\n\r\nPaid.";
        let parsed = parse_raw(raw, None, true, UnixMs(0)).unwrap();
        assert_eq!(parsed.category, InboxCategory::Receipts);
        assert!(parsed.is_read);
    }

    #[test]
    fn plain_text_only_message_has_no_raw_html() {
        let raw = b"Message-ID: <p1@x>\r\nFrom: a@b.c\r\nSubject: Plain\r\n\r\nJust text.\r\nSecond line.";
        let parsed = parse_raw(raw, None, false, UnixMs(0)).unwrap();
        assert!(parsed.raw_html.is_none());
        assert!(parsed.body_text.contains("Just text."));
        assert_eq!(parsed.snippet, "Just text. Second line.");
    }
}
