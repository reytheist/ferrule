//! Async IMAP TLS transport (`async-imap` + `tokio-rustls`):
//! - Supports TLS (rustls + webpki-roots) and plaintext (for local test servers).
//! - Supports `LOGIN` and `AUTHENTICATE XOAUTH2`.
//! - `LIST` + `STATUS` delta discovery so only folders with changed counts/UIDNEXT are opened.
//! - Uncapped incremental sync, header-only historical backfill, priority/bulk body downloads,
//!   on-demand attachment fetches, remote action execution, Sent verification, and interruptible
//!   IMAP IDLE.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_imap::types::Flag;
use async_imap::{Authenticator, Client, Session};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use rustls::pki_types::ServerName;
use rustls::RootCertStore;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use zeroize::Zeroizing;

use crate::auth::{CredentialKind, ProviderAuth, SecretString};
use crate::error::CoreError;
use crate::ids::{AccountId, UnixMs};
use crate::sync::pacing::{classify_error_message, FailureKind};
use crate::sync::{parse_headers_only, parse_raw, IngestMessage};
use crate::types::MailAction;

pub(crate) const FETCH_BATCH_SIZE: usize = 25;
pub(crate) const HEADER_BACKFILL_BATCH: usize = 50;
pub(crate) const FETCH_FULL_QUERY: &str = "(UID BODY.PEEK[] FLAGS)";
pub(crate) const FETCH_HEADER_QUERY: &str = "(UID BODY.PEEK[HEADER] FLAGS)";
pub(crate) const FETCH_FLAGS_QUERY: &str = "(UID FLAGS)";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Unified async stream supporting both TLS (`tokio-rustls`) and plaintext (for local tests).
pub(crate) enum ImapStream {
    Tls(Box<TlsStream<TcpStream>>),
    Plain(TcpStream),
}

impl std::fmt::Debug for ImapStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tls(_) => f.write_str("ImapStream::Tls"),
            Self::Plain(_) => f.write_str("ImapStream::Plain"),
        }
    }
}

impl Unpin for ImapStream {}

impl AsyncRead for ImapStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
            Self::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ImapStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match &mut *self {
            Self::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
            Self::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
            Self::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
            Self::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub(crate) type ImapSession = Session<ImapStream>;

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
    pub secret: SecretString,
    pub folder: String,
    pub since: UnixMs,
    pub now: UnixMs,
    pub gmail_dialect: bool,
    pub min_uid: i64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SyncWindow {
    pub since: UnixMs,
    pub now: UnixMs,
    pub min_uid: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct FetchedWindow {
    pub messages: Vec<IngestMessage>,
    pub uid_validity: i64,
    pub high_uid: i64,
    pub low_uid: i64,
    pub uid_next: u32,
    pub messages_count: u32,
    pub unseen_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderStatus {
    pub name: String,
    pub uid_validity: i64,
    pub uid_next: u32,
    pub messages: u32,
    pub unseen: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct FlagSnapshot {
    pub seen_uids: Vec<u32>,
    pub unseen_uids: Vec<u32>,
}

struct XOAuth2Authenticator {
    user: String,
    access_token: Zeroizing<String>,
    sent: bool,
}

impl Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&mut self, _data: &[u8]) -> Self::Response {
        if self.sent {
            String::new()
        } else {
            self.sent = true;
            format!(
                "user={}\x01auth=Bearer {}\x01\x01",
                self.user,
                self.access_token.as_str()
            )
        }
    }
}

pub(crate) fn map_imap_err(
    err: &async_imap::error::Error,
    account_id: AccountId,
    auth_phase: bool,
) -> CoreError {
    let text = err.to_string();
    match classify_error_message(&text, auth_phase) {
        FailureKind::AuthFailure => CoreError::AuthRequired {
            account_id,
            reason: format!("IMAP server rejected login: {text}"),
        },
        FailureKind::RateLimited { .. } => CoreError::Network {
            reason: text,
            retryable: true,
        },
        FailureKind::TransientNetwork => CoreError::Network {
            reason: text,
            retryable: true,
        },
    }
}

fn build_tls_connector() -> Result<TlsConnector, CoreError> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| CoreError::Internal {
            reason: format!("rustls protocol versions: {e}"),
        })?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

/// Open an authenticated async IMAP session over TLS (or plaintext for local tests).
pub(crate) async fn open_session(plan: &FetchPlan) -> Result<ImapSession, CoreError> {
    let addr = format!("{}:{}", plan.host, plan.port);
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr))
        .await
        .map_err(|_| CoreError::Network {
            reason: format!("timed out connecting to IMAP server {addr}"),
            retryable: true,
        })?
        .map_err(|e| CoreError::Network {
            reason: format!("IMAP connect failed: {e}"),
            retryable: true,
        })?;

    let stream = if plan.plaintext {
        ImapStream::Plain(tcp)
    } else {
        let connector = build_tls_connector()?;
        let server_name = ServerName::try_from(plan.host.clone()).map_err(|e| {
            CoreError::InvalidInput {
                field: "host".to_string(),
                reason: format!("invalid IMAP TLS hostname '{}': {e}", plan.host),
            }
        })?;
        let tls = tokio::time::timeout(CONNECT_TIMEOUT, connector.connect(server_name, tcp))
            .await
            .map_err(|_| CoreError::Network {
                reason: format!("IMAP TLS handshake timed out for {}", plan.host),
                retryable: true,
            })?
            .map_err(|e| CoreError::Network {
                reason: format!("IMAP TLS handshake failed: {e}"),
                retryable: true,
            })?;
        ImapStream::Tls(Box::new(tls))
    };

    let mut client = Client::new(stream);
    let _greeting = client
        .read_response()
        .await
        .map_err(|e| CoreError::Network {
            reason: format!("IMAP greeting failed: {e}"),
            retryable: true,
        })?;

    match plan.auth_mode {
        ImapAuthMode::Login => client
            .login(&plan.username, plan.secret.expose())
            .await
            .map_err(|(e, _)| map_imap_err(&e, plan.account_id, true)),
        ImapAuthMode::XOAuth2 => {
            let auth = XOAuth2Authenticator {
                user: plan.username.clone(),
                access_token: Zeroizing::new(plan.secret.expose().to_string()),
                sent: false,
            };
            client
                .authenticate("XOAUTH2", auth)
                .await
                .map_err(|(e, _)| map_imap_err(&e, plan.account_id, true))
        }
    }
}

fn since_imap_date(since: UnixMs) -> String {
    let at = DateTime::from_timestamp_millis(since.0)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).expect("epoch"));
    at.with_timezone(&Utc).format("%d-%b-%Y").to_string()
}

/// Query folder statuses using `LIST` + `STATUS (MESSAGES UNSEEN UIDNEXT UIDVALIDITY)`
/// across key mailboxes so the actor only `SELECT`s folders whose counts/UIDNEXT changed.
pub(crate) async fn list_folder_statuses(
    session: &mut ImapSession,
    account_id: AccountId,
    primary_folder: &str,
) -> Result<Vec<FolderStatus>, CoreError> {
    let mut candidate_folders: Vec<String> = vec![primary_folder.to_string()];

    if let Ok(stream) = session.list(Some(""), Some("*")).await {
        if let Ok(names) = stream.try_collect::<Vec<_>>().await {
            for name in names {
                let folder = name.name().to_string();
                let lower = folder.to_ascii_lowercase();
                let is_relevant = lower == "inbox"
                    || lower == "sent"
                    || lower == "sent items"
                    || lower == "[gmail]/sent mail"
                    || lower == "archive"
                    || lower == "[gmail]/all mail";
                if is_relevant && !candidate_folders.iter().any(|f| f.eq_ignore_ascii_case(&folder))
                {
                    candidate_folders.push(folder);
                }
            }
        }
    }

    let mut statuses = Vec::with_capacity(candidate_folders.len());
    for folder in candidate_folders {
        match session
            .status(&folder, "(MESSAGES UNSEEN UIDNEXT UIDVALIDITY)")
            .await
        {
            Ok(mb) => {
                statuses.push(FolderStatus {
                    name: folder,
                    uid_validity: mb.uid_validity.map(i64::from).unwrap_or(0),
                    uid_next: mb.uid_next.unwrap_or(0),
                    messages: mb.exists,
                    unseen: mb.unseen.unwrap_or(0),
                });
            }
            Err(_) if !folder.eq_ignore_ascii_case(primary_folder) => {
                // Non-primary folder may be non-selectable (\Noselect); skip cleanly.
                continue;
            }
            Err(e) => return Err(map_imap_err(&e, account_id, false)),
        }
    }
    Ok(statuses)
}

/// Fetch all new messages in `plan.folder` since `plan.since` with UID > `plan.min_uid`.
/// Removes the Phase 1 50-message cap: streams in bounded batches of `FETCH_BATCH_SIZE`.
pub(crate) async fn fetch_window_on_session(
    session: &mut ImapSession,
    plan: &FetchPlan,
) -> Result<FetchedWindow, CoreError> {
    let mailbox = session
        .select(&plan.folder)
        .await
        .map_err(|e| map_imap_err(&e, plan.account_id, false))?;
    let uid_validity = mailbox.uid_validity.map(i64::from).unwrap_or(0);
    let uid_next = mailbox.uid_next.unwrap_or(0);
    let messages_count = mailbox.exists;
    let unseen_count = mailbox.unseen.unwrap_or(0);

    let query = format!("SINCE {}", since_imap_date(plan.since));
    let mut uids: Vec<u32> = session
        .uid_search(query)
        .await
        .map_err(|e| map_imap_err(&e, plan.account_id, false))?
        .into_iter()
        .collect();
    uids.retain(|uid| i64::from(*uid) > plan.min_uid);
    uids.sort_unstable();

    let low_uid = uids.first().copied().map(i64::from).unwrap_or(0);
    let high_uid = uids.last().copied().map(i64::from).unwrap_or(plan.min_uid);
    if uids.is_empty() {
        return Ok(FetchedWindow {
            messages: Vec::new(),
            uid_validity,
            high_uid,
            low_uid,
            uid_next,
            messages_count,
            unseen_count,
        });
    }

    let messages = fetch_full_uids(session, plan, &uids).await?;
    Ok(FetchedWindow {
        messages,
        uid_validity,
        high_uid,
        low_uid,
        uid_next,
        messages_count,
        unseen_count,
    })
}

/// One-shot convenience wrapper (opens session, fetches window, logs out).
pub(crate) async fn fetch_window(plan: &FetchPlan) -> Result<FetchedWindow, CoreError> {
    let mut session = open_session(plan).await?;
    let result = fetch_window_on_session(&mut session, plan).await;
    let _ = session.logout().await;
    result
}

/// Fetch full RFC822 bodies + FLAGS for a set of UIDs in bounded batches.
pub(crate) async fn fetch_full_uids(
    session: &mut ImapSession,
    plan: &FetchPlan,
    uids: &[u32],
) -> Result<Vec<IngestMessage>, CoreError> {
    let mut messages = Vec::with_capacity(uids.len());
    for chunk in uids.chunks(FETCH_BATCH_SIZE) {
        let uid_set = chunk
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fetches = session
            .uid_fetch(uid_set, FETCH_FULL_QUERY)
            .await
            .map_err(|e| map_imap_err(&e, plan.account_id, false))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| map_imap_err(&e, plan.account_id, false))?;

        for item in fetches {
            let Some(body) = item.body() else { continue };
            let is_read = item.flags().any(|flag| matches!(flag, Flag::Seen));
            let gm_thrid = if plan.gmail_dialect {
                extract_gm_thrid(body)
            } else {
                None
            };
            if let Some(mut parsed) = parse_raw(body, gm_thrid, is_read, plan.now) {
                parsed.folder_name = plan.folder.clone();
                parsed.imap_uid = item.uid;
                messages.push(parsed);
            }
        }
    }
    Ok(messages)
}

/// Historical header-only backfill: fetch headers for UIDs below `before_uid` (walking backwards).
pub(crate) async fn fetch_historical_headers(
    plan: &FetchPlan,
    before_uid: u32,
    batch_limit: usize,
) -> Result<(Vec<IngestMessage>, u32, bool), CoreError> {
    if before_uid <= 1 {
        return Ok((Vec::new(), 1, true));
    }
    let mut session = open_session(plan).await?;
    session
        .select(&plan.folder)
        .await
        .map_err(|e| map_imap_err(&e, plan.account_id, false))?;

    let end_uid = before_uid.saturating_sub(1);
    let range = format!("1:{end_uid}");
    let mut uids: Vec<u32> = session
        .uid_search(format!("UID {range}"))
        .await
        .map_err(|e| map_imap_err(&e, plan.account_id, false))?
        .into_iter()
        .collect();
    uids.sort_unstable();

    if uids.is_empty() {
        let _ = session.logout().await;
        return Ok((Vec::new(), 1, true));
    }

    let start_idx = uids.len().saturating_sub(batch_limit.max(1));
    let slice = &uids[start_idx..];
    let new_low_uid = slice.first().copied().unwrap_or(1);
    let done = start_idx == 0 || new_low_uid <= 1;

    let mut messages = Vec::with_capacity(slice.len());
    for chunk in slice.chunks(FETCH_BATCH_SIZE) {
        let uid_set = chunk
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fetches = session
            .uid_fetch(uid_set, FETCH_HEADER_QUERY)
            .await
            .map_err(|e| map_imap_err(&e, plan.account_id, false))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| map_imap_err(&e, plan.account_id, false))?;

        for item in fetches {
            let Some(header_bytes) = item.header().or_else(|| item.body()) else {
                continue;
            };
            let is_read = item.flags().any(|flag| matches!(flag, Flag::Seen));
            let gm_thrid = if plan.gmail_dialect {
                extract_gm_thrid(header_bytes)
            } else {
                None
            };
            if let Some(uid) = item.uid {
                if let Some(parsed) = parse_headers_only(
                    header_bytes,
                    gm_thrid,
                    is_read,
                    plan.now,
                    plan.folder.clone(),
                    uid,
                ) {
                    messages.push(parsed);
                }
            }
        }
    }

    let _ = session.logout().await;
    Ok((messages, new_low_uid, done))
}

/// Fetch full bodies for specific `(folder, uid)` targets on a dedicated connection.
pub(crate) async fn fetch_bodies_for_uids(
    plan: &FetchPlan,
    uids: &[u32],
) -> Result<Vec<IngestMessage>, CoreError> {
    if uids.is_empty() {
        return Ok(Vec::new());
    }
    let mut session = open_session(plan).await?;
    session
        .select(&plan.folder)
        .await
        .map_err(|e| map_imap_err(&e, plan.account_id, false))?;
    let result = fetch_full_uids(&mut session, plan, uids).await;
    let _ = session.logout().await;
    result
}

/// Periodic flag reconciliation pass: fetch `(UID FLAGS)` for all UIDs in the folder.
pub(crate) async fn fetch_folder_flags(
    session: &mut ImapSession,
    account_id: AccountId,
    folder: &str,
) -> Result<FlagSnapshot, CoreError> {
    session
        .select(folder)
        .await
        .map_err(|e| map_imap_err(&e, account_id, false))?;
    let fetches = session
        .uid_fetch("1:*", FETCH_FLAGS_QUERY)
        .await
        .map_err(|e| map_imap_err(&e, account_id, false))?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| map_imap_err(&e, account_id, false))?;

    let mut seen_uids = Vec::new();
    let mut unseen_uids = Vec::new();
    for item in fetches {
        let Some(uid) = item.uid else { continue };
        if item.flags().any(|f| matches!(f, Flag::Seen)) {
            seen_uids.push(uid);
        } else {
            unseen_uids.push(uid);
        }
    }
    Ok(FlagSnapshot {
        seen_uids,
        unseen_uids,
    })
}

/// Apply one queued offline [`MailAction`] to remote IMAP UIDs in `folder`.
pub(crate) async fn apply_remote_action(
    session: &mut ImapSession,
    account_id: AccountId,
    folder: &str,
    uids: &[u32],
    action: &MailAction,
    gmail_dialect: bool,
) -> Result<(), CoreError> {
    if uids.is_empty() {
        return Ok(());
    }
    session
        .select(folder)
        .await
        .map_err(|e| map_imap_err(&e, account_id, false))?;
    let uid_set = uids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");

    match action {
        MailAction::MarkRead { read: true } => {
            let stream = session
                .uid_store(&uid_set, "+FLAGS.SILENT (\\Seen)")
                .await
                .map_err(|e| map_imap_err(&e, account_id, false))?;
            let _ = stream.try_collect::<Vec<_>>().await;
        }
        MailAction::MarkRead { read: false } => {
            let stream = session
                .uid_store(&uid_set, "-FLAGS.SILENT (\\Seen)")
                .await
                .map_err(|e| map_imap_err(&e, account_id, false))?;
            let _ = stream.try_collect::<Vec<_>>().await;
        }
        MailAction::Star { on: true } => {
            let stream = session
                .uid_store(&uid_set, "+FLAGS.SILENT (\\Flagged)")
                .await
                .map_err(|e| map_imap_err(&e, account_id, false))?;
            let _ = stream.try_collect::<Vec<_>>().await;
        }
        MailAction::Star { on: false } => {
            let stream = session
                .uid_store(&uid_set, "-FLAGS.SILENT (\\Flagged)")
                .await
                .map_err(|e| map_imap_err(&e, account_id, false))?;
            let _ = stream.try_collect::<Vec<_>>().await;
        }
        MailAction::Trash => {
            let dest = if gmail_dialect {
                "[Gmail]/Trash"
            } else {
                "Trash"
            };
            move_or_flag_deleted(session, account_id, &uid_set, dest).await?;
        }
        MailAction::ReportSpam => {
            let dest = if gmail_dialect {
                "[Gmail]/Spam"
            } else {
                "Junk"
            };
            move_or_flag_deleted(session, account_id, &uid_set, dest).await?;
        }
        MailAction::Archive => {
            if gmail_dialect {
                // In Gmail IMAP, removing from INBOX archives into All Mail.
                let stream = session
                    .uid_store(&uid_set, "+FLAGS.SILENT (\\Deleted)")
                    .await
                    .map_err(|e| map_imap_err(&e, account_id, false))?;
                let _ = stream.try_collect::<Vec<_>>().await;
                let _ = session.expunge().await;
            } else {
                move_or_flag_deleted(session, account_id, &uid_set, "Archive").await?;
            }
        }
        MailAction::Unarchive => {
            move_or_flag_deleted(session, account_id, &uid_set, "INBOX").await?;
        }
    }
    Ok(())
}

async fn move_or_flag_deleted(
    session: &mut ImapSession,
    account_id: AccountId,
    uid_set: &str,
    dest: &str,
) -> Result<(), CoreError> {
    if session.uid_mv(uid_set, dest).await.is_ok() {
        return Ok(());
    }
    // Fallback for servers without MOVE extension: COPY + \Deleted + EXPUNGE.
    if session.uid_copy(uid_set, dest).await.is_ok() {
        let stream = session
            .uid_store(uid_set, "+FLAGS.SILENT (\\Deleted)")
            .await
            .map_err(|e| map_imap_err(&e, account_id, false))?;
        let _ = stream.try_collect::<Vec<_>>().await;
        let _ = session.expunge().await;
    }
    Ok(())
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
pub(crate) async fn verify_in_sent(
    plan: &FetchPlan,
    message_id_hdr: &str,
) -> Result<bool, CoreError> {
    let mut session = open_session(plan).await?;
    let folders: Vec<&str> = if plan.gmail_dialect {
        vec!["[Gmail]/Sent Mail", plan.folder.as_str()]
    } else {
        vec!["Sent Items", "Sent", "INBOX"]
    };

    let safe_id = message_id_hdr.replace('"', "");
    let query = format!("HEADER Message-ID \"{safe_id}\"");
    let mut searched_any = false;
    let mut last_err: Option<CoreError> = None;

    for folder in folders {
        match session.select(folder).await {
            Ok(_) => {
                searched_any = true;
                match session.uid_search(query.clone()).await {
                    Ok(hits) if !hits.is_empty() => {
                        let _ = session.logout().await;
                        return Ok(true);
                    }
                    Ok(_) => {}
                    Err(e) => last_err = Some(map_imap_err(&e, plan.account_id, false)),
                }
            }
            Err(e) => last_err = Some(map_imap_err(&e, plan.account_id, false)),
        }
    }
    let _ = session.logout().await;

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
    auth: &ProviderAuth,
    secret: SecretString,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_queries_are_parenthesized_and_non_destructive() {
        assert!(FETCH_FULL_QUERY.starts_with('(') && FETCH_FULL_QUERY.ends_with(')'));
        assert!(FETCH_FULL_QUERY.contains("BODY.PEEK[]"));
        assert!(FETCH_FULL_QUERY.contains("FLAGS"));
        assert!(FETCH_HEADER_QUERY.contains("BODY.PEEK[HEADER]"));
    }
}
