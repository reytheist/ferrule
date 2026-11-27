//! Per-account async IMAP sync actors & dedicated durable SMTP submission worker.
//!
//! Architecture:
//! - Each account runs an independent Tokio actor task:
//!   1. Drain pending `ActorCommand`s (`SyncNow`, `FlushActions`, `FetchBody`, `Reauthed`, `Shutdown`).
//!   2. Check `AccountPacer` (auth circuit breaker halts retries until `Reauthed`).
//!   3. Execute one fair slice of `offline_action`s (max 20 actions or 2s per slice).
//!   4. `LIST` + `STATUS` across folders; open only folders whose counts/UIDNEXT changed.
//!      Periodically (every 15 min) run a full `\Seen` flag reconciliation pass.
//!   5. Spawn bounded background tasks (max 1 backfill + 1 body-download connection per account)
//!      for historical header backfill and priority/bulk body downloads.
//!   6. Park in interruptible IMAP IDLE (`tokio::select!` on command channel + keepalive timer).
//! - A separate dedicated SMTP submission worker (`Notify`-driven + 5s timer fallback)
//!   processes due outbox sends independently so sending never blocks on mailbox sync.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, Notify, Semaphore};

use crate::error::CoreError;
use crate::events::CoreEvent;
use crate::ids::{AccountId, MessageId};
use crate::store;
use crate::sync::imap::{self, FetchPlan, ImapSession, SyncWindow};
use crate::sync::pacing::{classify_error_message, AccountPacer, FailureKind};
use crate::sync::Dialect;
use crate::types::SyncState;
use crate::MailCore;

const SLICE_MAX_DURATION: Duration = Duration::from_secs(2);
const IDLE_TIMEOUT: Duration = Duration::from_secs(240);
const FLAG_PASS_INTERVAL_MS: i64 = 15 * 60 * 1000; // 15 minutes

#[derive(Debug)]
pub(crate) enum ActorCommand {
    SyncNow,
    FlushActions,
    FetchBody(MessageId),
    Reauthed,
    Shutdown,
}

#[derive(Clone)]
pub(crate) struct SyncManager {
    actors: Arc<Mutex<HashMap<AccountId, mpsc::Sender<ActorCommand>>>>,
    smtp_notify: Arc<Notify>,
    smtp_started: Arc<Mutex<bool>>,
}

impl SyncManager {
    pub(crate) fn new() -> Self {
        Self {
            actors: Arc::new(Mutex::new(HashMap::new())),
            smtp_notify: Arc::new(Notify::new()),
            smtp_started: Arc::new(Mutex::new(false)),
        }
    }

    /// Wake the dedicated SMTP submission worker immediately (e.g., when a draft is queued).
    pub(crate) fn wake_smtp(&self) {
        self.smtp_notify.notify_one();
    }

    /// Start the dedicated durable SMTP submission worker if not already running.
    pub(crate) fn start_smtp_worker(&self, core: MailCore) {
        let mut started = match self.smtp_started.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if *started {
            return;
        }
        *started = true;
        drop(started);

        let notify = self.smtp_notify.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                tokio::select! {
                    _ = notify.notified() => {}
                    _ = interval.tick() => {}
                }
                let _ = core.tick_scheduler().await;
            }
        });
    }

    /// Spawn a per-account sync actor if one is not already active for `account_id`.
    pub(crate) fn ensure_actor(&self, core: MailCore, account_id: AccountId) {
        let mut guard = match self.actors.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some(tx) = guard.get(&account_id) {
            if !tx.is_closed() {
                return;
            }
        }
        let (tx, rx) = mpsc::channel(32);
        guard.insert(account_id, tx);
        drop(guard);

        tokio::spawn(run_account_actor(core, account_id, rx));
    }

    /// Send a non-blocking command to an account's actor (if running).
    pub(crate) fn send_command(&self, account_id: AccountId, cmd: ActorCommand) {
        if let Ok(guard) = self.actors.lock() {
            if let Some(tx) = guard.get(&account_id) {
                let _ = tx.try_send(cmd);
            }
        }
    }

    /// Shut down and remove an account's actor (e.g. on `remove_account`).
    pub(crate) fn stop_actor(&self, account_id: AccountId) {
        if let Ok(mut guard) = self.actors.lock() {
            if let Some(tx) = guard.remove(&account_id) {
                let _ = tx.try_send(ActorCommand::Shutdown);
            }
        }
    }
}

async fn run_account_actor(
    core: MailCore,
    account_id: AccountId,
    mut rx: mpsc::Receiver<ActorCommand>,
) {
    let mut pacer = AccountPacer::new();
    let backfill_sem = Arc::new(Semaphore::new(1));
    let body_sem = Arc::new(Semaphore::new(1));
    let mut session: Option<ImapSession> = None;

    // Recover any offline actions stranded in_flight from a previous crash.
    let _ = core
        .inner
        .store
        .with_conn(store::recover_in_flight_actions);

    loop {
        // 1. If auth circuit breaker is tripped, park until user explicitly re-authenticates.
        if pacer.is_auth_tripped() {
            session = None;
            match rx.recv().await {
                Some(ActorCommand::Reauthed) => {
                    pacer.reset_auth_breaker();
                }
                Some(ActorCommand::Shutdown) | None => break,
                Some(_) => continue,
            }
        }

        // 2. If in backoff, sleep until retry_at or interrupt on command.
        let now = core.now();
        if !pacer.can_attempt(now) {
            if let Some(retry_at) = pacer.next_allowed_at() {
                let wait_ms = (retry_at.0 - now.0).max(250) as u64;
                let sleep = tokio::time::sleep(Duration::from_millis(wait_ms));
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => {}
                    cmd = rx.recv() => {
                        match cmd {
                            Some(ActorCommand::Reauthed) => pacer.reset_auth_breaker(),
                            Some(ActorCommand::SyncNow) => {}
                            Some(ActorCommand::Shutdown) | None => break,
                            Some(_) => continue,
                        }
                    }
                }
            }
        }

        // 3. Drain any coalesced commands without blocking.
        let mut priority_body: Option<MessageId> = None;
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                ActorCommand::Reauthed => pacer.reset_auth_breaker(),
                ActorCommand::FetchBody(mid) => priority_body = Some(mid),
                ActorCommand::Shutdown => {
                    if let Some(mut s) = session.take() {
                        let _ = s.logout().await;
                    }
                    return;
                }
                ActorCommand::SyncNow | ActorCommand::FlushActions => {}
            }
        }

        // 4. Execute one sync turn (fair-slice actions + LIST-STATUS delta + optional flag pass).
        match run_sync_turn(
            &core,
            account_id,
            &mut session,
            &backfill_sem,
            &body_sem,
            priority_body,
        )
        .await
        {
            Ok(primary_folder) => {
                pacer.record_success();
                // 5. Park in interruptible IMAP IDLE if we have a live session.
                if let Some(active) = session.take() {
                    match park_in_idle(active, &primary_folder, &mut rx).await {
                        IdleOutcome::Continue(returned) => {
                            session = Some(returned);
                        }
                        IdleOutcome::Shutdown => return,
                        IdleOutcome::Reauthed => {
                            pacer.reset_auth_breaker();
                            session = None;
                        }
                        IdleOutcome::Dropped => {
                            session = None;
                        }
                    }
                } else {
                    // Fallback wait if session couldn't be kept alive.
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                        cmd = rx.recv() => {
                            match cmd {
                                Some(ActorCommand::Shutdown) | None => return,
                                Some(ActorCommand::Reauthed) => pacer.reset_auth_breaker(),
                                Some(_) => {}
                            }
                        }
                    }
                }
            }
            Err(err) => {
                session = None;
                let now = core.now();
                let kind = match &err {
                    CoreError::AuthRequired { .. } => FailureKind::AuthFailure,
                    other => classify_error_message(&other.to_string(), false),
                };
                // Deterministic-enough jitter seed from current millis.
                let jitter = ((now.0.unsigned_abs() % 1000) as f64) / 1000.0;
                let state = pacer.record_failure(kind, err.to_string(), now, jitter);
                let _ = core
                    .inner
                    .store
                    .with_conn(|conn| store::set_sync_state(conn, account_id, &state));
                core.emit(CoreEvent::SyncChanged { account_id, state });
            }
        }
    }

    if let Some(mut s) = session.take() {
        let _ = s.logout().await;
    }
}

enum IdleOutcome {
    Continue(ImapSession),
    Reauthed,
    Dropped,
    Shutdown,
}

async fn park_in_idle(
    mut session: ImapSession,
    primary_folder: &str,
    rx: &mut mpsc::Receiver<ActorCommand>,
) -> IdleOutcome {
    if session.select(primary_folder).await.is_err() {
        return IdleOutcome::Dropped;
    }
    let mut idle = session.idle();
    if idle.init().await.is_err() {
        return IdleOutcome::Dropped;
    }
    let (idle_wait, interrupt) = idle.wait_with_timeout(IDLE_TIMEOUT);
    let mut shutdown = false;
    let mut reauthed = false;

    tokio::select! {
        _ = idle_wait => {}
        cmd = rx.recv() => {
            drop(interrupt);
            match cmd {
                Some(ActorCommand::Shutdown) | None => shutdown = true,
                Some(ActorCommand::Reauthed) => reauthed = true,
                Some(_) => {}
            }
        }
    }

    match idle.done().await {
        Ok(returned) => {
            if shutdown {
                let mut s = returned;
                let _ = s.logout().await;
                IdleOutcome::Shutdown
            } else if reauthed {
                let mut s = returned;
                let _ = s.logout().await;
                IdleOutcome::Reauthed
            } else {
                IdleOutcome::Continue(returned)
            }
        }
        Err(_) => {
            if shutdown {
                IdleOutcome::Shutdown
            } else if reauthed {
                IdleOutcome::Reauthed
            } else {
                IdleOutcome::Dropped
            }
        }
    }
}

async fn run_sync_turn(
    core: &MailCore,
    account_id: AccountId,
    session_slot: &mut Option<ImapSession>,
    backfill_sem: &Arc<Semaphore>,
    body_sem: &Arc<Semaphore>,
    priority_body: Option<MessageId>,
) -> Result<String, CoreError> {
    let now = core.now();
    let window_days = core.inner.config.sync_window_days;

    let account = core.account_row(account_id)?;
    let primary_folder = Dialect::from_auth(&account.auth)
        .primary_sync_folder()
        .to_string();
    let gmail_dialect = Dialect::from_auth(&account.auth) == Dialect::Gmail;

    let Some(secret) = core
        .resolve_transport_secret(account_id, &account.auth)
        .await?
    else {
        return Err(CoreError::AuthRequired {
            account_id,
            reason: "no saved credential in OS keychain".to_string(),
        });
    };

    let base_plan = imap::plan_for(
        account_id,
        &account.auth,
        secret.clone(),
        primary_folder.clone(),
        SyncWindow {
            since: now.sub_days(window_days),
            now,
            min_uid: 0,
        },
    );

    if session_slot.is_none() {
        core.emit(CoreEvent::SyncChanged {
            account_id,
            state: SyncState::Connecting,
        });
        *session_slot = Some(imap::open_session(&base_plan).await?);
    }
    let session = session_slot.as_mut().expect("session established");

    // 1. Fair-slice offline action queue (max 20 actions or 2s elapsed).
    let slice_start = Instant::now();
    let slice = core.inner.store.with_conn(|conn| {
        store::claim_action_slice(conn, account_id, now, store::DEFAULT_SLICE_MAX_ACTIONS)
    })?;
    for action_row in slice {
        if slice_start.elapsed() >= SLICE_MAX_DURATION {
            // Re-queue remaining claimed items immediately for the next slice.
            let _ = core.inner.store.with_conn(|conn| {
                store::retry_or_fail_offline_action(
                    conn,
                    action_row.id,
                    action_row.attempts.saturating_sub(1),
                    "yielded slice budget",
                    now,
                )
            });
            continue;
        }
        let Some(thread_id) = action_row.thread_id else {
            let _ = core
                .inner
                .store
                .with_conn(|conn| store::complete_offline_action(conn, action_row.id));
            continue;
        };
        let folder_uids = core
            .inner
            .store
            .with_conn(|conn| store::thread_imap_uids(conn, thread_id))?;
        let mut by_folder: HashMap<String, Vec<u32>> = HashMap::new();
        for (folder, uid) in folder_uids {
            by_folder.entry(folder).or_default().push(uid);
        }
        let mut failed_err: Option<String> = None;
        for (folder, uids) in by_folder {
            if let Err(err) = imap::apply_remote_action(
                session,
                account_id,
                &folder,
                &uids,
                &action_row.action,
                gmail_dialect,
            )
            .await
            {
                failed_err = Some(err.to_string());
                break;
            }
        }
        match failed_err {
            None => {
                let _ = core
                    .inner
                    .store
                    .with_conn(|conn| store::complete_offline_action(conn, action_row.id));
            }
            Some(reason) => {
                let _ = core.inner.store.with_conn(|conn| {
                    store::retry_or_fail_offline_action(
                        conn,
                        action_row.id,
                        action_row.attempts,
                        &reason,
                        now,
                    )
                });
            }
        }
    }

    // 2. LIST + STATUS delta check: only SELECT folders whose counts/UIDNEXT changed.
    let statuses = imap::list_folder_statuses(session, account_id, &primary_folder).await?;
    let mut all_touched = Vec::new();
    let mut did_sync_work = false;

    for status in statuses {
        let cursor = core
            .inner
            .store
            .with_conn(|conn| store::folder_cursor(conn, account_id, &status.name))?;

        let unchanged = cursor.as_ref().is_some_and(|c| {
            c.uid_validity == status.uid_validity
                && c.uid_next == status.uid_next
                && c.messages_count == status.messages
                && c.unseen_count == status.unseen
                && c.high_uid > 0
        });

        let needs_flag_pass = cursor
            .as_ref()
            .map(|c| now.0.saturating_sub(c.last_full_flag_pass.0) >= FLAG_PASS_INTERVAL_MS)
            .unwrap_or(false);

        if unchanged && !needs_flag_pass {
            continue;
        }

        if !did_sync_work {
            did_sync_work = true;
            let syncing_state = SyncState::Syncing {
                done: 0,
                total: None,
            };
            let _ = core
                .inner
                .store
                .with_conn(|conn| store::set_sync_state(conn, account_id, &syncing_state));
            core.emit(CoreEvent::SyncChanged {
                account_id,
                state: syncing_state,
            });
        }

        if !unchanged {
            let since = cursor
                .as_ref()
                .map(|c| c.window_floor)
                .unwrap_or_else(|| now.sub_days(window_days));
            let min_uid = match &cursor {
                Some(c) if c.uid_validity == status.uid_validity && c.uid_validity > 0 => {
                    c.high_uid
                }
                _ => 0,
            };
            let folder_plan = FetchPlan {
                folder: status.name.clone(),
                since,
                now,
                min_uid,
                ..base_plan.clone()
            };
            let fetched = imap::fetch_window_on_session(session, &folder_plan).await?;
            let touched = core.inner.store.with_conn(|conn| {
                store::update_folder_status(
                    conn,
                    account_id,
                    &store::FolderStatusUpdate {
                        folder_name: &status.name,
                        uid_validity: fetched.uid_validity,
                        uid_next: fetched.uid_next,
                        messages_count: fetched.messages_count,
                        unseen_count: fetched.unseen_count,
                    },
                    now,
                )?;
                let touched = store::ingest_messages(conn, account_id, fetched.messages, now)?;
                if fetched.high_uid > 0 {
                    store::update_folder_high_uid(
                        conn,
                        account_id,
                        &status.name,
                        fetched.uid_validity,
                        fetched.high_uid,
                    )?;
                }
                if fetched.low_uid > 0 && cursor.as_ref().map(|c| c.low_uid).unwrap_or(0) == 0 {
                    store::update_folder_backfill(
                        conn,
                        account_id,
                        &status.name,
                        fetched.low_uid,
                        fetched.low_uid <= 1,
                    )?;
                }
                Ok(touched)
            })?;
            if !touched.is_empty() {
                core.notify_hermes_ingested(account_id, &touched);
            }
            for tid in touched {
                if !all_touched.contains(&tid) {
                    all_touched.push(tid);
                }
            }
        }

        if needs_flag_pass {
            if let Ok(snapshot) = imap::fetch_folder_flags(session, account_id, &status.name).await
            {
                let touched = core.inner.store.with_conn(|conn| {
                    let t = store::sync_folder_seen_flags(
                        conn,
                        account_id,
                        &status.name,
                        &snapshot.seen_uids,
                        &snapshot.unseen_uids,
                    )?;
                    store::mark_folder_flag_pass(conn, account_id, &status.name, now)?;
                    Ok(t)
                })?;
                for tid in touched {
                    if !all_touched.contains(&tid) {
                        all_touched.push(tid);
                    }
                }
            }
        }
    }

    // 3. Kick off bounded background historical header backfill (separate connection).
    if let Ok(permit) = backfill_sem.clone().try_acquire_owned() {
        let cursor = core
            .inner
            .store
            .with_conn(|conn| store::folder_cursor(conn, account_id, &primary_folder))?;
        if let Some(c) = cursor {
            if !c.backfill_done && c.low_uid > 1 {
                let bg_core = core.clone();
                let bg_plan = base_plan.clone();
                let low_uid = c.low_uid as u32;
                tokio::spawn(async move {
                    let _permit = permit;
                    if let Ok((msgs, new_low, done)) =
                        imap::fetch_historical_headers(&bg_plan, low_uid, imap::HEADER_BACKFILL_BATCH)
                            .await
                    {
                        let bg_now = bg_core.now();
                        if let Ok(touched) = bg_core.inner.store.with_conn(|conn| {
                            let t = store::ingest_messages(conn, account_id, msgs, bg_now)?;
                            store::update_folder_backfill(
                                conn,
                                account_id,
                                &bg_plan.folder,
                                i64::from(new_low),
                                done,
                            )?;
                            Ok(t)
                        }) {
                            if !touched.is_empty() {
                                bg_core.emit(CoreEvent::MailChanged {
                                    account_id,
                                    thread_ids: touched,
                                });
                            }
                        }
                    }
                });
            }
        }
    }

    // 4. Kick off bounded priority/bulk body download for headers_only messages (separate connection).
    if let Ok(permit) = body_sem.clone().try_acquire_owned() {
        let pending = core
            .inner
            .store
            .with_conn(|conn| store::list_headers_only_messages(conn, account_id, 15))?;
        if !pending.is_empty() || priority_body.is_some() {
            let bg_core = core.clone();
            let bg_plan = base_plan.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let mut by_folder: HashMap<String, Vec<(MessageId, u32)>> = HashMap::new();
                if let Some(mid) = priority_body {
                    if let Ok(Some((folder, uid))) = bg_core
                        .inner
                        .store
                        .with_conn(|conn| store::message_imap_location(conn, mid))
                    {
                        by_folder.entry(folder).or_default().push((mid, uid));
                    }
                }
                for (mid, _tid, folder, uid) in pending {
                    let entry = by_folder.entry(folder).or_default();
                    if !entry.iter().any(|(m, _)| *m == mid) {
                        entry.push((mid, uid));
                    }
                }
                for (folder, pairs) in by_folder {
                    let uids: Vec<u32> = pairs.iter().map(|(_, u)| *u).collect();
                    let folder_plan = FetchPlan {
                        folder,
                        ..bg_plan.clone()
                    };
                    if let Ok(fetched) = imap::fetch_bodies_for_uids(&folder_plan, &uids).await {
                        let mut touched = Vec::new();
                        for msg in fetched {
                            if let Some(uid) = msg.imap_uid {
                                if let Some((mid, _)) = pairs.iter().find(|(_, u)| *u == uid) {
                                    if let Ok(Some(tid)) = bg_core.inner.store.with_conn(|conn| {
                                        store::upgrade_message_body(conn, account_id, *mid, &msg)
                                    }) {
                                        if !touched.contains(&tid) {
                                            touched.push(tid);
                                        }
                                    }
                                }
                            }
                        }
                        if !touched.is_empty() {
                            bg_core.emit(CoreEvent::MailChanged {
                                account_id,
                                thread_ids: touched,
                            });
                        }
                    }
                }
            });
        }
    }

    let idle_state = SyncState::Idle { last_ok: Some(now) };
    core.inner
        .store
        .with_conn(|conn| store::set_sync_state(conn, account_id, &idle_state))?;
    core.emit(CoreEvent::SyncChanged {
        account_id,
        state: idle_state,
    });
    if !all_touched.is_empty() {
        core.emit(CoreEvent::MailChanged {
            account_id,
            thread_ids: all_touched,
        });
    }

    Ok(primary_folder)
}
