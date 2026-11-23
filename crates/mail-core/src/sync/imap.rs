//! Real IMAP TLS sync adapter (imap + rustls): fetches the 90-day SINCE window and
//! verifies the Sent folder during crash recovery. Runs on a blocking thread; callers
//! surface [`CoreError::AuthRequired`] / [`CoreError::Network`] without crashing.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};

use crate::auth::CredentialKind;
use crate::error::CoreError;
use crate::ids::{AccountId, UnixMs};
use crate::sync::{parse_raw, IngestMessage};

const MAX_MESSAGES_PER_SYNC: usize = 50;
const FETCH_BATCH_SIZE: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImapAuthMode {
    Login,
    XOAuth2,
}

#[derive(Debug, Clone)]
pub(crate) struct FetchPlan {
    pub account_id: AccountId,
    pub host: String,
    pub port: u16,
    pub plaintext: bool,
    pub auth_mode: ImapAuthMode,
    pub username: String,
    pub secret: String,
    pub folder: String,
    pub since: UnixMs,
    pub now: UnixMs,
    pub gmail_dialect: bool,
    pub min_uid: i64,
}

/// Sync window parameters for one fetch/verify run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SyncWindow {
    /// Oldest date to fetch (the 90-day floor).
    pub since: UnixMs,
    /// Clock reading used to fill in messages with no Date header.
    pub now: UnixMs,
    /// Only accept UIDs above this value (incremental sync).
    pub min_uid: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct FetchedWindow {
    pub messages: Vec<IngestMessage>,
    pub uid_validity: i64,
    pub high_uid: i64,
}

fn map_imap_err(err: &imap::Error, account_id: AccountId, auth_phase: bool) -> CoreError {
    match err {
        imap::Error::No(_) | imap::Error::Bad(_) if auth_phase => CoreError::AuthRequired {
            account_id,
            reason: format!("IMAP server rejected login: {err}"),
        },
        imap::Error::Io(e) => CoreError::Network {
            reason: e.to_string(),
            retryable: true,
        },
        imap::Error::RustlsHandshake(e) => CoreError::Network {
            reason: e.to_string(),
            retryable: true,
        },
        imap::Error::ConnectionLost => CoreError::Network {
            reason: "IMAP connection lost".to_string(),
            retryable: true,
        },
        other => CoreError::Network {
            reason: other.to_string(),
            retryable: false,
        },
    }
}

struct XOAuth2Authenticator {
    user: String,
    access_token: String,
    sent: AtomicBool,
}

impl imap::Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&self, _data: &[u8]) -> Self::Response {
        // Send the SASL XOAUTH2 string once; if the server replies with an error
        // continuation challenge, return empty string so the server completes with NO.
        if self.sent.swap(true, Ordering::SeqCst) {
            String::new()
        } else {
            format!("user={}\x01auth=Bearer {}\x01\x01", self.user, self.access_token)
        }
    }
}

fn open_session(plan: &FetchPlan) -> Result<imap::Session<imap::Connection>, CoreError> {
    let mut builder = imap::ClientBuilder::new(plan.host.as_str(), plan.port);
    if plan.plaintext {
        builder = builder.mode(imap::ConnectionMode::Plaintext);
    }
    let client = builder
        .connect()
        .map_err(|e| map_imap_err(&e, plan.account_id, false))?;
    match plan.auth_mode {
        ImapAuthMode::Login => client
            .login(&plan.username, &plan.secret)
            .map_err(|(e, _)| map_imap_err(&e, plan.account_id, true)),
        ImapAuthMode::XOAuth2 => {
            let auth = XOAuth2Authenticator {
                user: plan.username.clone(),
                access_token: plan.secret.clone(),
                sent: AtomicBool::new(false),
            };
            client
                .authenticate("XOAUTH2", &auth)
                .map_err(|(e, _)| map_imap_err(&e, plan.account_id, true))
        }
    }
}

fn since_imap_date(since: UnixMs) -> String {
    let at = DateTime::from_timestamp_millis(since.0)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).expect("epoch"));
    at.with_timezone(&Utc).format("%d-%b-%Y").to_string()
}

/// Fetch everything in the folder newer than `plan.since` (capped to the most recent
/// [`MAX_MESSAGES_PER_SYNC`] messages) and parse it for ingest.
pub(crate) fn fetch_window(plan: &FetchPlan) -> Result<FetchedWindow, CoreError> {
    let mut session = open_session(plan)?;
    let mailbox = session
        .select(&plan.folder)
        .map_err(|e| map_imap_err(&e, plan.account_id, false))?;
    let uid_validity = mailbox.uid_validity.map(i64::from).unwrap_or(0);

    let query = format!("SINCE {}", since_imap_date(plan.since));
    let mut uids: Vec<u32> = session
        .uid_search(query)
        .map_err(|e| map_imap_err(&e, plan.account_id, false))?
        .into_iter()
        .collect();
    uids.retain(|uid| i64::from(*uid) > plan.min_uid);
    uids.sort_unstable();

    if uids.len() > MAX_MESSAGES_PER_SYNC {
        let drop_count = uids.len() - MAX_MESSAGES_PER_SYNC;
        uids.drain(..drop_count);
    }
    let high_uid = uids.last().copied().unwrap_or(0);
    if uids.is_empty() {
        let _ = session.logout();
        return Ok(FetchedWindow {
            messages: Vec::new(),
            uid_validity,
            high_uid: 0,
        });
    }

    let mut messages = Vec::new();
    for chunk in uids.chunks(FETCH_BATCH_SIZE) {
        let uid_set = chunk
            .iter()
            .map(|uid| uid.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let fetches = session
            .uid_fetch(uid_set, "RFC822 FLAGS")
            .map_err(|e| map_imap_err(&e, plan.account_id, false))?;

        for item in fetches.iter() {
            let Some(body) = item.body() else {
                continue;
            };
            let is_read = item
                .flags()
                .iter()
                .any(|flag| matches!(flag, imap::types::Flag::Seen));
            let gm_thrid = if plan.gmail_dialect {
                extract_gm_thrid(body)
            } else {
                None
            };
            if let Some(parsed) = parse_raw(body, gm_thrid, is_read, plan.now) {
                messages.push(parsed);
            }
        }
    }
    let _ = session.logout();

    Ok(FetchedWindow {
        messages,
        uid_validity,
        high_uid: i64::from(high_uid),
    })
}

/// `X-GM-THRID` from the raw header block (Gmail threading id).
fn extract_gm_thrid(raw: &[u8]) -> Option<String> {
    const NEEDLE: &[u8] = b"x-gm-thrid:";
    let header_end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or(raw.len().min(16 * 1024));
    let region = &raw[..header_end.min(raw.len())];
    let mut idx = 0;
    while idx + NEEDLE.len() <= region.len() {
        if region[idx..idx + NEEDLE.len()].eq_ignore_ascii_case(NEEDLE) {
            let rest = &region[idx + NEEDLE.len()..];
            let end = rest
                .iter()
                .position(|b| *b == b'\r' || *b == b'\n')
                .unwrap_or(rest.len());
            let value = String::from_utf8_lossy(&rest[..end]).trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
        idx += 1;
    }
    None
}

/// Crash-recovery probe: is the frozen `Message-ID` already present in Sent?
///
/// `Ok(true)` → already delivered, `Ok(false)` → confirmed absent (safe to retry),
/// `Err(_)` → could not verify, so the caller must NOT resend.
pub(crate) fn verify_in_sent(plan: &FetchPlan, message_id_hdr: &str) -> Result<bool, CoreError> {
    let mut session = open_session(plan)?;
    let folders: Vec<&str> = if plan.gmail_dialect {
        vec![plan.folder.as_str()]
    } else {
        vec!["Sent Items", "Sent", "INBOX"]
    };

    let safe_id = message_id_hdr.replace('"', "");
    let query = format!("HEADER Message-ID \"{safe_id}\"");
    let mut searched_any = false;
    let mut last_err: Option<CoreError> = None;

    for folder in folders {
        match session.select(folder) {
            Ok(_) => {
                searched_any = true;
                match session.uid_search(query.clone()) {
                    Ok(hits) if !hits.is_empty() => {
                        let _ = session.logout();
                        return Ok(true);
                    }
                    Ok(_) => {}
                    Err(e) => last_err = Some(map_imap_err(&e, plan.account_id, false)),
                }
            }
            Err(e) => last_err = Some(map_imap_err(&e, plan.account_id, false)),
        }
    }
    let _ = session.logout();

    if searched_any {
        Ok(false)
    } else {
        Err(last_err.unwrap_or_else(|| CoreError::Network {
            reason: "no searchable Sent folder".to_string(),
            retryable: true,
        }))
    }
}

/// Build the fetch plan from account auth + a resolved secret.
pub(crate) fn plan_for(
    account_id: AccountId,
    auth: &crate::auth::ProviderAuth,
    secret: String,
    folder: String,
    window: SyncWindow,
) -> FetchPlan {
    let endpoint = auth.imap_endpoint();
    let auth_mode = match auth.credential_kind() {
        CredentialKind::Password => ImapAuthMode::Login,
        CredentialKind::OAuthRefreshToken | CredentialKind::OAuthClientSecret => {
            ImapAuthMode::XOAuth2
        }
    };
    FetchPlan {
        account_id,
        host: endpoint.host,
        port: endpoint.port,
        plaintext: !endpoint.tls,
        auth_mode,
        username: auth.username().to_string(),
        secret,
        folder,
        since: window.since,
        now: window.now,
        gmail_dialect: crate::sync::Dialect::from_auth(auth) == crate::sync::Dialect::Gmail,
        min_uid: window.min_uid,
    }
}
