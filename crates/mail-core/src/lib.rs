pub mod agent;
pub mod ai;
pub mod auth;
pub mod calendar;
pub mod error;
pub mod events;
pub mod host;
pub mod ids;
pub mod mcp;
pub mod rules;
pub mod sanitize;
pub mod schedule;
pub mod search;
pub mod types;

mod calendar_core;
mod store;
mod sync;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::agent::{
    action_for_destination, build_agent_query, frame_untrusted_email_content,
    mailbox_view_for_folder, AgentCreateDraftInput, AgentMoveDestination, AgentPort,
    AgentUpdateDraftInput,
};
use crate::auth::{
    BuiltInOAuthDefaults, CredentialKind, OAuthClient, ProviderAuth, ProviderKind, SecretKey,
    SecretStore, SecretString,
};
use crate::error::CoreError;
use crate::events::{Alert, CoreEvent, HermesPushEvent};
use crate::host::{Clock, OsKeychain, SystemClock};
use crate::ids::{
    AccountId, AgentActivityId, AgentId, AttachmentId, DraftId, JobId, LabelId, MessageId, RuleId,
    SavedSearchId, SignatureId, TemplateId, ThreadId, UnixMs,
};
use crate::schedule::{
    step_follow_up, step_outbox, step_snooze, FollowUpEvent, FollowUpState, OutboxEffect,
    OutboxEvent, OutboxState, SnoozeEvent, SnoozeState,
};
use crate::store::JobKind;
use crate::sync::actor::{ActorCommand, SyncManager};
use crate::sync::smtp::{
    freeze_message, send_frozen, FreezeInput, FrozenAttachment, FrozenMessage, SmtpOutcome,
};
use crate::sync::Dialect;
use crate::types::*;

#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub db_path: Option<PathBuf>,
    pub sync_window_days: i64,
    pub default_undo_secs: u32,
    pub oauth_defaults: BuiltInOAuthDefaults,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            db_path: None,
            sync_window_days: 90,
            default_undo_secs: 7,
            oauth_defaults: BuiltInOAuthDefaults::from_compile_env(),
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
    hermes_events: broadcast::Sender<HermesPushEvent>,
    token_cache: Mutex<HashMap<AccountId, CachedAccessToken>>,
    sync_manager: SyncManager,
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
        let (hermes_events, _) = broadcast::channel(128);
        Ok(Self {
            inner: Arc::new(MailCoreInner {
                config,
                store,
                host,
                events,
                hermes_events,
                token_cache: Mutex::new(HashMap::new()),
                sync_manager: SyncManager::new(),
            }),
        })
    }

    /// Start background per-account async IMAP sync actors and the dedicated durable
    /// SMTP submission worker. Call once on desktop startup.
    pub fn start_background_services(&self) {
        self.inner.sync_manager.start_smtp_worker(self.clone());
        if let Ok(accounts) = self.inner.store.with_conn(store::list_account_rows) {
            for account in accounts {
                self.inner.sync_manager.ensure_actor(self.clone(), account.id);
            }
        }
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

    pub fn subscribe_hermes_events(&self) -> broadcast::Receiver<HermesPushEvent> {
        self.inner.hermes_events.subscribe()
    }

    /// Emit a safe metadata-only Hermes push event if the owning account has agent
    /// read access enabled.
    pub fn emit_hermes_event(&self, event: HermesPushEvent) {
        let allowed = self
            .inner
            .store
            .with_conn(|conn| store::get_account_permission(conn, event.account_id()))
            .map(|p| p.enabled && p.allow_read)
            .unwrap_or(false);
        if allowed {
            let _ = self.inner.hermes_events.send(event);
        }
    }

    /// Emit `new_message` and `new_invite` push events for newly ingested threads.
    pub(crate) fn notify_hermes_ingested(&self, account_id: AccountId, thread_ids: &[ThreadId]) {
        if thread_ids.is_empty() {
            return;
        }
        let Ok(perm) = self
            .inner
            .store
            .with_conn(|conn| store::get_account_permission(conn, account_id))
        else {
            return;
        };
        if !perm.enabled || !perm.allow_read {
            return;
        }
        let events: Vec<HermesPushEvent> = self
            .inner
            .store
            .with_conn(|conn| {
                let mut out = Vec::new();
                for &thread_id in thread_ids {
                    let Some(thread) = store::get_thread(conn, thread_id)? else {
                        continue;
                    };
                    let Some(msg) = store::last_message(conn, thread_id)? else {
                        continue;
                    };
                    let folder = store::get_single_message(conn, msg.id)?
                        .map(|m| m.folder_name)
                        .unwrap_or_else(|| "INBOX".to_string());
                    if !perm.is_folder_allowed(&folder) {
                        continue;
                    }
                    out.push(HermesPushEvent::NewMessage {
                        account_id,
                        thread_id,
                        message_id: msg.id,
                        from_email: msg.from_email.clone(),
                        from_name: msg.from_name.clone(),
                        subject: thread.subject.clone(),
                        date: msg.date,
                        folder,
                    });
                    for invite in store::load_message_invites(conn, msg.id)? {
                        out.push(HermesPushEvent::NewInvite {
                            account_id,
                            thread_id,
                            message_id: msg.id,
                            uid: invite.uid,
                            summary: invite.summary,
                            organizer_email: invite.organizer_email,
                            start_at: invite.start_at,
                            end_at: invite.end_at,
                        });
                    }
                }
                Ok(out)
            })
            .unwrap_or_default();

        for ev in events {
            let _ = self.inner.hermes_events.send(ev);
        }
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

    /// Open an external `https://`, `http://`, or `mailto:` URL in the OS default handler.
    pub fn open_external_url(&self, raw: &str) -> Result<(), CoreError> {
        let trimmed = raw.trim();
        let lower = trimmed.to_ascii_lowercase();
        let allowed = lower.starts_with("https://")
            || lower.starts_with("http://")
            || lower.starts_with("mailto:");
        if !allowed {
            return Err(CoreError::InvalidInput {
                field: "url".to_string(),
                reason: "only https://, http://, and mailto: URLs may be opened".to_string(),
            });
        }
        open::that_detached(trimmed).map_err(|e| CoreError::Internal {
            reason: format!("failed to open url: {e}"),
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

    /// Synchronous check used at desktop launch to decide whether to show the
    /// first-boot-only Launch Splash before Onboarding.
    pub fn needs_first_run_setup(&self) -> Result<bool, CoreError> {
        self.inner.store.with_conn(|conn| {
            let settings = store::get_app_settings(conn)?;
            if settings.onboarding_completed {
                return Ok(false);
            }
            Ok(store::account_count(conn)? == 0)
        })
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

        self.inner.sync_manager.stop_actor(account_id);
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
            store::update_account_meta(conn, input.account_id, display_name, input.color)
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

    /// Return previously configured OAuth client IDs (from existing accounts or
    /// build-time `FERRULE_*` defaults) so adding a Gmail or Outlook account can
    /// launch browser sign-in in one click.
    pub async fn saved_oauth_clients(&self) -> Result<SavedOAuthClients, CoreError> {
        let mut clients = self.inner.store.with_conn(|conn| {
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
        })?;

        let defaults = &self.inner.config.oauth_defaults;
        if let Some(google_id) = defaults
            .google_client_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            clients.gmail_built_in = true;
            if clients.gmail_client_id.is_none() {
                clients.gmail_client_id = Some(google_id.to_string());
            }
        }
        if let Some(outlook_id) = defaults
            .outlook_client_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            clients.outlook_built_in = true;
            if clients.outlook_client_id.is_none() {
                clients.outlook_client_id = Some(outlook_id.to_string());
                if clients.outlook_tenant.is_none() {
                    let tenant = defaults
                        .outlook_tenant
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or("common");
                    clients.outlook_tenant = Some(tenant.to_string());
                }
            }
        }
        Ok(clients)
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
        self.inner
            .sync_manager
            .send_command(account_id, ActorCommand::Reauthed);
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
        let defaults = &self.inner.config.oauth_defaults;
        if defaults
            .google_client_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            == Some(target_client_id.as_str())
        {
            if let Some(secret) = defaults.google_client_secret.as_ref() {
                if !secret.expose().trim().is_empty() {
                    return Ok(Some(secret.clone()));
                }
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
            CredentialKind::Password | CredentialKind::AiApiKey | CredentialKind::WebhookSecret => {
                self.secret_for(account_id, auth)
            }
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
        let (view, changed, account_id, headers_only_ids) = self.inner.store.with_conn(|conn| {
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
            let mut headers_only_ids = Vec::new();
            for message in &messages {
                if message.body_state == "headers_only" {
                    headers_only_ids.push(message.id);
                }
                let att_rows = store::list_message_attachments(conn, message.id)?;
                let raw = match &message.raw_html {
                    Some(html) => sync::inline_cid_images(html, &att_rows),
                    None => sanitize::text_to_html(&message.body_text),
                };
                let doc = sanitize::sanitize_html(&raw, remote_content);
                let invites = store::load_message_invites(conn, message.id)?;
                let attachments: Vec<AttachmentMeta> = att_rows
                    .iter()
                    .filter(|a| !a.is_inline || a.content_id.is_none())
                    .map(store::AttachmentRow::to_meta)
                    .collect();
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
                    attachments,
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
                headers_only_ids,
            ))
        })?;

        for mid in headers_only_ids {
            self.inner
                .sync_manager
                .send_command(account_id, ActorCommand::FetchBody(mid));
        }

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
        let now = self.now();
        let outcome = self
            .inner
            .store
            .with_conn(|conn| store::apply_mail_action(conn, &action, &thread_ids))?;
        if !outcome.updated_ids.is_empty() {
            let pairs = self
                .inner
                .store
                .with_conn(|conn| store::thread_account_pairs(conn, &outcome.updated_ids))?;
            let _ = self
                .inner
                .store
                .with_conn(|conn| store::enqueue_offline_actions(conn, &pairs, &action, now));
            let accounts = self
                .inner
                .store
                .with_conn(|conn| store::thread_account_ids(conn, &outcome.updated_ids))?;
            for account_id in accounts {
                self.inner
                    .sync_manager
                    .send_command(account_id, ActorCommand::FlushActions);
                self.emit(CoreEvent::MailChanged {
                    account_id,
                    thread_ids: outcome.updated_ids.clone(),
                });
            }
        }
        Ok(outcome)
    }

    /// Number of queued offline actions awaiting remote IMAP synchronization for an account.
    pub async fn pending_offline_actions(&self, account_id: AccountId) -> Result<u32, CoreError> {
        self.inner
            .store
            .with_conn(|conn| store::pending_action_count(conn, account_id))
    }

    // ---------------------------------------------------------------- drafts

    pub async fn create_draft(&self, input: CreateDraftInput) -> Result<DraftView, CoreError> {
        use base64::Engine as _;
        const MAX_TOTAL_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;

        let mut decoded_attachments: Vec<(String, String, Vec<u8>)> =
            Vec::with_capacity(input.attachments.len());
        let mut total_bytes = 0usize;
        for att in &input.attachments {
            let filename = att.filename.trim();
            if filename.is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "attachments".to_string(),
                    reason: "attachment filename is required".to_string(),
                });
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&att.data_base64)
                .map_err(|e| CoreError::InvalidInput {
                    field: "attachments".to_string(),
                    reason: format!("invalid base64 attachment '{filename}': {e}"),
                })?;
            total_bytes = total_bytes.saturating_add(bytes.len());
            if total_bytes > MAX_TOTAL_ATTACHMENT_BYTES {
                return Err(CoreError::InvalidInput {
                    field: "attachments".to_string(),
                    reason: "total attachment size exceeds 25 MB limit".to_string(),
                });
            }
            let mime = if att.mime.trim().is_empty() {
                "application/octet-stream".to_string()
            } else {
                att.mime.trim().to_string()
            };
            decoded_attachments.push((filename.to_string(), mime, bytes));
        }

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
                    body_html: input.body_html,
                    origin: DraftOrigin::Human,
                    state: DraftState::Editing,
                    updated_at: now,
                },
            )?;
            if !decoded_attachments.is_empty() {
                store::insert_draft_attachments(conn, draft_id, &decoded_attachments)?;
            }
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

            use base64::Engine as _;
            let frozen_attachments: Vec<FrozenAttachment> =
                store::list_draft_attachments(conn, draft_id)?
                    .into_iter()
                    .map(|att| FrozenAttachment {
                        filename: att.filename,
                        mime: att.mime,
                        data_base64: base64::engine::general_purpose::STANDARD.encode(&att.data),
                    })
                    .collect();

            let frozen = freeze_message(
                FreezeInput {
                    from_name: identity.display_name.clone(),
                    from_address: identity.address.clone(),
                    to: draft.to.clone(),
                    cc: draft.cc.clone(),
                    bcc: draft.bcc.clone(),
                    subject: draft.subject.clone(),
                    body_text: draft.body_text.clone(),
                    body_html: draft.body_html.clone(),
                    attachments: frozen_attachments,
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
        self.inner.sync_manager.wake_smtp();

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

    // ---------------------------------------------------------------- attachments & signatures

    /// Return attachment bytes (base64-encoded). If not yet cached in SQLite, fetches
    /// the owning message from IMAP on demand and caches the attachment locally.
    pub async fn download_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> Result<AttachmentData, CoreError> {
        use base64::Engine as _;

        let att = self
            .inner
            .store
            .with_conn(|conn| store::get_attachment(conn, attachment_id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "attachment".to_string(),
            })?;

        if let Some(bytes) = att.data {
            return Ok(AttachmentData {
                id: att.id,
                filename: att.filename,
                mime: att.mime,
                size: bytes.len() as u64,
                data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
            });
        }

        // On-demand IMAP fetch for uncached attachment.
        let location = self
            .inner
            .store
            .with_conn(|conn| store::message_imap_location(conn, att.message_id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "attachment source message".to_string(),
            })?;
        let account = self.account_row(att.account_id)?;
        let Some(secret) = self
            .resolve_transport_secret(att.account_id, &account.auth)
            .await?
        else {
            return Err(CoreError::AuthRequired {
                account_id: att.account_id,
                reason: "no saved credential in OS keychain".to_string(),
            });
        };
        let plan = sync::imap::plan_for(
            att.account_id,
            &account.auth,
            secret,
            location.0,
            sync::imap::SyncWindow {
                since: self.now(),
                now: self.now(),
                min_uid: 0,
            },
        );
        let fetched = sync::imap::fetch_bodies_for_uids(&plan, &[location.1]).await?;
        let bytes = fetched
            .into_iter()
            .flat_map(|m| m.attachments.into_iter())
            .find(|parsed| {
                parsed.filename == att.filename
                    || (att.part_id.is_some() && parsed.part_id == att.part_id)
            })
            .and_then(|parsed| parsed.data)
            .ok_or_else(|| CoreError::NotFound {
                entity: "attachment payload on server".to_string(),
            })?;

        self.inner
            .store
            .with_conn(|conn| store::cache_attachment_data(conn, att.id, &bytes))?;

        Ok(AttachmentData {
            id: att.id,
            filename: att.filename,
            mime: att.mime,
            size: bytes.len() as u64,
            data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        })
    }

    /// Save an attachment to a local file path (downloads on demand if uncached).
    pub async fn save_attachment(
        &self,
        attachment_id: AttachmentId,
        dest_path: PathBuf,
    ) -> Result<String, CoreError> {
        use base64::Engine as _;
        let data = self.download_attachment(attachment_id).await?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&data.data_base64)
            .map_err(|e| CoreError::Internal {
                reason: format!("decode attachment base64: {e}"),
            })?;
        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| CoreError::Storage {
                    reason: format!("create attachment parent dir: {e}"),
                })?;
            }
        }
        std::fs::write(&dest_path, &bytes).map_err(|e| CoreError::Storage {
            reason: format!("write attachment file: {e}"),
        })?;
        Ok(dest_path.to_string_lossy().to_string())
    }

    pub async fn list_signatures(
        &self,
        account_id: Option<AccountId>,
    ) -> Result<Vec<SignatureView>, CoreError> {
        self.inner
            .store
            .with_conn(|conn| store::list_signatures(conn, account_id))
    }

    pub async fn save_signature(
        &self,
        input: SaveSignatureInput,
    ) -> Result<SignatureView, CoreError> {
        let now = self.now();
        self.inner
            .store
            .with_conn(|conn| store::save_signature(conn, &input, now))
    }

    pub async fn delete_signature(&self, signature_id: SignatureId) -> Result<(), CoreError> {
        let deleted = self
            .inner
            .store
            .with_conn(|conn| store::delete_signature(conn, signature_id))?;
        if !deleted {
            return Err(CoreError::NotFound {
                entity: "signature".to_string(),
            });
        }
        Ok(())
    }

    // ---------------------------------------------------------------- Phase 3: workflow

    pub async fn list_saved_searches(&self) -> Result<Vec<SavedSearchView>, CoreError> {
        self.inner.store.with_conn(store::list_saved_searches)
    }

    pub async fn save_saved_search(
        &self,
        input: SaveSavedSearchInput,
    ) -> Result<SavedSearchView, CoreError> {
        let now = self.now();
        self.inner
            .store
            .with_conn(|conn| store::save_saved_search(conn, &input, now))
    }

    pub async fn delete_saved_search(&self, id: SavedSearchId) -> Result<(), CoreError> {
        let deleted = self
            .inner
            .store
            .with_conn(|conn| store::delete_saved_search(conn, id))?;
        if !deleted {
            return Err(CoreError::NotFound {
                entity: "saved search".to_string(),
            });
        }
        Ok(())
    }

    pub async fn list_screener_senders(&self) -> Result<Vec<ScreenerSenderView>, CoreError> {
        self.inner.store.with_conn(store::list_screener_senders)
    }

    pub async fn screen_sender(
        &self,
        address: String,
        display_name: Option<String>,
        decision: ScreenerDecision,
    ) -> Result<u32, CoreError> {
        let now = self.now();
        let affected = self.inner.store.with_conn(|conn| {
            store::screen_sender(conn, &address, display_name.as_deref(), decision, now)
        })?;
        self.emit(CoreEvent::Resync);
        Ok(affected)
    }

    pub async fn list_vip_senders(&self) -> Result<Vec<VipSenderView>, CoreError> {
        self.inner.store.with_conn(store::list_vip_senders)
    }

    pub async fn add_vip_sender(
        &self,
        address: String,
        display_name: Option<String>,
    ) -> Result<VipSenderView, CoreError> {
        let now = self.now();
        let view = self
            .inner
            .store
            .with_conn(|conn| store::add_vip_sender(conn, &address, display_name.as_deref(), now))?;
        self.emit(CoreEvent::Resync);
        Ok(view)
    }

    pub async fn remove_vip_sender(&self, address: String) -> Result<(), CoreError> {
        let removed = self
            .inner
            .store
            .with_conn(|conn| store::remove_vip_sender(conn, &address))?;
        if !removed {
            return Err(CoreError::NotFound {
                entity: "vip sender".to_string(),
            });
        }
        self.emit(CoreEvent::Resync);
        Ok(())
    }

    pub async fn list_labels(&self) -> Result<Vec<LabelView>, CoreError> {
        self.inner.store.with_conn(store::list_labels)
    }

    pub async fn save_label(&self, input: SaveLabelInput) -> Result<LabelView, CoreError> {
        let now = self.now();
        let view = self
            .inner
            .store
            .with_conn(|conn| store::save_label(conn, &input, now))?;
        self.emit(CoreEvent::Resync);
        Ok(view)
    }

    pub async fn delete_label(&self, label_id: LabelId) -> Result<(), CoreError> {
        let deleted = self
            .inner
            .store
            .with_conn(|conn| store::delete_label(conn, label_id))?;
        if !deleted {
            return Err(CoreError::NotFound {
                entity: "label".to_string(),
            });
        }
        self.emit(CoreEvent::Resync);
        Ok(())
    }

    pub async fn label_threads(
        &self,
        label_id: LabelId,
        thread_ids: Vec<ThreadId>,
        add: bool,
    ) -> Result<(), CoreError> {
        self.inner.store.with_conn(|conn| {
            if !store::label_exists(conn, label_id)? {
                return Err(CoreError::NotFound {
                    entity: "label".to_string(),
                });
            }
            store::set_thread_label(conn, label_id, &thread_ids, add)
        })?;
        self.emit(CoreEvent::Resync);
        Ok(())
    }

    pub async fn list_rules(&self) -> Result<Vec<MailRuleView>, CoreError> {
        self.inner.store.with_conn(store::list_rules)
    }

    pub async fn save_rule(&self, input: SaveMailRuleInput) -> Result<MailRuleView, CoreError> {
        let now = self.now();
        self.inner
            .store
            .with_conn(|conn| store::save_rule(conn, &input, now))
    }

    pub async fn delete_rule(&self, rule_id: RuleId) -> Result<(), CoreError> {
        let deleted = self
            .inner
            .store
            .with_conn(|conn| store::delete_rule(conn, rule_id))?;
        if !deleted {
            return Err(CoreError::NotFound {
                entity: "rule".to_string(),
            });
        }
        Ok(())
    }

    pub async fn run_rule_now(&self, rule_id: RuleId) -> Result<u32, CoreError> {
        let matched = self.inner.store.with_conn(|conn| {
            let rule = store::get_rule(conn, rule_id)?.ok_or_else(|| CoreError::NotFound {
                entity: "rule".to_string(),
            })?;
            store::run_rule_retroactively(conn, &rule.conditions, rule.match_all, &rule.actions)
        })?;
        if matched > 0 {
            self.emit(CoreEvent::Resync);
        }
        Ok(matched)
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
            let sent_msg_id = crate::ids::MessageId::fresh();
            let thread_id = match job.thread_id {
                Some(thread_id) => {
                    store::insert_message(
                        conn,
                        &store::NewMessage {
                            id: sent_msg_id,
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
                            raw_html: frozen.body_html.clone(),
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
                            id: sent_msg_id,
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
                            raw_html: frozen.body_html.clone(),
                            unsub: None,
                            trackers: 0,
                        },
                    )?;
                    thread_id
                }
            };

            if !frozen.attachments.is_empty() {
                use base64::Engine as _;
                let mut sent_atts = Vec::with_capacity(frozen.attachments.len());
                for att in &frozen.attachments {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&att.data_base64) {
                        sent_atts.push(store::NewAttachment {
                            id: AttachmentId::fresh(),
                            message_id: sent_msg_id,
                            account_id: job.account_id,
                            filename: att.filename.clone(),
                            mime: att.mime.clone(),
                            size: bytes.len() as u64,
                            content_id: None,
                            is_inline: false,
                            part_id: None,
                            data: Some(bytes),
                        });
                    }
                }
                if !sent_atts.is_empty() {
                    store::insert_attachments(conn, &sent_atts)?;
                    store::refresh_thread_summary(conn, thread_id)?;
                }
            }

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
            secret,
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
        sync::imap::verify_in_sent(&plan, &message_id).await
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
            secret,
            folder,
            sync::imap::SyncWindow {
                since,
                now,
                min_uid,
            },
        );
        let fetched = sync::imap::fetch_window(&plan).await;
        self.inner
            .sync_manager
            .send_command(account_id, ActorCommand::SyncNow);

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
                    self.notify_hermes_ingested(account_id, &touched);
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

    // ---------------------------------------------------------------- agent & AI (Phase 4)

    pub async fn list_agent_permissions(&self) -> Result<Vec<AgentAccountPermissionView>, CoreError> {
        self.inner.store.with_conn(store::list_agent_permissions)
    }

    pub async fn save_agent_permission(
        &self,
        input: SaveAgentAccountPermissionInput,
    ) -> Result<AgentAccountPermissionView, CoreError> {
        let now = self.now();
        self.inner
            .store
            .with_conn(|conn| store::save_account_permission(conn, &input, now))
    }

    pub async fn get_agent_bridge_config(&self) -> Result<AgentBridgeConfig, CoreError> {
        let mut cfg = self.inner.store.with_conn(store::get_bridge_config)?;
        cfg.has_webhook_secret = self
            .inner
            .host
            .secrets
            .get(&SecretKey::webhook_secret())?
            .is_some();
        cfg.ipc_endpoint = mcp::default_ipc_endpoint();
        Ok(cfg)
    }

    pub async fn save_agent_bridge_config(
        &self,
        input: SaveAgentBridgeConfigInput,
    ) -> Result<AgentBridgeConfig, CoreError> {
        let cleaned_url = mcp::validate_localhost_webhook_url(&input.webhook_url)?;
        if input.sse_port == 0 {
            return Err(CoreError::InvalidInput {
                field: "ssePort".to_string(),
                reason: "SSE port must be between 1 and 65535".to_string(),
            });
        }
        if input.clear_webhook_secret {
            self.inner.host.secrets.delete(&SecretKey::webhook_secret())?;
        } else if let Some(secret) = input.webhook_secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            self.inner
                .host
                .secrets
                .put(&SecretKey::webhook_secret(), &SecretString::new(secret))?;
        }
        self.inner.store.with_conn(|conn| {
            store::save_bridge_config(
                conn,
                input.mcp_enabled,
                input.sse_port,
                input.webhook_enabled,
                &cleaned_url,
            )
        })?;
        self.get_agent_bridge_config().await
    }

    pub async fn list_agent_activity(&self, limit: u32) -> Result<Vec<AgentActivityView>, CoreError> {
        self.inner
            .store
            .with_conn(|conn| store::list_agent_activity(conn, limit))
    }

    /// Reverse an undoable agent mutation in 1 click and mark the activity entry undone.
    pub async fn undo_agent_activity(
        &self,
        activity_id: AgentActivityId,
    ) -> Result<AgentActivityView, CoreError> {
        let now = self.now();
        let entry = self
            .inner
            .store
            .with_conn(|conn| store::get_agent_activity(conn, activity_id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "agent activity".to_string(),
            })?;
        if entry.undone {
            return Ok(entry.to_view());
        }
        let Some(payload) = entry.undo_payload else {
            return Err(CoreError::IllegalState {
                reason: "this agent action has no undo payload".to_string(),
            });
        };

        match payload {
            AgentUndoPayload::RestoreThreadFlags { states } => {
                self.inner
                    .store
                    .with_conn(|conn| store::restore_thread_flags(conn, &states))?;
            }
            AgentUndoPayload::ToggleLabel {
                label_id,
                thread_ids,
                was_added,
            } => {
                self.inner.store.with_conn(|conn| {
                    if store::label_exists(conn, label_id)? {
                        store::set_thread_label(conn, label_id, &thread_ids, !was_added)?;
                    }
                    Ok(())
                })?;
            }
            AgentUndoPayload::CancelSnooze {
                thread_id,
                previous_due_at,
            } => {
                match previous_due_at {
                    Some(prev) if prev > now => {
                        let _ = self.snooze_thread(thread_id, prev).await?;
                    }
                    _ => {
                        let _ = self.unsnooze_thread(thread_id).await;
                    }
                }
            }
        }

        let updated = self.inner.store.with_conn(|conn| {
            store::mark_agent_activity_undone(conn, activity_id, now)?;
            store::get_agent_activity(conn, activity_id)?
                .map(|r| r.to_view())
                .ok_or_else(|| CoreError::NotFound {
                    entity: "agent activity".to_string(),
                })
        })?;
        self.emit(CoreEvent::Resync);
        self.emit(CoreEvent::AgentActivityChanged);
        Ok(updated)
    }

    pub async fn list_pending_agent_drafts(&self) -> Result<Vec<DraftView>, CoreError> {
        self.inner.store.with_conn(store::list_pending_agent_drafts)
    }

    pub async fn reject_agent_draft(
        &self,
        draft_id: DraftId,
        reason: String,
    ) -> Result<(), CoreError> {
        let now = self.now();
        self.inner.store.with_conn(|conn| {
            let draft = store::get_draft(conn, draft_id)?.ok_or_else(|| CoreError::NotFound {
                entity: "draft".to_string(),
            })?;
            if !matches!(draft.origin, DraftOrigin::Agent { .. }) {
                return Err(CoreError::IllegalState {
                    reason: "only agent drafts can be rejected".to_string(),
                });
            }
            store::set_draft_state(
                conn,
                draft_id,
                &DraftState::Rejected {
                    reason: if reason.trim().is_empty() {
                        "Rejected by user".to_string()
                    } else {
                        reason.trim().to_string()
                    },
                },
                now,
            )
        })?;
        self.emit(CoreEvent::DraftChanged { draft_id });
        self.emit(CoreEvent::AgentActivityChanged);
        Ok(())
    }

    // ---------------------------------------------------------------- In-Pane AI

    pub async fn get_ai_config(&self) -> Result<AiConfigView, CoreError> {
        let mut cfg = self.inner.store.with_conn(store::get_ai_config)?;
        cfg.has_api_key = self
            .inner
            .host
            .secrets
            .get(&SecretKey::ai_api_key())?
            .is_some();
        Ok(cfg)
    }

    pub async fn save_ai_config(
        &self,
        input: SaveAiConfigInput,
    ) -> Result<AiConfigView, CoreError> {
        let base_url = input.base_url.trim();
        if input.enabled && base_url.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "baseUrl".to_string(),
                reason: "AI base URL is required when AI is enabled".to_string(),
            });
        }
        if !base_url.is_empty() {
            let parsed = url::Url::parse(base_url).map_err(|e| CoreError::InvalidInput {
                field: "baseUrl".to_string(),
                reason: format!("invalid AI base URL: {e}"),
            })?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(CoreError::InvalidInput {
                    field: "baseUrl".to_string(),
                    reason: "AI base URL must use http:// or https://".to_string(),
                });
            }
        }
        if input.clear_api_key {
            self.inner.host.secrets.delete(&SecretKey::ai_api_key())?;
        } else if let Some(key) = input.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            self.inner
                .host
                .secrets
                .put(&SecretKey::ai_api_key(), &SecretString::new(key))?;
        }
        self.inner
            .store
            .with_conn(|conn| store::save_ai_config(conn, input.enabled, base_url, input.model.trim()))?;
        self.get_ai_config().await
    }

    pub async fn ai_summarize_thread(&self, thread_id: ThreadId) -> Result<AiSummaryResult, CoreError> {
        let cfg = self.get_ai_config().await?;
        if !cfg.enabled {
            return Err(CoreError::IllegalState {
                reason: "In-pane AI is disabled — enable it in Settings".to_string(),
            });
        }
        let api_key = self.inner.host.secrets.get(&SecretKey::ai_api_key())?;
        let plain = self.read_thread_plain_unframed(thread_id)?;
        ai::summarize_thread(&cfg.base_url, &cfg.model, api_key.as_ref(), &plain).await
    }

    pub async fn ai_ask_thread(
        &self,
        thread_id: ThreadId,
        question: String,
        selected_text: Option<String>,
    ) -> Result<AiAnswerResult, CoreError> {
        if question.trim().is_empty() {
            return Err(CoreError::InvalidInput {
                field: "question".to_string(),
                reason: "question is required".to_string(),
            });
        }
        let cfg = self.get_ai_config().await?;
        if !cfg.enabled {
            return Err(CoreError::IllegalState {
                reason: "In-pane AI is disabled — enable it in Settings".to_string(),
            });
        }
        let api_key = self.inner.host.secrets.get(&SecretKey::ai_api_key())?;
        let plain = self.read_thread_plain_unframed(thread_id)?;
        ai::ask_thread(
            &cfg.base_url,
            &cfg.model,
            api_key.as_ref(),
            &plain,
            &question,
            selected_text.as_deref(),
        )
        .await
    }

    pub async fn ai_draft_reply(
        &self,
        thread_id: ThreadId,
        instructions: String,
        stage_for_approval: bool,
    ) -> Result<AiDraftResult, CoreError> {
        let cfg = self.get_ai_config().await?;
        if !cfg.enabled {
            return Err(CoreError::IllegalState {
                reason: "In-pane AI is disabled — enable it in Settings".to_string(),
            });
        }
        let api_key = self.inner.host.secrets.get(&SecretKey::ai_api_key())?;
        let plain = self.read_thread_plain_unframed(thread_id)?;
        let body_text =
            ai::draft_reply(&cfg.base_url, &cfg.model, api_key.as_ref(), &plain, &instructions)
                .await?;
        let staged_draft = if stage_for_approval {
            let rationale = if instructions.trim().is_empty() {
                "Drafted by in-pane AI".to_string()
            } else {
                format!("In-pane AI draft: {}", instructions.trim())
            };
            Some(
                self.stage_agent_draft("in-pane-ai", thread_id, body_text.clone(), rationale)
                    .await?,
            )
        } else {
            None
        };
        Ok(AiDraftResult {
            body_text,
            staged_draft,
        })
    }

    // ---------------------------------------------------------------- agent internal helpers

    fn check_account_perm(
        &self,
        account_id: AccountId,
        need_read: bool,
        need_triage: bool,
        need_drafts: bool,
    ) -> Result<store::AccountPermissionRow, CoreError> {
        let perm = self
            .inner
            .store
            .with_conn(|conn| store::get_account_permission(conn, account_id))?;
        if !perm.enabled {
            return Err(CoreError::PermissionDenied {
                reason: "agent access is disabled for this account".to_string(),
            });
        }
        if need_read && !perm.allow_read {
            return Err(CoreError::PermissionDenied {
                reason: "agent read access is disabled for this account".to_string(),
            });
        }
        if need_triage && !perm.allow_triage {
            return Err(CoreError::PermissionDenied {
                reason: "agent triage mutations are disabled for this account".to_string(),
            });
        }
        if need_drafts && !perm.allow_drafts {
            return Err(CoreError::PermissionDenied {
                reason: "agent draft staging is disabled for this account".to_string(),
            });
        }
        Ok(perm)
    }

    fn check_thread_access(
        &self,
        thread_id: ThreadId,
        need_read: bool,
        need_triage: bool,
        need_drafts: bool,
    ) -> Result<AccountId, CoreError> {
        let (account_id, folders) = self.inner.store.with_conn(|conn| {
            let thread = store::get_thread(conn, thread_id)?.ok_or(CoreError::NotFound {
                entity: "thread".to_string(),
            })?;
            let folders = store::thread_folders(conn, thread_id)?;
            Ok((thread.account_id, folders))
        })?;
        let perm = self.check_account_perm(account_id, need_read, need_triage, need_drafts)?;
        if !folders.is_empty() && folders.iter().all(|f| !perm.is_folder_allowed(f)) {
            return Err(CoreError::PermissionDenied {
                reason: "agent access is disabled for this thread's folder".to_string(),
            });
        }
        Ok(account_id)
    }

    fn resolve_agent_threads(
        &self,
        mut thread_ids: Vec<ThreadId>,
        message_ids: Vec<MessageId>,
    ) -> Result<Vec<ThreadId>, CoreError> {
        if !message_ids.is_empty() {
            let from_msgs = self
                .inner
                .store
                .with_conn(|conn| store::resolve_message_thread_ids(conn, &message_ids))?;
            for tid in from_msgs {
                if !thread_ids.contains(&tid) {
                    thread_ids.push(tid);
                }
            }
        }
        if thread_ids.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "thread_ids".to_string(),
                reason: "at least one thread_id or message_id is required".to_string(),
            });
        }
        Ok(thread_ids)
    }

    pub(crate) async fn agent_list_accounts(&self) -> Result<Vec<AccountView>, CoreError> {
        let accounts = self.list_accounts().await?;
        let mut allowed = Vec::new();
        for acct in accounts {
            if let Ok(perm) = self
                .inner
                .store
                .with_conn(|conn| store::get_account_permission(conn, acct.id))
            {
                if perm.enabled {
                    allowed.push(acct);
                }
            }
        }
        Ok(allowed)
    }

    pub(crate) async fn agent_list_folders(
        &self,
        account_id: Option<AccountId>,
    ) -> Result<Vec<FolderInfoView>, CoreError> {
        let target_ids: Vec<AccountId> = match account_id {
            Some(id) => {
                self.check_account_perm(id, true, false, false)?;
                vec![id]
            }
            None => self
                .agent_list_accounts()
                .await?
                .into_iter()
                .map(|a| a.id)
                .collect(),
        };
        self.inner.store.with_conn(|conn| {
            let mut out = Vec::new();
            for id in target_ids {
                let perm = store::get_account_permission(conn, id)?;
                if !perm.enabled || !perm.allow_read {
                    continue;
                }
                for folder in store::list_account_folders(conn, id, &perm)? {
                    if folder.agent_allowed {
                        out.push(folder);
                    }
                }
            }
            Ok(out)
        })
    }

    pub(crate) async fn agent_list_messages(
        &self,
        account_id: Option<AccountId>,
        folder: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ThreadSummary>, CoreError> {
        if let Some(id) = account_id {
            let perm = self.check_account_perm(id, true, false, false)?;
            if let Some(f) = folder {
                if !perm.is_folder_allowed(f) {
                    return Err(CoreError::PermissionDenied {
                        reason: format!("agent access is disabled for folder '{f}'"),
                    });
                }
            }
        }
        let view = mailbox_view_for_folder(folder);
        let query = build_agent_query(account_id, view, limit, offset);
        let items = self.list_threads(query).await?.items;
        self.filter_permitted_summaries(items)
    }

    pub(crate) async fn agent_search_messages(
        &self,
        text: &str,
        account_id: Option<AccountId>,
        limit: u32,
    ) -> Result<Vec<ThreadSummary>, CoreError> {
        if let Some(id) = account_id {
            self.check_account_perm(id, true, false, false)?;
        }
        let query = build_agent_query(
            account_id,
            MailboxView::Search {
                text: text.to_string(),
            },
            limit,
            0,
        );
        let items = self.list_threads(query).await?.items;
        self.filter_permitted_summaries(items)
    }

    fn filter_permitted_summaries(
        &self,
        items: Vec<ThreadSummary>,
    ) -> Result<Vec<ThreadSummary>, CoreError> {
        self.inner.store.with_conn(|conn| {
            let mut perm_cache: HashMap<AccountId, store::AccountPermissionRow> = HashMap::new();
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let perm = match perm_cache.get(&item.account_id) {
                    Some(p) => p,
                    None => {
                        let p = store::get_account_permission(conn, item.account_id)?;
                        perm_cache.insert(item.account_id, p);
                        perm_cache.get(&item.account_id).unwrap()
                    }
                };
                if !perm.enabled || !perm.allow_read {
                    continue;
                }
                let folders = store::thread_folders(conn, item.id)?;
                if !folders.is_empty() && folders.iter().all(|f| !perm.is_folder_allowed(f)) {
                    continue;
                }
                out.push(item);
            }
            Ok(out)
        })
    }

    fn read_thread_plain_unframed(&self, thread_id: ThreadId) -> Result<String, CoreError> {
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

    /// Read-only plain-text rendering of one message for [`AgentPort`] — no HTML ever
    /// crosses, and content is wrapped in prompt-injection framing.
    pub(crate) async fn agent_read_message_plain(
        &self,
        message_id: MessageId,
    ) -> Result<String, CoreError> {
        let msg = self
            .inner
            .store
            .with_conn(|conn| store::get_single_message(conn, message_id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "message".to_string(),
            })?;
        let perm = self.check_account_perm(msg.account_id, true, false, false)?;
        if !perm.is_folder_allowed(&msg.folder_name) {
            return Err(CoreError::PermissionDenied {
                reason: format!("agent access is disabled for folder '{}'", msg.folder_name),
            });
        }
        let from = match &msg.from_name {
            Some(name) => format!("{name} <{}>", msg.from_email),
            None => msg.from_email.clone(),
        };
        let date = chrono::DateTime::from_timestamp_millis(msg.date.0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| msg.date.0.to_string());
        let raw = format!(
            "Message-ID: {}\nThread-ID: {}\nFolder: {}\nDate: {}\nFrom: {}\nTo: {}\nSubject: {}\n\n{}",
            msg.id,
            msg.thread_id,
            msg.folder_name,
            date,
            from,
            msg.to.join(", "),
            msg.subject,
            msg.body_text
        );
        Ok(frame_untrusted_email_content(&raw))
    }

    /// Read-only plain-text rendering of a thread for [`AgentPort`] — no HTML ever
    /// crosses, and content is wrapped in prompt-injection framing.
    pub(crate) async fn agent_read_thread_plain(
        &self,
        thread_id: ThreadId,
    ) -> Result<String, CoreError> {
        self.check_thread_access(thread_id, true, false, false)?;
        let raw = self.read_thread_plain_unframed(thread_id)?;
        Ok(frame_untrusted_email_content(&raw))
    }

    pub(crate) async fn agent_create_draft(
        &self,
        agent_id: &str,
        input: AgentCreateDraftInput,
    ) -> Result<DraftView, CoreError> {
        if let Some(thread_id) = input.reply_to_thread {
            self.check_thread_access(thread_id, false, false, true)?;
            return self
                .stage_agent_draft(agent_id, thread_id, input.body_text, input.rationale)
                .await;
        }
        let account_id = match input.account_id {
            Some(id) => id,
            None => self
                .agent_list_accounts()
                .await?
                .into_iter()
                .next()
                .map(|a| a.id)
                .ok_or_else(|| CoreError::NotFound {
                    entity: "permitted account".to_string(),
                })?,
        };
        self.check_account_perm(account_id, false, false, true)?;

        let now = self.now();
        let view = self.inner.store.with_conn(|conn| {
            store::get_account(conn, account_id)?.ok_or(CoreError::NotFound {
                entity: "account".to_string(),
            })?;
            let identity =
                store::primary_identity(conn, account_id)?.ok_or(CoreError::NotFound {
                    entity: "identity".to_string(),
                })?;
            let draft_id = store::insert_draft(
                conn,
                &store::InsertDraft {
                    account_id,
                    from_identity_id: identity.id,
                    from_locked: false,
                    reply_to_thread: None,
                    to: input.to,
                    cc: input.cc,
                    bcc: Vec::new(),
                    subject: input.subject,
                    body_text: input.body_text,
                    body_html: None,
                    origin: DraftOrigin::Agent {
                        agent_id: agent_id.to_string(),
                        rationale: input.rationale,
                    },
                    state: DraftState::PendingApproval,
                    updated_at: now,
                },
            )?;
            let draft = store::get_draft(conn, draft_id)?.ok_or(CoreError::Internal {
                reason: "draft disappeared after insert".to_string(),
            })?;
            let view = store::draft_view(conn, &draft)?;
            let _ = store::insert_agent_activity(
                conn,
                &store::NewAgentActivity {
                    agent_id,
                    tool_name: "create_draft",
                    summary: format!("Staged draft \u{201c}{}\u{201d} for approval", view.subject),
                    account_id: Some(account_id),
                    thread_id: None,
                    undo_payload: None,
                    created_at: now,
                },
            );
            Ok(view)
        })?;
        self.emit(CoreEvent::DraftChanged { draft_id: view.id });
        self.emit(CoreEvent::Attention {
            alert: Alert::ApprovalRequested { draft_id: view.id },
        });
        self.emit(CoreEvent::AgentActivityChanged);
        Ok(view)
    }

    pub(crate) async fn agent_update_draft(
        &self,
        agent_id: &str,
        input: AgentUpdateDraftInput,
    ) -> Result<DraftView, CoreError> {
        let now = self.now();
        let existing_account = self.inner.store.with_conn(|conn| {
            store::get_draft(conn, input.draft_id)?
                .map(|d| d.account_id)
                .ok_or_else(|| CoreError::NotFound {
                    entity: "draft".to_string(),
                })
        })?;
        self.check_account_perm(existing_account, false, false, true)?;
        let view = self.inner.store.with_conn(|conn| {
            let updated = store::update_pending_agent_draft(
                conn,
                &store::UpdateAgentDraftParams {
                    draft_id: input.draft_id,
                    to: input.to,
                    cc: input.cc,
                    subject: input.subject,
                    body_text: input.body_text,
                    rationale: input.rationale,
                    updated_at: now,
                },
            )?;
            let _ = store::insert_agent_activity(
                conn,
                &store::NewAgentActivity {
                    agent_id,
                    tool_name: "update_draft",
                    summary: format!("Updated staged draft \u{201c}{}\u{201d}", updated.subject),
                    account_id: Some(updated.account_id),
                    thread_id: updated.reply_to_thread,
                    undo_payload: None,
                    created_at: now,
                },
            );
            Ok(updated)
        })?;
        self.emit(CoreEvent::DraftChanged { draft_id: view.id });
        self.emit(CoreEvent::AgentActivityChanged);
        Ok(view)
    }

    pub(crate) async fn agent_mark_messages(
        &self,
        agent_id: &str,
        thread_ids: Vec<ThreadId>,
        message_ids: Vec<MessageId>,
        read: Option<bool>,
        starred: Option<bool>,
    ) -> Result<Vec<ThreadId>, CoreError> {
        if read.is_none() && starred.is_none() {
            return Err(CoreError::InvalidInput {
                field: "read/starred".to_string(),
                reason: "specify at least one of read or starred".to_string(),
            });
        }
        let resolved = self.resolve_agent_threads(thread_ids, message_ids)?;
        let mut first_account = None;
        for &tid in &resolved {
            let acct = self.check_thread_access(tid, false, true, false)?;
            if first_account.is_none() {
                first_account = Some(acct);
            }
        }
        let snapshots = self
            .inner
            .store
            .with_conn(|conn| store::snapshot_thread_flags(conn, &resolved))?;

        let mut updated = Vec::new();
        if let Some(r) = read {
            let outcome = self
                .act_on_threads(MailAction::MarkRead { read: r }, resolved.clone())
                .await?;
            updated = outcome.updated_ids;
        }
        if let Some(s) = starred {
            let outcome = self
                .act_on_threads(MailAction::Star { on: s }, resolved.clone())
                .await?;
            if updated.is_empty() {
                updated = outcome.updated_ids;
            }
        }

        let mut parts = Vec::new();
        if let Some(r) = read {
            parts.push(if r { "read" } else { "unread" });
        }
        if let Some(s) = starred {
            parts.push(if s { "starred" } else { "unstarred" });
        }
        let summary = format!(
            "Marked {} thread{} {}",
            resolved.len(),
            if resolved.len() == 1 { "" } else { "s" },
            parts.join(" & ")
        );
        let now = self.now();
        self.inner.store.with_conn(|conn| {
            store::insert_agent_activity(
                conn,
                &store::NewAgentActivity {
                    agent_id,
                    tool_name: "mark_messages",
                    summary,
                    account_id: first_account,
                    thread_id: resolved.first().copied(),
                    undo_payload: Some(AgentUndoPayload::RestoreThreadFlags { states: snapshots }),
                    created_at: now,
                },
            )
        })?;
        self.emit(CoreEvent::AgentActivityChanged);
        Ok(updated)
    }

    pub(crate) async fn agent_move_messages(
        &self,
        agent_id: &str,
        thread_ids: Vec<ThreadId>,
        message_ids: Vec<MessageId>,
        destination: AgentMoveDestination,
    ) -> Result<Vec<ThreadId>, CoreError> {
        let resolved = self.resolve_agent_threads(thread_ids, message_ids)?;
        let mut first_account = None;
        for &tid in &resolved {
            let acct = self.check_thread_access(tid, false, true, false)?;
            if first_account.is_none() {
                first_account = Some(acct);
            }
        }
        let snapshots = self
            .inner
            .store
            .with_conn(|conn| store::snapshot_thread_flags(conn, &resolved))?;
        let action = action_for_destination(destination);
        let outcome = self.act_on_threads(action, resolved.clone()).await?;
        let dest_label = match destination {
            AgentMoveDestination::Inbox => "Inbox",
            AgentMoveDestination::Archive => "Archive",
            AgentMoveDestination::Trash => "Trash",
            AgentMoveDestination::Spam => "Spam",
        };
        let now = self.now();
        self.inner.store.with_conn(|conn| {
            store::insert_agent_activity(
                conn,
                &store::NewAgentActivity {
                    agent_id,
                    tool_name: "move_messages",
                    summary: format!(
                        "Moved {} thread{} to {}",
                        resolved.len(),
                        if resolved.len() == 1 { "" } else { "s" },
                        dest_label
                    ),
                    account_id: first_account,
                    thread_id: resolved.first().copied(),
                    undo_payload: Some(AgentUndoPayload::RestoreThreadFlags { states: snapshots }),
                    created_at: now,
                },
            )
        })?;
        self.emit(CoreEvent::AgentActivityChanged);
        Ok(outcome.updated_ids)
    }

    pub(crate) async fn agent_label_messages(
        &self,
        agent_id: &str,
        label_id: LabelId,
        thread_ids: Vec<ThreadId>,
        message_ids: Vec<MessageId>,
        add: bool,
    ) -> Result<Vec<ThreadId>, CoreError> {
        let resolved = self.resolve_agent_threads(thread_ids, message_ids)?;
        let mut first_account = None;
        for &tid in &resolved {
            let acct = self.check_thread_access(tid, false, true, false)?;
            if first_account.is_none() {
                first_account = Some(acct);
            }
        }
        self.label_threads(label_id, resolved.clone(), add).await?;
        let now = self.now();
        self.inner.store.with_conn(|conn| {
            store::insert_agent_activity(
                conn,
                &store::NewAgentActivity {
                    agent_id,
                    tool_name: "label_messages",
                    summary: format!(
                        "{} label on {} thread{}",
                        if add { "Applied" } else { "Removed" },
                        resolved.len(),
                        if resolved.len() == 1 { "" } else { "s" }
                    ),
                    account_id: first_account,
                    thread_id: resolved.first().copied(),
                    undo_payload: Some(AgentUndoPayload::ToggleLabel {
                        label_id,
                        thread_ids: resolved.clone(),
                        was_added: add,
                    }),
                    created_at: now,
                },
            )
        })?;
        self.emit(CoreEvent::AgentActivityChanged);
        Ok(resolved)
    }

    pub(crate) async fn agent_snooze_thread(
        &self,
        agent_id: &str,
        thread_id: ThreadId,
        until: UnixMs,
    ) -> Result<ThreadId, CoreError> {
        let account_id = self.check_thread_access(thread_id, false, true, false)?;
        let prev_due = self
            .inner
            .store
            .with_conn(|conn| Ok(store::live_snooze_for_thread(conn, thread_id)?.map(|j| j.due_at)))?;
        let _ = self.snooze_thread(thread_id, until).await?;
        let now = self.now();
        self.inner.store.with_conn(|conn| {
            store::insert_agent_activity(
                conn,
                &store::NewAgentActivity {
                    agent_id,
                    tool_name: "snooze_thread",
                    summary: "Snoozed thread".to_string(),
                    account_id: Some(account_id),
                    thread_id: Some(thread_id),
                    undo_payload: Some(AgentUndoPayload::CancelSnooze {
                        thread_id,
                        previous_due_at: prev_due,
                    }),
                    created_at: now,
                },
            )
        })?;
        self.emit(CoreEvent::AgentActivityChanged);
        Ok(thread_id)
    }

    pub(crate) async fn agent_archive_thread(
        &self,
        agent_id: &str,
        thread_id: ThreadId,
    ) -> Result<ThreadId, CoreError> {
        let account_id = self.check_thread_access(thread_id, false, true, false)?;
        let snapshots = self
            .inner
            .store
            .with_conn(|conn| store::snapshot_thread_flags(conn, &[thread_id]))?;
        let _ = self
            .act_on_threads(MailAction::Archive, vec![thread_id])
            .await?;
        let now = self.now();
        self.inner.store.with_conn(|conn| {
            store::insert_agent_activity(
                conn,
                &store::NewAgentActivity {
                    agent_id,
                    tool_name: "archive_thread",
                    summary: "Archived thread".to_string(),
                    account_id: Some(account_id),
                    thread_id: Some(thread_id),
                    undo_payload: Some(AgentUndoPayload::RestoreThreadFlags { states: snapshots }),
                    created_at: now,
                },
            )
        })?;
        self.emit(CoreEvent::AgentActivityChanged);
        Ok(thread_id)
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
        self.check_thread_access(thread_id, false, false, true)?;
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
                    body_html: None,
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
            let view = store::draft_view(conn, &draft)?;
            let _ = store::insert_agent_activity(
                conn,
                &store::NewAgentActivity {
                    agent_id,
                    tool_name: "create_draft",
                    summary: format!("Staged reply draft for \u{201c}{}\u{201d}", thread.subject),
                    account_id: Some(thread.account_id),
                    thread_id: Some(thread_id),
                    undo_payload: None,
                    created_at: now,
                },
            );
            Ok(view)
        })?;

        self.emit(CoreEvent::DraftChanged { draft_id: view.id });
        self.emit(CoreEvent::Attention {
            alert: Alert::ApprovalRequested { draft_id: view.id },
        });
        self.emit(CoreEvent::AgentActivityChanged);
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
