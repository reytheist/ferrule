pub mod agent;
pub mod auth;
pub mod calendar;
pub mod error;
pub mod events;
pub mod host;
pub mod ids;
pub mod sanitize;
pub mod schedule;
pub mod staff;
pub mod types;

mod calendar_core;
mod staff_core;
mod store;
mod sync;

pub use crate::sync::staff::{parse_sse_frame, StaffSyncEvent, StaffSyncSnapshot};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::agent::AgentPort;
use crate::auth::{CredentialKind, OAuthClient, ProviderAuth, ProviderKind, SecretKey, SecretStore, SecretString};
use crate::error::CoreError;
use crate::events::{Alert, CoreEvent};
use crate::host::{Clock, OsKeychain, SystemClock};
use crate::ids::{AccountId, AgentId, DraftId, JobId, TemplateId, ThreadId, UnixMs};
use crate::schedule::{
    step_follow_up, step_outbox, step_snooze, FollowUpEvent, FollowUpState, OutboxEffect,
    OutboxEvent, OutboxState, SnoozeEvent, SnoozeState,
};
use crate::store::JobKind;
use crate::sync::smtp::{freeze_message, send_frozen, FreezeInput, FrozenMessage, SmtpOutcome};
use crate::sync::Dialect;
use crate::types::*;

#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub db_path: Option<PathBuf>,
    pub sync_window_days: i64,
    pub default_undo_secs: u32,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            db_path: None,
            sync_window_days: 90,
            default_undo_secs: 7,
        }
    }
}

pub struct Host {
    pub secrets: Arc<dyn SecretStore>,
    pub clock: Arc<dyn Clock>,
}

impl Host {
    pub fn desktop() -> Self {
        Self {
            secrets: Arc::new(OsKeychain::default()),
            clock: Arc::new(SystemClock),
        }
    }
}

struct CachedAccessToken {
    token: SecretString,
    expires_at: UnixMs,
}

struct MailCoreInner {
    config: CoreConfig,
    store: store::Store,
    host: Host,
    events: broadcast::Sender<CoreEvent>,
    token_cache: Mutex<HashMap<AccountId, CachedAccessToken>>,
}

#[derive(Clone)]
pub struct MailCore {
    inner: Arc<MailCoreInner>,
}

/// Crate-private proof that a human code path (send or approval) authorized this send.
///
/// The type is not exported from the crate, so Phase-2 MCP servers — which only ever
/// receive an [`AgentPort`] — cannot name, construct, or obtain one. This is the
/// compile-time send-safety boundary described in the architecture.
pub(crate) struct Sendable {
    job_id: JobId,
    draft_id: DraftId,
    account_id: AccountId,
    reply_to_thread: Option<ThreadId>,
    frozen: FrozenMessage,
    summary: String,
}

enum SendExpect {
    /// `send_draft`: human-originated draft still in `Editing`.
    Editing,
    /// `approve_agent_draft`: agent-originated draft still in `PendingApproval`.
    PendingApproval,
}

impl MailCore {
    pub fn open(config: CoreConfig, host: Host) -> Result<Self, CoreError> {
        let store = match &config.db_path {
            Some(path) => store::Store::open(path)?,
            None => store::Store::open_in_memory()?,
        };
        let (events, _) = broadcast::channel(128);
        Ok(Self {
            inner: Arc::new(MailCoreInner {
                config,
                store,
                host,
                events,
                token_cache: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn config(&self) -> &CoreConfig {
        &self.inner.config
    }

    pub fn now(&self) -> UnixMs {
        self.inner.host.clock.now()
    }

    pub fn secrets(&self) -> &dyn SecretStore {
        self.inner.host.secrets.as_ref()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.inner.events.subscribe()
    }

    pub fn agent_port(&self, agent_id: AgentId) -> AgentPort {
        AgentPort::new(agent_id, self.clone())
    }

    pub fn primary_folder_for(&self, auth: &ProviderAuth) -> &'static str {
        Dialect::from_auth(auth).primary_sync_folder()
    }

    pub fn db_health_check(&self) -> Result<(), CoreError> {
        self.inner.store.with_conn(|conn| {
            conn.execute_batch("SELECT 1;")
                .map_err(|e| CoreError::Storage {
                    reason: e.to_string(),
                })
        })
    }

    fn emit(&self, event: CoreEvent) {
        let _ = self.inner.events.send(event);
    }

    // ---------------------------------------------------------------- settings

    pub async fn get_settings(&self) -> Result<AppSettings, CoreError> {
        self.inner.store.with_conn(store::get_app_settings)
    }

    pub async fn update_settings(&self, settings: AppSettings) -> Result<AppSettings, CoreError> {
        self.inner
            .store
            .with_conn(|conn| store::save_app_settings(conn, &settings))?;
        Ok(settings)
    }

    // ---------------------------------------------------------------- accounts

    pub async fn list_accounts(&self) -> Result<Vec<AccountView>, CoreError> {
        self.inner.store.with_conn(store::account_views)
    }

    fn account_view(&self, account_id: AccountId) -> Result<Option<AccountView>, CoreError> {
        let views = self.inner.store.with_conn(store::account_views)?;
        Ok(views.into_iter().find(|view| view.id == account_id))
    }

    pub async fn add_account(&self, input: NewAccountInput) -> Result<AccountView, CoreError> {
        let display_name = input.display_name.trim();
        if display_name.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "displayName".to_string(),
                reason: "account name is required".to_string(),
            });
        }
        input.auth.validate()?;
        let secret = input.secret.and_then(|raw| {
            let normalized = match &input.auth {
                ProviderAuth::GmailAppPassword { .. } => {
                    raw.chars().filter(|c| !c.is_whitespace()).collect::<String>()
                }
                _ => raw.trim().to_string(),
            };
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        });
        if matches!(input.auth.credential_kind(), CredentialKind::Password) && secret.is_none() {
            return Err(CoreError::InvalidInput {
                field: "secret".to_string(),
                reason: "this provider requires a password or app password".to_string(),
            });
        }

        let now = self.now();
        let sync_window = self.inner.config.sync_window_days;
        let account_id = self.inner.store.with_conn(|conn| {
            let id = store::insert_account(
                conn,
                display_name,
                input.color,
                input.space_kind,
                &input.auth,
                &SyncState::Idle { last_ok: None },
                now,
            )?;
            store::insert_identity(conn, id, input.auth.address(), None, true)?;
            store::upsert_folder_cursor(
                conn,
                id,
                Dialect::from_auth(&input.auth).primary_sync_folder(),
                now.sub_days(sync_window),
                now,
            )?;
            Ok(id)
        })?;

        // Secrets go to the SecretStore only; on failure the account row is rolled
        // back so the database never holds an account without its credential.
        if let Some(secret) = secret {
            let key = SecretKey {
                account_id,
                kind: input.auth.credential_kind(),
            };
            if let Err(err) = self
                .inner
                .host
                .secrets
                .put(&key, &SecretString::new(secret))
            {
                let _ = self
                    .inner
                    .store
                    .with_conn(|conn| store::delete_account(conn, account_id));
                return Err(err);
            }
        }

        self.emit(CoreEvent::Resync);
        self.account_view(account_id)?
            .ok_or_else(|| CoreError::Internal {
                reason: "account disappeared after insert".to_string(),
            })
    }

    pub async fn remove_account(&self, account_id: AccountId) -> Result<(), CoreError> {
        let auth = self
            .inner
            .store
            .with_conn(|conn| store::get_account(conn, account_id))?
            .map(|account| account.auth)
            .ok_or_else(|| CoreError::NotFound {
                entity: "account".to_string(),
            })?;

        // Credential first: a keychain failure must not orphan the account row.
        let key = SecretKey {
            account_id,
            kind: auth.credential_kind(),
        };
        self.inner.host.secrets.delete(&key)?;
        let _ = self.inner.host.secrets.delete(&SecretKey {
            account_id,
            kind: CredentialKind::OAuthClientSecret,
        });
        if let Ok(mut cache) = self.inner.token_cache.lock() {
            cache.remove(&account_id);
        }

        self.inner
            .store
            .with_conn(|conn| store::delete_account(conn, account_id))?;
        self.emit(CoreEvent::Resync);
        Ok(())
    }

    pub async fn update_account(
        &self,
        input: UpdateAccountInput,
    ) -> Result<AccountView, CoreError> {
        let display_name = input.display_name.trim();
        if display_name.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "displayName".to_string(),
                reason: "space name is required".to_string(),
            });
        }
        let updated = self.inner.store.with_conn(|conn| {
            store::update_account_meta(
                conn,
                input.account_id,
                display_name,
                input.color,
                input.space_kind,
            )
        })?;
        if !updated {
            return Err(CoreError::NotFound {
                entity: "account".to_string(),
            });
        }
        self.emit(CoreEvent::Resync);
        self.account_view(input.account_id)?
            .ok_or_else(|| CoreError::NotFound {
                entity: "account".to_string(),
            })
    }

    /// Return the previously configured OAuth client IDs found on existing accounts so
    /// adding another Gmail or Outlook account can launch browser sign-in in one click.
    pub async fn saved_oauth_clients(&self) -> Result<SavedOAuthClients, CoreError> {
        self.inner.store.with_conn(|conn| {
            let rows = store::list_account_rows(conn)?;
            let mut clients = SavedOAuthClients::default();
            for row in rows {
                match row.auth {
                    ProviderAuth::GmailOAuth { client, .. } if clients.gmail_client_id.is_none() => {
                        let trimmed = client.client_id.trim();
                        if !trimmed.is_empty() {
                            clients.gmail_client_id = Some(trimmed.to_string());
                        }
                    }
                    ProviderAuth::OutlookOAuth {
                        client_id, tenant, ..
                    } if clients.outlook_client_id.is_none() => {
                        let trimmed = client_id.trim();
                        if !trimmed.is_empty() {
                            clients.outlook_client_id = Some(trimmed.to_string());
                            let t = tenant.trim();
                            clients.outlook_tenant = Some(if t.is_empty() {
                                "common".to_string()
                            } else {
                                t.to_string()
                            });
                        }
                    }
                    _ => {}
                }
            }
            Ok(clients)
        })
    }

    /// Run one-click browser OAuth2 PKCE sign-in, discover the selected account's email
    /// and name from the provider, auto-populate the space name and a unique unused color,
    /// persist the refresh token (and optional client secret) in the OS keychain, and
    /// create the account only after sign-in succeeds.
    pub async fn oauth_sign_in_account(
        &self,
        input: OAuthSignInInput,
    ) -> Result<AccountView, CoreError> {
        let saved = self.saved_oauth_clients().await?;
        let client_id = if !input.client_id.trim().is_empty() {
            input.client_id.trim().to_string()
        } else {
            match input.provider {
                ProviderKind::Gmail => saved.gmail_client_id.unwrap_or_default(),
                ProviderKind::Outlook => saved.outlook_client_id.unwrap_or_default(),
                ProviderKind::Imap => String::new(),
            }
        };
        let tenant = input
            .tenant
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .or(saved.outlook_tenant)
            .unwrap_or_else(|| "common".to_string());

        let endpoints = sync::oauth::endpoints_for_sign_in(
            input.provider,
            &client_id,
            Some(&tenant),
            None,
        )?;

        let explicit_secret = input
            .client_secret
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let lookup_auth = match input.provider {
            ProviderKind::Gmail => ProviderAuth::GmailOAuth {
                address: String::new(),
                client: OAuthClient {
                    client_id: client_id.clone(),
                    has_client_secret: explicit_secret.is_some(),
                },
            },
            ProviderKind::Outlook => ProviderAuth::OutlookOAuth {
                address: String::new(),
                client_id: client_id.clone(),
                tenant: tenant.clone(),
            },
            ProviderKind::Imap => {
                return Err(CoreError::InvalidInput {
                    field: "provider".to_string(),
                    reason: "browser sign-in is only supported for Gmail and Outlook".to_string(),
                });
            }
        };
        let resolved_secret = self.resolve_oauth_client_secret(&lookup_auth, explicit_secret, None)?;

        let bundle = sync::oauth::run_browser_pkce_flow_for_endpoints(
            &endpoints,
            resolved_secret.as_ref().map(SecretString::expose),
        )
        .await?;
        let profile = sync::oauth::fetch_profile(&endpoints, &bundle).await?;
        let refresh = bundle.refresh_token.ok_or_else(|| CoreError::Network {
            reason: "OAuth provider did not return a refresh token — revoke app access and try again"
                .to_string(),
            retryable: false,
        })?;
        let existing = self.list_accounts().await?;
        if existing
            .iter()
            .any(|acct| acct.email.eq_ignore_ascii_case(&profile.email))
        {
            return Err(CoreError::InvalidInput {
                field: "address".to_string(),
                reason: format!(
                    "{} is already connected — pick a different account in your browser",
                    profile.email
                ),
            });
        }

        let display_name = input
            .display_name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| default_space_name(&profile.email, &existing));
        let used_colors: Vec<SpaceColor> = existing.iter().map(|a| a.color).collect();
        let color = input
            .color
            .unwrap_or_else(|| SpaceColor::pick_unique(&used_colors));

        let auth = match input.provider {
            ProviderKind::Gmail => ProviderAuth::GmailOAuth {
                address: profile.email.clone(),
                client: OAuthClient {
                    client_id,
                    has_client_secret: resolved_secret.is_some(),
                },
            },
            ProviderKind::Outlook => ProviderAuth::OutlookOAuth {
                address: profile.email.clone(),
                client_id,
                tenant,
            },
            ProviderKind::Imap => unreachable!(),
        };

        let auth_for_secret = auth.clone();
        let created = self
            .add_account(NewAccountInput {
                display_name,
                color,
                space_kind: input.space_kind,
                auth,
                secret: Some(refresh),
            })
            .await?;

        if let Some(secret) = resolved_secret {
            let _ = self.inner.host.secrets.put(
                &SecretKey {
                    account_id: created.id,
                    kind: CredentialKind::OAuthClientSecret,
                },
                &secret,
            );
            if let Some(client_key) = Self::oauth_client_secret_key(&auth_for_secret) {
                let _ = self.inner.host.secrets.put(&client_key, &secret);
            }
        }
        self.cache_access_token(created.id, bundle.access_token, bundle.expires_in_secs);
        Ok(created)
    }

    /// Re-run browser OAuth2 PKCE sign-in for an existing Gmail/Outlook OAuth account.
    pub async fn reauth_oauth_account(&self, account_id: AccountId) -> Result<AccountView, CoreError> {
        let account = self.account_row(account_id)?;
        let client_secret = self.resolve_oauth_client_secret(&account.auth, None, Some(account_id))?;
        let bundle = sync::oauth::run_browser_pkce_flow(
            &account.auth,
            client_secret.as_ref().map(SecretString::expose),
        )
        .await?;

        if let Some(refresh) = bundle.refresh_token {
            self.inner.host.secrets.put(
                &SecretKey {
                    account_id,
                    kind: CredentialKind::OAuthRefreshToken,
                },
                &SecretString::new(refresh),
            )?;
        }
        self.cache_access_token(account_id, bundle.access_token, bundle.expires_in_secs);
        self.sync_account(account_id).await
    }

    fn oauth_client_secret_key(auth: &ProviderAuth) -> Option<SecretKey> {
        let client_id = match auth {
            ProviderAuth::GmailOAuth { client, .. } => client.client_id.trim(),
            _ => return None,
        };
        if client_id.is_empty() {
            return None;
        }
        let derived = AccountId::derive(uuid::Uuid::NAMESPACE_URL, format!("ferrule:oauth-client:{client_id}").as_bytes());
        Some(SecretKey {
            account_id: derived,
            kind: CredentialKind::OAuthClientSecret,
        })
    }

    /// Resolve a Google Desktop OAuth `client_secret` from explicit input, this account's
    /// keychain slot, the shared `client_id` keychain slot, or another existing account
    /// using the same `client_id` (backfilling the shared slot when found).
    pub fn resolve_oauth_client_secret(
        &self,
        auth: &ProviderAuth,
        explicit: Option<String>,
        account_id: Option<AccountId>,
    ) -> Result<Option<SecretString>, CoreError> {
        if let Some(secret) = explicit.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            return Ok(Some(SecretString::new(secret)));
        }
        if let Some(id) = account_id {
            if let Some(secret) = self.inner.host.secrets.get(&SecretKey {
                account_id: id,
                kind: CredentialKind::OAuthClientSecret,
            })? {
                return Ok(Some(secret));
            }
        }
        let Some(client_key) = Self::oauth_client_secret_key(auth) else {
            return Ok(None);
        };
        if let Some(secret) = self.inner.host.secrets.get(&client_key)? {
            return Ok(Some(secret));
        }
        let target_client_id = match auth {
            ProviderAuth::GmailOAuth { client, .. } => client.client_id.trim().to_string(),
            _ => return Ok(None),
        };
        let existing_ids: Vec<AccountId> = self.inner.store.with_conn(|conn| {
            let views = store::account_views(conn)?;
            let mut matches = Vec::new();
            for view in views {
                if let Some(row) = store::get_account(conn, view.id)? {
                    if let ProviderAuth::GmailOAuth { client, .. } = row.auth {
                        if client.client_id.trim() == target_client_id {
                            matches.push(view.id);
                        }
                    }
                }
            }
            Ok(matches)
        })?;
        for existing_id in existing_ids {
            if let Some(secret) = self.inner.host.secrets.get(&SecretKey {
                account_id: existing_id,
                kind: CredentialKind::OAuthClientSecret,
            })? {
                let _ = self.inner.host.secrets.put(&client_key, &secret);
                return Ok(Some(secret));
            }
        }
        Ok(None)
    }

    fn cache_access_token(&self, account_id: AccountId, token: String, expires_in_secs: i64) {
        let expires_at = self.now().add_secs(expires_in_secs.max(60));
        if let Ok(mut cache) = self.inner.token_cache.lock() {
            cache.insert(
                account_id,
                CachedAccessToken {
                    token: SecretString::new(token),
                    expires_at,
                },
            );
        }
    }

    /// Resolve the active credential for IMAP/SMTP: raw password for Password accounts,
    /// or a fresh (cached or refreshed) OAuth2 access token for OAuth accounts.
    async fn resolve_transport_secret(
        &self,
        account_id: AccountId,
        auth: &ProviderAuth,
    ) -> Result<Option<SecretString>, CoreError> {
        match auth.credential_kind() {
            CredentialKind::Password
            | CredentialKind::StaffSyncToken
            | CredentialKind::DiscordWebhookUrl => self.secret_for(account_id, auth),
            CredentialKind::OAuthRefreshToken | CredentialKind::OAuthClientSecret => {
                let now = self.now();
                if let Ok(cache) = self.inner.token_cache.lock() {
                    if let Some(entry) = cache.get(&account_id) {
                        if entry.expires_at > now.add_secs(60) {
                            return Ok(Some(entry.token.clone()));
                        }
                    }
                }
                let Some(refresh) = self.secret_for(account_id, auth)? else {
                    return Ok(None);
                };
                let client_secret = self.resolve_oauth_client_secret(auth, None, Some(account_id))?;
                let bundle = sync::oauth::refresh_access_token(
                    account_id,
                    auth,
                    refresh.expose(),
                    client_secret.as_ref().map(SecretString::expose),
                )
                .await?;
                if let Some(rotated) = bundle.refresh_token {
                    self.inner.host.secrets.put(
                        &SecretKey {
                            account_id,
                            kind: CredentialKind::OAuthRefreshToken,
                        },
                        &SecretString::new(rotated),
                    )?;
                }
                let access = SecretString::new(bundle.access_token.clone());
                self.cache_access_token(account_id, bundle.access_token, bundle.expires_in_secs);
                Ok(Some(access))
            }
        }
    }

    /// Seed four demo accounts, six threads, timers, and templates when the database
    /// is empty. Returns `false` (and does nothing) when accounts already exist.
    pub async fn seed_demo_if_empty(&self) -> Result<bool, CoreError> {
        let now = self.now();
        let seeded = self
            .inner
            .store
            .with_conn(|conn| store::seed_demo(conn, now))?;
        if seeded {
            self.emit(CoreEvent::Resync);
        }
        Ok(seeded)
    }

    // ---------------------------------------------------------------- threads

    pub async fn list_threads(&self, query: ThreadQuery) -> Result<ThreadPage, CoreError> {
        let now = self.now();
        self.inner
            .store
            .with_conn(|conn| store::list_threads(conn, &query, now))
    }

    pub async fn mailbox_counts(&self, scope: Scope) -> Result<MailboxCounts, CoreError> {
        let now = self.now();
        self.inner
            .store
            .with_conn(|conn| store::mailbox_counts(conn, &scope, now))
    }

    /// Sanitizes every message body on read (no stale sanitized copies), marks the
    /// thread read, and reports trackers + unsubscribe options per message.
    pub async fn open_thread(
        &self,
        thread_id: ThreadId,
        remote_content: RemoteContent,
    ) -> Result<ThreadView, CoreError> {
        let (view, changed, account_id) = self.inner.store.with_conn(|conn| {
            let thread = store::get_thread(conn, thread_id)?.ok_or(CoreError::NotFound {
                entity: "thread".to_string(),
            })?;
            let account =
                store::get_account(conn, thread.account_id)?.ok_or(CoreError::NotFound {
                    entity: "account".to_string(),
                })?;
            let messages = store::load_messages(conn, thread_id)?;
            let changed = store::mark_thread_read(conn, thread_id)?;
            let snoozed_until = store::live_snooze_for_thread(conn, thread_id)?.map(|j| j.due_at);
            let follow_up_due =
                store::live_follow_up_for_thread(conn, thread_id)?.map(|j| j.due_at);

            let mut out = Vec::with_capacity(messages.len());
            for message in &messages {
                let raw = match &message.raw_html {
                    Some(html) => html.clone(),
                    None => sanitize::text_to_html(&message.body_text),
                };
                let doc = sanitize::sanitize_html(&raw, remote_content);
                let invites = store::load_message_invites(conn, message.id)?;
                out.push(MessageView {
                    id: message.id,
                    from_name: message.from_name.clone(),
                    from_email: message.from_email.clone(),
                    to: message.to.clone(),
                    cc: message.cc.clone(),
                    date: message.date,
                    snippet: message.snippet.clone(),
                    body_srcdoc: doc.srcdoc.into_inner(),
                    body_text: message.body_text.clone(),
                    privacy: doc.privacy,
                    attachments: Vec::new(),
                    unsubscribe: message.unsub.clone(),
                    invites,
                });
            }

            Ok((
                ThreadView {
                    id: thread.id,
                    account_id: thread.account_id,
                    account_label: account.display_name,
                    account_color: account.color,
                    account_email: account.email,
                    subject: thread.subject,
                    starred: thread.starred,
                    snoozed_until,
                    follow_up_due,
                    messages: out,
                },
                changed,
                thread.account_id,
            ))
        })?;

        if changed {
            self.emit(CoreEvent::MailChanged {
                account_id,
                thread_ids: vec![thread_id],
            });
        }
        Ok(view)
    }

    pub async fn act_on_threads(
        &self,
        action: MailAction,
        thread_ids: Vec<ThreadId>,
    ) -> Result<ActionOutcome, CoreError> {
        let outcome = self
            .inner
            .store
            .with_conn(|conn| store::apply_mail_action(conn, &action, &thread_ids))?;
        if !outcome.updated_ids.is_empty() {
            let accounts = self
                .inner
                .store
                .with_conn(|conn| store::thread_account_ids(conn, &outcome.updated_ids))?;
            for account_id in accounts {
                self.emit(CoreEvent::MailChanged {
                    account_id,
                    thread_ids: outcome.updated_ids.clone(),
                });
            }
        }
        Ok(outcome)
    }

    // ---------------------------------------------------------------- drafts

    pub async fn create_draft(&self, input: CreateDraftInput) -> Result<DraftView, CoreError> {
        let now = self.now();
        let view = self.inner.store.with_conn(|conn| {
            store::get_account(conn, input.account_id)?.ok_or(CoreError::NotFound {
                entity: "account".to_string(),
            })?;
            let identity =
                store::primary_identity(conn, input.account_id)?.ok_or(CoreError::NotFound {
                    entity: "identity".to_string(),
                })?;
            let mut from_locked = false;
            if let Some(thread_id) = input.reply_to_thread {
                let thread = store::get_thread(conn, thread_id)?.ok_or(CoreError::NotFound {
                    entity: "thread".to_string(),
                })?;
                // From Identity Guard: a reply can never cross account boundaries.
                if thread.account_id != input.account_id {
                    return Err(CoreError::IdentityMismatch);
                }
                from_locked = true;
            }
            let draft_id = store::insert_draft(
                conn,
                &store::InsertDraft {
                    account_id: input.account_id,
                    from_identity_id: identity.id,
                    from_locked,
                    reply_to_thread: input.reply_to_thread,
                    to: input.to,
                    cc: input.cc,
                    bcc: input.bcc,
                    subject: input.subject,
                    body_text: input.body_text,
                    origin: DraftOrigin::Human,
                    state: DraftState::Editing,
                    updated_at: now,
                },
            )?;
            let draft = store::get_draft(conn, draft_id)?.ok_or(CoreError::Internal {
                reason: "draft disappeared after insert".to_string(),
            })?;
            store::draft_view(conn, &draft)
        })?;
        self.emit(CoreEvent::DraftChanged { draft_id: view.id });
        Ok(view)
    }

    pub async fn get_draft(&self, draft_id: DraftId) -> Result<DraftView, CoreError> {
        self.inner.store.with_conn(|conn| {
            let draft = store::get_draft(conn, draft_id)?.ok_or(CoreError::NotFound {
                entity: "draft".to_string(),
            })?;
            store::draft_view(conn, &draft)
        })
    }

    /// Queue a human draft for delivery: freezes the message into a private
    /// [`Sendable`] token and parks an `OutboxState::Scheduled` job on the outbox.
    pub async fn send_draft(
        &self,
        draft_id: DraftId,
        mode: SendMode,
    ) -> Result<SendReceipt, CoreError> {
        self.enqueue_send(draft_id, mode, SendExpect::Editing).await
    }

    /// Human approval for an agent-staged draft (the only way out of
    /// `DraftState::PendingApproval`).
    pub async fn approve_agent_draft(
        &self,
        draft_id: DraftId,
        mode: SendMode,
    ) -> Result<SendReceipt, CoreError> {
        self.enqueue_send(draft_id, mode, SendExpect::PendingApproval)
            .await
    }

    async fn enqueue_send(
        &self,
        draft_id: DraftId,
        mode: SendMode,
        expect: SendExpect,
    ) -> Result<SendReceipt, CoreError> {
        let now = self.now();
        let due = match mode {
            SendMode::Undoable { undo_secs } => now.add_secs(i64::from(undo_secs)),
            SendMode::Scheduled { send_at } => {
                if send_at <= now {
                    return Err(CoreError::InvalidInput {
                        field: "sendAt".to_string(),
                        reason: "scheduled send must be in the future".to_string(),
                    });
                }
                send_at
            }
        };

        let sendable = self.inner.store.with_conn(|conn| {
            let draft = store::get_draft(conn, draft_id)?.ok_or(CoreError::NotFound {
                entity: "draft".to_string(),
            })?;
            match expect {
                SendExpect::Editing => {
                    if !matches!(draft.origin, DraftOrigin::Human) {
                        return Err(CoreError::IllegalState {
                            reason: "agent drafts require human approval".to_string(),
                        });
                    }
                    if !matches!(draft.state, DraftState::Editing) {
                        return Err(CoreError::IllegalState {
                            reason: format!("draft is not editable: {:?}", draft.state),
                        });
                    }
                }
                SendExpect::PendingApproval => {
                    if !matches!(draft.origin, DraftOrigin::Agent { .. }) {
                        return Err(CoreError::IllegalState {
                            reason: "only agent drafts await approval".to_string(),
                        });
                    }
                    if !matches!(draft.state, DraftState::PendingApproval) {
                        return Err(CoreError::IllegalState {
                            reason: format!("draft is not pending approval: {:?}", draft.state),
                        });
                    }
                }
            }

            store::get_account(conn, draft.account_id)?.ok_or(CoreError::NotFound {
                entity: "account".to_string(),
            })?;
            let identity =
                store::identity(conn, draft.from_identity_id)?.ok_or(CoreError::NotFound {
                    entity: "identity".to_string(),
                })?;
            if identity.account_id != draft.account_id {
                return Err(CoreError::IdentityMismatch);
            }
            if draft.to.is_empty() && draft.cc.is_empty() && draft.bcc.is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "to".to_string(),
                    reason: "at least one recipient is required to send".to_string(),
                });
            }

            let (in_reply_to, references) = match draft.reply_to_thread {
                Some(thread_id) => {
                    let thread =
                        store::get_thread(conn, thread_id)?.ok_or(CoreError::NotFound {
                            entity: "thread".to_string(),
                        })?;
                    if thread.account_id != draft.account_id {
                        return Err(CoreError::IdentityMismatch);
                    }
                    let parent = store::last_message(conn, thread_id)?.map(|m| m.message_id_hdr);
                    let references = parent.clone();
                    (parent, references)
                }
                None => (None, None),
            };

            let frozen = freeze_message(
                FreezeInput {
                    from_name: identity.display_name.clone(),
                    from_address: identity.address.clone(),
                    to: draft.to.clone(),
                    cc: draft.cc.clone(),
                    bcc: draft.bcc.clone(),
                    subject: draft.subject.clone(),
                    body_text: draft.body_text.clone(),
                    in_reply_to,
                    references,
                },
                now,
            );
            Ok(Sendable {
                job_id: JobId::fresh(),
                draft_id,
                account_id: draft.account_id,
                reply_to_thread: draft.reply_to_thread,
                summary: format!("Send \u{201c}{}\u{201d}", draft.subject),
                frozen,
            })
        })?;

        let scheduled = matches!(mode, SendMode::Scheduled { .. });
        let frozen_json = store::to_json(&sendable.frozen)?;
        let state_json = store::to_json(&OutboxState::Scheduled { due })?;
        self.inner.store.with_conn(|conn| {
            store::insert_job(
                conn,
                &store::NewJob {
                    id: sendable.job_id,
                    kind: JobKind::Send,
                    account_id: sendable.account_id,
                    thread_id: sendable.reply_to_thread,
                    draft_id: Some(sendable.draft_id),
                    due_at: due,
                    live: true,
                    state_json,
                    summary: sendable.summary.clone(),
                    payload: Some(frozen_json),
                    message_id_hdr: Some(sendable.frozen.message_id.clone()),
                    created_at: now,
                },
            )?;
            store::set_draft_state(
                conn,
                sendable.draft_id,
                &DraftState::Queued {
                    job_id: sendable.job_id,
                },
                now,
            )
        })?;

        self.emit(CoreEvent::JobChanged {
            job: JobView {
                id: sendable.job_id,
                kind: JobKind::Send.as_str().to_string(),
                account_id: sendable.account_id,
                thread_id: sendable.reply_to_thread,
                due_at: due,
                state: "scheduled".to_string(),
                summary: sendable.summary.clone(),
            },
        });
        self.emit(CoreEvent::DraftChanged {
            draft_id: sendable.draft_id,
        });

        Ok(SendReceipt {
            job_id: sendable.job_id,
            draft_id: sendable.draft_id,
            due_at: due,
            scheduled,
        })
    }

    /// Undo Send / cancel Send Later while the job is still cancellable.
    pub async fn cancel_send(&self, job_id: JobId) -> Result<UndoOutcome, CoreError> {
        let now = self.now();
        let (outcome, view, draft_id) = self.inner.store.with_conn(|conn| {
            let job = store::get_job(conn, job_id)?.ok_or(CoreError::NotFound {
                entity: "job".to_string(),
            })?;
            if job.kind != JobKind::Send {
                return Err(CoreError::InvalidInput {
                    field: "jobId".to_string(),
                    reason: "not a send job".to_string(),
                });
            }
            let state: OutboxState = store::from_json(&job.state_json)?;
            if matches!(state, OutboxState::Cancelled { .. }) {
                return Ok((UndoOutcome::Cancelled, job.to_view(), job.draft_id));
            }
            // TooLate for Sending / Verifying / Sent — SMTP already started.
            let (next, _) = step_outbox(state, OutboxEvent::Cancel, now)?;
            store::update_job_state(conn, job_id, &store::to_json(&next)?, job.due_at, false)?;
            if let Some(draft_id) = job.draft_id {
                // Cancelled sends hand the draft back to its resting state: human
                // drafts keep editing, agent drafts go back to awaiting approval.
                let resting_state =
                    match store::get_draft(conn, draft_id)?.map(|draft| draft.origin) {
                        Some(DraftOrigin::Agent { .. }) => DraftState::PendingApproval,
                        _ => DraftState::Editing,
                    };
                store::set_draft_state(conn, draft_id, &resting_state, now)?;
            }
            let mut cancelled = job.clone();
            cancelled.state_json = store::to_json(&next)?;
            cancelled.live = false;
            Ok((UndoOutcome::Cancelled, cancelled.to_view(), job.draft_id))
        })?;
        self.emit(CoreEvent::JobChanged { job: view });
        if let Some(draft_id) = draft_id {
            self.emit(CoreEvent::DraftChanged { draft_id });
        }
        Ok(outcome)
    }

    // ---------------------------------------------------------------- timers

    pub async fn snooze_thread(
        &self,
        thread_id: ThreadId,
        until: UnixMs,
    ) -> Result<JobView, CoreError> {
        let now = self.now();
        if until <= now {
            return Err(CoreError::InvalidInput {
                field: "until".to_string(),
                reason: "snooze time must be in the future".to_string(),
            });
        }
        let view = self
            .inner
            .store
            .with_conn(|conn| {
                let thread = store::get_thread(conn, thread_id)?.ok_or(CoreError::NotFound {
                    entity: "thread".to_string(),
                })?;
                if let Some(job) = store::live_snooze_for_thread(conn, thread_id)? {
                    let state: SnoozeState = store::from_json(&job.state_json)?;
                    let (next, _) = step_snooze(state, SnoozeEvent::Reschedule(until), now)?;
                    store::update_job_state(conn, job.id, &store::to_json(&next)?, until, true)?;
                    Ok(store::get_job(conn, job.id)?.map(|j| j.to_view()))
                } else {
                    let job_id = JobId::fresh();
                    store::insert_job(
                        conn,
                        &store::NewJob {
                            id: job_id,
                            kind: JobKind::Snooze,
                            account_id: thread.account_id,
                            thread_id: Some(thread_id),
                            draft_id: None,
                            due_at: until,
                            live: true,
                            state_json: store::to_json(&SnoozeState::Armed { until })?,
                            summary: "Snoozed".to_string(),
                            payload: None,
                            message_id_hdr: None,
                            created_at: now,
                        },
                    )?;
                    Ok(store::get_job(conn, job_id)?.map(|j| j.to_view()))
                }
            })?
            .ok_or_else(|| CoreError::Internal {
                reason: "snooze job disappeared".to_string(),
            })?;
        self.emit(CoreEvent::JobChanged { job: view.clone() });
        Ok(view)
    }

    pub async fn unsnooze_thread(&self, thread_id: ThreadId) -> Result<(), CoreError> {
        let now = self.now();
        let job = self
            .inner
            .store
            .with_conn(|conn| store::live_snooze_for_thread(conn, thread_id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "snooze job".to_string(),
            })?;
        let state: SnoozeState = store::from_json(&job.state_json)?;
        let (next, _) = step_snooze(state, SnoozeEvent::Cancel, now)?;
        self.inner.store.with_conn(|conn| {
            store::update_job_state(conn, job.id, &store::to_json(&next)?, job.due_at, false)
        })?;
        self.emit(CoreEvent::JobChanged { job: job.to_view() });
        Ok(())
    }

    pub async fn set_follow_up(
        &self,
        thread_id: ThreadId,
        due_at: UnixMs,
    ) -> Result<JobView, CoreError> {
        let now = self.now();
        if due_at <= now {
            return Err(CoreError::InvalidInput {
                field: "dueAt".to_string(),
                reason: "follow-up must be in the future".to_string(),
            });
        }
        let view = self
            .inner
            .store
            .with_conn(|conn| {
                let thread = store::get_thread(conn, thread_id)?.ok_or(CoreError::NotFound {
                    entity: "thread".to_string(),
                })?;
                if let Some(job) = store::live_follow_up_for_thread(conn, thread_id)? {
                    let state: FollowUpState = store::from_json(&job.state_json)?;
                    let (next, _) = step_follow_up(state, FollowUpEvent::Set(due_at), now)?;
                    store::update_job_state(conn, job.id, &store::to_json(&next)?, due_at, true)?;
                    Ok(store::get_job(conn, job.id)?.map(|j| j.to_view()))
                } else {
                    let job_id = JobId::fresh();
                    store::insert_job(
                        conn,
                        &store::NewJob {
                            id: job_id,
                            kind: JobKind::FollowUp,
                            account_id: thread.account_id,
                            thread_id: Some(thread_id),
                            draft_id: None,
                            due_at,
                            live: true,
                            state_json: store::to_json(&FollowUpState::Armed { due: due_at })?,
                            summary: "Follow up".to_string(),
                            payload: None,
                            message_id_hdr: None,
                            created_at: now,
                        },
                    )?;
                    Ok(store::get_job(conn, job_id)?.map(|j| j.to_view()))
                }
            })?
            .ok_or_else(|| CoreError::Internal {
                reason: "follow-up job disappeared".to_string(),
            })?;
        self.emit(CoreEvent::JobChanged { job: view.clone() });
        Ok(view)
    }

    pub async fn dismiss_follow_up(&self, job_id: JobId) -> Result<(), CoreError> {
        let now = self.now();
        let job = self
            .inner
            .store
            .with_conn(|conn| store::get_job(conn, job_id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "job".to_string(),
            })?;
        if job.kind != JobKind::FollowUp {
            return Err(CoreError::InvalidInput {
                field: "jobId".to_string(),
                reason: "not a follow-up job".to_string(),
            });
        }
        if !job.live {
            return Ok(()); // already dismissed — idempotent
        }
        let state: FollowUpState = store::from_json(&job.state_json)?;
        let (next, _) = step_follow_up(state, FollowUpEvent::Dismiss, now)?;
        self.inner.store.with_conn(|conn| {
            store::update_job_state(conn, job.id, &store::to_json(&next)?, job.due_at, false)
        })?;
        self.emit(CoreEvent::JobChanged { job: job.to_view() });
        Ok(())
    }

    pub async fn list_jobs(&self) -> Result<Vec<JobView>, CoreError> {
        let jobs = self.inner.store.with_conn(store::list_live_jobs)?;
        Ok(jobs.iter().map(|job| job.to_view()).collect())
    }

    // ---------------------------------------------------------------- templates

    pub async fn list_templates(&self) -> Result<Vec<TemplateView>, CoreError> {
        self.inner.store.with_conn(store::list_templates)
    }

    pub async fn save_template(&self, input: SaveTemplateInput) -> Result<TemplateView, CoreError> {
        self.inner
            .store
            .with_conn(|conn| store::save_template(conn, &input))
    }

    pub async fn delete_template(&self, template_id: TemplateId) -> Result<(), CoreError> {
        let deleted = self
            .inner
            .store
            .with_conn(|conn| store::delete_template(conn, template_id))?;
        if !deleted {
            return Err(CoreError::NotFound {
                entity: "template".to_string(),
            });
        }
        Ok(())
    }

    // ---------------------------------------------------------------- scheduler

    /// Fire every due timer exactly once: snoozes wake threads, follow-ups raise
    /// reminders, and outbox jobs attempt delivery with crash-safe state transitions.
    pub async fn tick_scheduler(&self) -> Result<Vec<JobView>, CoreError> {
        let now = self.now();
        let due = self
            .inner
            .store
            .with_conn(|conn| store::due_jobs(conn, now))?;
        let mut fired = Vec::new();

        for mut job in due {
            match job.kind {
                JobKind::Snooze => {
                    let state: SnoozeState = store::from_json(&job.state_json)?;
                    match step_snooze(state, SnoozeEvent::Fire, now) {
                        Ok((next, effect)) => {
                            self.inner.store.with_conn(|conn| {
                                store::update_job_state(
                                    conn,
                                    job.id,
                                    &store::to_json(&next)?,
                                    job.due_at,
                                    false,
                                )
                            })?;
                            if effect.is_some() {
                                if let Some(thread_id) = job.thread_id {
                                    self.emit(CoreEvent::Attention {
                                        alert: Alert::SnoozeWoke { thread_id },
                                    });
                                }
                            }
                            job.state_json = store::to_json(&next)?;
                            job.live = false;
                            fired.push(job.to_view());
                        }
                        Err(CoreError::IllegalState { .. }) => continue,
                        Err(err) => return Err(err),
                    }
                }
                JobKind::FollowUp => {
                    let state: FollowUpState = store::from_json(&job.state_json)?;
                    match step_follow_up(state.clone(), FollowUpEvent::Fire, now) {
                        Ok((next, effect)) if next != state => {
                            // Fired reminders stay live so they remain in Follow-ups
                            // until dismissed or satisfied.
                            self.inner.store.with_conn(|conn| {
                                store::update_job_state(
                                    conn,
                                    job.id,
                                    &store::to_json(&next)?,
                                    job.due_at,
                                    true,
                                )
                            })?;
                            if effect.is_some() {
                                if let Some(thread_id) = job.thread_id {
                                    self.emit(CoreEvent::Attention {
                                        alert: Alert::FollowUpDue { thread_id },
                                    });
                                }
                            }
                            job.state_json = store::to_json(&next)?;
                            fired.push(job.to_view());
                        }
                        Ok(_) => continue,
                        Err(CoreError::IllegalState { .. }) => continue,
                        Err(err) => return Err(err),
                    }
                }
                JobKind::Send => {
                    if let Some(view) = self.tick_send_job(&job, now).await? {
                        fired.push(view);
                    }
                }
            }
        }
        Ok(fired)
    }

    async fn tick_send_job(
        &self,
        job: &store::JobRow,
        now: UnixMs,
    ) -> Result<Option<JobView>, CoreError> {
        let state: OutboxState = store::from_json(&job.state_json)?;
        match &state {
            OutboxState::Scheduled { due } | OutboxState::RetryWait { due, .. } if *due <= now => {
                let (next, effect) = step_outbox(state.clone(), OutboxEvent::Due, now)?;
                debug_assert_eq!(effect, Some(OutboxEffect::StartSmtp));
                // Persist the Sending lease BEFORE touching the network: a crash from
                // here recovers through Verifying, never through a blind resend.
                self.inner.store.with_conn(|conn| {
                    store::update_job_state(conn, job.id, &store::to_json(&next)?, job.due_at, true)
                })?;

                let account = self.account_row(job.account_id)?;
                let Some(secret) = self
                    .resolve_transport_secret(job.account_id, &account.auth)
                    .await?
                else {
                    return self
                        .fail_send(
                            job,
                            next,
                            OutboxEvent::SmtpPermanent {
                                reason: "account requires re-authentication".to_string(),
                            },
                            now,
                        )
                        .map(Some);
                };

                let payload = job.payload.as_deref().ok_or_else(|| CoreError::Internal {
                    reason: "send job has no frozen payload".to_string(),
                })?;
                let frozen: FrozenMessage = store::from_json(payload)?;
                let outcome = send_frozen(
                    &account.auth,
                    &account.auth.smtp_endpoint(),
                    account.auth.username(),
                    secret.expose(),
                    &frozen,
                )
                .await;

                match outcome {
                    SmtpOutcome::Accepted => self
                        .finalize_send(job, &frozen, OutboxState::Sent { at: now }, now)
                        .map(Some),
                    SmtpOutcome::Transient(reason) => self
                        .fail_send(job, next, OutboxEvent::SmtpTransient { reason }, now)
                        .map(Some),
                    SmtpOutcome::Permanent(reason) => self
                        .fail_send(job, next, OutboxEvent::SmtpPermanent { reason }, now)
                        .map(Some),
                }
            }
            OutboxState::Sending { lease_until, .. } if *lease_until <= now => {
                // Crash recovery: expired lease → Verifying + Sent-folder search.
                let (next, effect) = step_outbox(state.clone(), OutboxEvent::LeaseExpired, now)?;
                debug_assert_eq!(effect, Some(OutboxEffect::SearchSent));
                let verify_due = now.add_secs(30);
                self.inner.store.with_conn(|conn| {
                    store::update_job_state(conn, job.id, &store::to_json(&next)?, verify_due, true)
                })?;

                let payload = job.payload.as_deref().ok_or_else(|| CoreError::Internal {
                    reason: "send job has no frozen payload".to_string(),
                })?;
                let frozen: FrozenMessage = store::from_json(payload)?;
                let verification = self.verify_sent(job, &frozen).await;

                match verification {
                    Ok(true) => self
                        .finalize_send(job, &frozen, OutboxState::Sent { at: now }, now)
                        .map(Some),
                    Ok(false) => {
                        let (retry, _) = step_outbox(next, OutboxEvent::NotInSent, now)?;
                        let due = match &retry {
                            OutboxState::RetryWait { due, .. } => *due,
                            _ => now,
                        };
                        self.inner.store.with_conn(|conn| {
                            store::update_job_state(
                                conn,
                                job.id,
                                &store::to_json(&retry)?,
                                due,
                                true,
                            )
                        })?;
                        let mut updated = job.clone();
                        updated.state_json = store::to_json(&retry)?;
                        updated.due_at = due;
                        Ok(Some(updated.to_view()))
                    }
                    // Unverifiable (offline / auth): stay in Verifying — resending
                    // here is the only way to double-deliver, so we never do.
                    Err(err) => {
                        if matches!(err, CoreError::AuthRequired { .. }) {
                            self.emit(CoreEvent::SyncChanged {
                                account_id: job.account_id,
                                state: SyncState::Blocked {
                                    reason: "account requires re-authentication".to_string(),
                                },
                            });
                        }
                        let mut updated = job.clone();
                        updated.state_json = store::to_json(&next)?;
                        updated.due_at = verify_due;
                        Ok(Some(updated.to_view()))
                    }
                }
            }
            _ => Ok(None),
        }
    }

    fn account_row(&self, account_id: AccountId) -> Result<store::AccountRow, CoreError> {
        self.inner
            .store
            .with_conn(|conn| store::get_account(conn, account_id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "account".to_string(),
            })
    }

    fn secret_for(
        &self,
        account_id: AccountId,
        auth: &ProviderAuth,
    ) -> Result<Option<SecretString>, CoreError> {
        let key = SecretKey {
            account_id,
            kind: auth.credential_kind(),
        };
        self.inner.host.secrets.get(&key)
    }

    fn fail_send(
        &self,
        job: &store::JobRow,
        current: OutboxState,
        event: OutboxEvent,
        now: UnixMs,
    ) -> Result<JobView, CoreError> {
        let (next, effect) = step_outbox(current, event, now)?;
        let live = matches!(
            next,
            OutboxState::Scheduled { .. } | OutboxState::RetryWait { .. }
        );
        let due = match &next {
            OutboxState::Scheduled { due } | OutboxState::RetryWait { due, .. } => *due,
            _ => job.due_at,
        };
        let failed = matches!(next, OutboxState::Failed { .. });
        let reason = match &next {
            OutboxState::Failed { reason } => reason.clone(),
            _ => String::new(),
        };

        self.inner.store.with_conn(|conn| {
            store::update_job_state(conn, job.id, &store::to_json(&next)?, due, live)?;
            if failed {
                if let Some(draft_id) = job.draft_id {
                    store::set_draft_state(
                        conn,
                        draft_id,
                        &DraftState::Rejected {
                            reason: reason.clone(),
                        },
                        now,
                    )?;
                }
            }
            Ok(())
        })?;

        if effect == Some(OutboxEffect::NotifyFailed) {
            self.emit(CoreEvent::Attention {
                alert: Alert::SendFailed {
                    job_id: job.id,
                    reason,
                },
            });
        }
        if let Some(draft_id) = job.draft_id {
            self.emit(CoreEvent::DraftChanged { draft_id });
        }
        let mut updated = job.clone();
        updated.state_json = store::to_json(&next)?;
        updated.due_at = due;
        updated.live = live;
        Ok(updated.to_view())
    }

    /// Deliver the final state of a send: insert the sent message (reply or brand-new
    /// thread), satisfy follow-ups, mark draft + job done. Idempotent on re-entry.
    fn finalize_send(
        &self,
        job: &store::JobRow,
        frozen: &FrozenMessage,
        next: OutboxState,
        at: UnixMs,
    ) -> Result<JobView, CoreError> {
        let snippet = crate::sync::snippet_of(&frozen.body_text);
        let (view, thread_id) = self.inner.store.with_conn(|conn| {
            let thread_id = match job.thread_id {
                Some(thread_id) => {
                    store::insert_message(
                        conn,
                        &store::NewMessage {
                            id: crate::ids::MessageId::fresh(),
                            account_id: job.account_id,
                            thread_id,
                            message_id_hdr: frozen.message_id.clone(),
                            in_reply_to: frozen.in_reply_to.clone(),
                            from_name: frozen.from_name.clone(),
                            from_email: frozen.from_address.clone(),
                            to: frozen.to.clone(),
                            cc: frozen.cc.clone(),
                            subject: frozen.subject.clone(),
                            date: at,
                            snippet: snippet.clone(),
                            is_read: true,
                            body_text: frozen.body_text.clone(),
                            raw_html: None,
                            unsub: None,
                            trackers: 0,
                        },
                    )?;
                    if let Some(follow_up) = store::live_follow_up_for_thread(conn, thread_id)? {
                        let state: FollowUpState = store::from_json(&follow_up.state_json)?;
                        if let Ok((satisfied, _)) = step_follow_up(
                            state,
                            FollowUpEvent::Satisfy {
                                by: crate::ids::MessageId::fresh(),
                            },
                            at,
                        ) {
                            store::update_job_state(
                                conn,
                                follow_up.id,
                                &store::to_json(&satisfied)?,
                                follow_up.due_at,
                                false,
                            )?;
                        }
                    }
                    thread_id
                }
                None => {
                    let thread_id = ThreadId::fresh();
                    store::upsert_thread(
                        conn,
                        &store::NewThread {
                            id: thread_id,
                            account_id: job.account_id,
                            remote_key: &frozen.message_id,
                            subject: &frozen.subject,
                            snippet: &snippet,
                            sender_name: frozen.from_name.as_deref().unwrap_or(""),
                            sender_email: &frozen.from_address,
                            last_at: at,
                            category: InboxCategory::Primary,
                        },
                    )?;
                    store::insert_message(
                        conn,
                        &store::NewMessage {
                            id: crate::ids::MessageId::fresh(),
                            account_id: job.account_id,
                            thread_id,
                            message_id_hdr: frozen.message_id.clone(),
                            in_reply_to: frozen.in_reply_to.clone(),
                            from_name: frozen.from_name.clone(),
                            from_email: frozen.from_address.clone(),
                            to: frozen.to.clone(),
                            cc: frozen.cc.clone(),
                            subject: frozen.subject.clone(),
                            date: at,
                            snippet: snippet.clone(),
                            is_read: true,
                            body_text: frozen.body_text.clone(),
                            raw_html: None,
                            unsub: None,
                            trackers: 0,
                        },
                    )?;
                    thread_id
                }
            };

            if let Some(draft_id) = job.draft_id {
                store::set_draft_state(conn, draft_id, &DraftState::Sent { at }, at)?;
            }
            store::update_job_state(conn, job.id, &store::to_json(&next)?, job.due_at, false)?;
            let mut finished = job.clone();
            finished.state_json = store::to_json(&next)?;
            finished.live = false;
            Ok((finished.to_view(), thread_id))
        })?;

        self.emit(CoreEvent::MailChanged {
            account_id: job.account_id,
            thread_ids: vec![thread_id],
        });
        if let Some(draft_id) = job.draft_id {
            self.emit(CoreEvent::DraftChanged { draft_id });
        }
        self.emit(CoreEvent::JobChanged { job: view.clone() });
        Ok(view)
    }

    async fn verify_sent(
        &self,
        job: &store::JobRow,
        frozen: &FrozenMessage,
    ) -> Result<bool, CoreError> {
        let account = self.account_row(job.account_id)?;
        let Some(secret) = self
            .resolve_transport_secret(job.account_id, &account.auth)
            .await?
        else {
            return Err(CoreError::AuthRequired {
                account_id: job.account_id,
                reason: "no saved credential in OS keychain".to_string(),
            });
        };
        let plan = sync::imap::plan_for(
            job.account_id,
            &account.auth,
            secret.expose().to_string(),
            Dialect::from_auth(&account.auth)
                .primary_sync_folder()
                .to_string(),
            sync::imap::SyncWindow {
                since: job.due_at,
                now: self.now(),
                min_uid: 0,
            },
        );
        let message_id = job
            .message_id_hdr
            .clone()
            .unwrap_or_else(|| frozen.message_id.clone());
        tokio::task::spawn_blocking(move || sync::imap::verify_in_sent(&plan, &message_id))
            .await
            .map_err(|e| CoreError::Internal {
                reason: format!("verify task failed: {e}"),
            })?
    }

    // ---------------------------------------------------------------- sync

    /// Real IMAP sync: fetch the 90-day window over TLS, ingest into SQLite + FTS5,
    /// and surface `AuthRequired` / `Network` cleanly (state persisted, no crash).
    pub async fn sync_account(&self, account_id: AccountId) -> Result<AccountView, CoreError> {
        let now = self.now();
        let window_days = self.inner.config.sync_window_days;

        let (auth, folder, since, min_uid) = self.inner.store.with_conn(|conn| {
            let account = store::get_account(conn, account_id)?.ok_or(CoreError::NotFound {
                entity: "account".to_string(),
            })?;
            let folder = Dialect::from_auth(&account.auth)
                .primary_sync_folder()
                .to_string();
            let cursor = store::folder_cursor(conn, account_id, &folder)?;
            let since = cursor
                .as_ref()
                .map(|c| c.window_floor)
                .unwrap_or_else(|| now.sub_days(window_days));
            let min_uid = match &cursor {
                Some(cursor) if cursor.uid_validity > 0 => cursor.high_uid,
                _ => 0,
            };
            Ok((account.auth, folder, since, min_uid))
        })?;

        let secret = match self.resolve_transport_secret(account_id, &auth).await {
            Ok(Some(secret)) => secret,
            Ok(None) => {
                let err = CoreError::AuthRequired {
                    account_id,
                    reason: "no saved credential in OS keychain".to_string(),
                };
                self.persist_sync_failure(account_id, &err, now)?;
                return Err(err);
            }
            Err(err) => {
                self.persist_sync_failure(account_id, &err, now)?;
                return Err(err);
            }
        };

        self.inner.store.with_conn(|conn| {
            store::set_sync_state(
                conn,
                account_id,
                &SyncState::Syncing {
                    done: 0,
                    total: None,
                },
            )
        })?;
        self.emit(CoreEvent::SyncChanged {
            account_id,
            state: SyncState::Syncing {
                done: 0,
                total: None,
            },
        });

        let plan = sync::imap::plan_for(
            account_id,
            &auth,
            secret.expose().to_string(),
            folder,
            sync::imap::SyncWindow {
                since,
                now,
                min_uid,
            },
        );
        let fetched = tokio::task::spawn_blocking(move || sync::imap::fetch_window(&plan))
            .await
            .map_err(|e| CoreError::Internal {
                reason: format!("sync task failed: {e}"),
            })?;

        match fetched {
            Ok(window) => {
                let touched = self.inner.store.with_conn(|conn| {
                    let touched = store::ingest_messages(conn, account_id, window.messages, now)?;
                    store::update_folder_high_uid(
                        conn,
                        account_id,
                        &plan_folder(&auth),
                        window.uid_validity,
                        window.high_uid,
                    )?;
                    store::set_sync_state(
                        conn,
                        account_id,
                        &SyncState::Idle { last_ok: Some(now) },
                    )?;
                    Ok(touched)
                })?;
                self.emit(CoreEvent::SyncChanged {
                    account_id,
                    state: SyncState::Idle { last_ok: Some(now) },
                });
                if !touched.is_empty() {
                    self.emit(CoreEvent::MailChanged {
                        account_id,
                        thread_ids: touched,
                    });
                }
                self.account_view(account_id)?
                    .ok_or_else(|| CoreError::NotFound {
                        entity: "account".to_string(),
                    })
            }
            Err(err) => {
                self.persist_sync_failure(account_id, &err, now)?;
                Err(err)
            }
        }
    }

    fn persist_sync_failure(
        &self,
        account_id: AccountId,
        err: &CoreError,
        now: UnixMs,
    ) -> Result<(), CoreError> {
        let state = match err {
            CoreError::AuthRequired { reason, .. } => SyncState::Blocked {
                reason: reason.clone(),
            },
            CoreError::Network { reason, .. } => SyncState::Backoff {
                retry_at: now.add_secs(30),
                attempt: 1,
                reason: Some(reason.clone()),
            },
            other => SyncState::Blocked {
                reason: other.to_string(),
            },
        };
        self.inner
            .store
            .with_conn(|conn| store::set_sync_state(conn, account_id, &state))?;
        self.emit(CoreEvent::SyncChanged {
            account_id,
            state: state.clone(),
        });
        Ok(())
    }

    // ---------------------------------------------------------------- agent

    /// Read-only plain-text rendering for [`AgentPort`] — no HTML ever crosses.
    pub(crate) async fn read_thread_plain(&self, thread_id: ThreadId) -> Result<String, CoreError> {
        self.inner.store.with_conn(|conn| {
            let thread = store::get_thread(conn, thread_id)?.ok_or(CoreError::NotFound {
                entity: "thread".to_string(),
            })?;
            let messages = store::load_messages(conn, thread_id)?;
            let mut out = format!("Subject: {}\n\n", thread.subject);
            for message in &messages {
                let from = match &message.from_name {
                    Some(name) => format!("{name} <{}>", message.from_email),
                    None => message.from_email.clone(),
                };
                let date = chrono::DateTime::from_timestamp_millis(message.date.0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| message.date.0.to_string());
                out.push_str(&format!("[{date}] {from}\n{}\n\n", message.body_text));
            }
            Ok(out)
        })
    }

    /// Stage an agent draft in `PendingApproval`. There is deliberately no send path
    /// here; only [`MailCore::approve_agent_draft`] (human) can queue it.
    pub(crate) async fn stage_agent_draft(
        &self,
        agent_id: &str,
        thread_id: ThreadId,
        body_text: String,
        rationale: String,
    ) -> Result<DraftView, CoreError> {
        let now = self.now();
        let view = self.inner.store.with_conn(|conn| {
            let thread = store::get_thread(conn, thread_id)?.ok_or(CoreError::NotFound {
                entity: "thread".to_string(),
            })?;
            store::get_account(conn, thread.account_id)?.ok_or(CoreError::NotFound {
                entity: "account".to_string(),
            })?;
            let identity =
                store::primary_identity(conn, thread.account_id)?.ok_or(CoreError::NotFound {
                    entity: "identity".to_string(),
                })?;

            // Reply to the latest message that is not from our own identity.
            let messages = store::load_messages(conn, thread_id)?;
            let to = messages
                .iter()
                .rev()
                .find(|m| !m.from_email.eq_ignore_ascii_case(&identity.address))
                .map(|m| vec![m.from_email.clone()])
                .or_else(|| messages.last().map(|m| vec![m.from_email.clone()]))
                .unwrap_or_default();

            let subject = if thread.subject.to_lowercase().starts_with("re:") {
                thread.subject.clone()
            } else {
                format!("Re: {}", thread.subject)
            };

            let draft_id = store::insert_draft(
                conn,
                &store::InsertDraft {
                    account_id: thread.account_id,
                    from_identity_id: identity.id,
                    from_locked: true,
                    reply_to_thread: Some(thread_id),
                    to,
                    cc: Vec::new(),
                    bcc: Vec::new(),
                    subject,
                    body_text,
                    origin: DraftOrigin::Agent {
                        agent_id: agent_id.to_string(),
                        rationale,
                    },
                    state: DraftState::PendingApproval,
                    updated_at: now,
                },
            )?;
            let draft = store::get_draft(conn, draft_id)?.ok_or(CoreError::Internal {
                reason: "draft disappeared after insert".to_string(),
            })?;
            store::draft_view(conn, &draft)
        })?;

        self.emit(CoreEvent::DraftChanged { draft_id: view.id });
        self.emit(CoreEvent::Attention {
            alert: Alert::ApprovalRequested { draft_id: view.id },
        });
        Ok(view)
    }
}

fn default_space_name(email: &str, existing: &[AccountView]) -> String {
    let trimmed = email.trim();
    let local = trimmed
        .split('@')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed);
    let collision = existing
        .iter()
        .any(|acct| acct.display_name.eq_ignore_ascii_case(local));
    if !collision {
        return local.to_string();
    }
    let domain_label = trimmed
        .split('@')
        .nth(1)
        .and_then(|d| d.split('.').next())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match domain_label {
        Some(domain) => format!("{local} ({domain})"),
        None => trimmed.to_string(),
    }
}

fn plan_folder(auth: &ProviderAuth) -> String {
    Dialect::from_auth(auth).primary_sync_folder().to_string()
}
