//! `MailCore` facade methods for Phase B: Pigment Staff Sync & Email Collaboration.

use crate::auth::{SecretKey, SecretString};
use crate::error::CoreError;
use crate::events::CoreEvent;
use crate::ids::{
    CommentId, ContactId, PinnedLinkId, SharedTemplateId, SharedThreadId, StaffMemberId, ThreadId,
};
use crate::sanitize;
use crate::staff::{
    extract_mentions, normalize_handle, AddSharedCommentInput, ConnectStaffWorkspaceInput,
    DraftLockAction, DraftLockState, HandoffStatus, PinnedLinkView, SavePinnedLinkInput,
    SaveSharedTemplateInput, SaveShowContactInput, ShareThreadInput, ShareVisibility,
    SharedCommentView, SharedDraftView, SharedMessageSnapshot, SharedTemplateView,
    SharedThreadSummary, SharedThreadView, ShowContactView, StaffMemberView, StaffSyncStatus,
    StaffWorkspaceView, UpdateHandoffInput, UpdateSharedDraftInput,
};
use crate::store;
use crate::sync::staff::{StaffSyncClient, StaffSyncSnapshot};
use crate::types::{RemoteContent, SpaceKind};
use crate::MailCore;

impl MailCore {
    // ---------------------------------------------------------------- staff workspace

    pub async fn get_staff_workspace(&self) -> Result<StaffWorkspaceView, CoreError> {
        self.inner.store.with_conn(store::get_staff_workspace)
    }

    pub async fn connect_staff_workspace(
        &self,
        input: ConnectStaffWorkspaceInput,
    ) -> Result<StaffWorkspaceView, CoreError> {
        let server_url = input.server_url.trim().trim_end_matches('/').to_string();
        let valid_scheme = server_url.starts_with("https://")
            || server_url.starts_with("local://")
            || crate::sync::staff::is_loopback_http(&server_url);
        if !valid_scheme {
            return Err(CoreError::InvalidInput {
                field: "serverUrl".to_string(),
                reason: "Server URL must start with https://, local://, or http://localhost"
                    .to_string(),
            });
        }
        let token = input.token.trim().to_string();
        if token.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "token".to_string(),
                reason: "Staff invite or session token is required".to_string(),
            });
        }
        let staff_name = input.staff_name.trim().to_string();
        if staff_name.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "staffName".to_string(),
                reason: "Display name is required".to_string(),
            });
        }
        let staff_handle = normalize_handle(&input.staff_handle);
        if staff_handle.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "staffHandle".to_string(),
                reason: "A valid @handle (letters, numbers, -, _) is required".to_string(),
            });
        }
        let staff_email = input.staff_email.trim().to_ascii_lowercase();
        if !staff_email.contains('@') || staff_email.len() < 3 {
            return Err(CoreError::InvalidInput {
                field: "staffEmail".to_string(),
                reason: "A valid Pigment staff email address is required".to_string(),
            });
        }
        let staff_role = if input.staff_role.trim().is_empty() {
            "Pigment Staff".to_string()
        } else {
            input.staff_role.trim().to_string()
        };

        // Store token strictly in OS keychain (SecretStore), never in SQLite.
        self.inner
            .host
            .secrets
            .put(&SecretKey::staff_sync_token(), &SecretString::new(token.clone()))?;

        let now = self.now();
        let current_member = StaffMemberView {
            id: StaffMemberId::derive(
                uuid::Uuid::NAMESPACE_URL,
                format!("ferrule:staff:{staff_handle}").as_bytes(),
            ),
            name: staff_name.clone(),
            role: staff_role.clone(),
            handle: staff_handle.clone(),
            email: staff_email.clone(),
            department: None,
            updated_at: now,
        };

        if server_url.starts_with("local://") {
            let ws = self.inner.store.with_conn(|conn| {
                let ws = store::upsert_staff_workspace(
                    conn,
                    &store::StaffWorkspaceUpsert {
                        server_url: &server_url,
                        staff_name: &staff_name,
                        staff_handle: &staff_handle,
                        staff_email: &staff_email,
                        staff_role: &staff_role,
                        now,
                        status: StaffSyncStatus::Connected,
                        last_error: None,
                    },
                )?;
                store::seed_local_pigment_demo(conn, &current_member, now)?;
                store::seed_local_calendar_demo(conn, now)?;
                Ok(ws)
            })?;
            self.emit(CoreEvent::StaffChanged);
            return Ok(ws);
        }

        // Real HTTP/HTTPS Staff Sync server: persist local workspace first, then try initial pull.
        let client = StaffSyncClient::new(&server_url, &token)?;
        let pull_res = client.pull_snapshot(None).await;
        let ws = self.inner.store.with_conn(|conn| {
            let (status, err_msg) = match &pull_res {
                Ok(_) => (StaffSyncStatus::Connected, None),
                Err(err) => (StaffSyncStatus::Offline, Some(err.to_string())),
            };
            let ws = store::upsert_staff_workspace(
                conn,
                &store::StaffWorkspaceUpsert {
                    server_url: &server_url,
                    staff_name: &staff_name,
                    staff_handle: &staff_handle,
                    staff_email: &staff_email,
                    staff_role: &staff_role,
                    now,
                    status,
                    last_error: err_msg.as_deref(),
                },
            )?;
            store::upsert_staff_member(conn, &current_member)?;
            if let Ok(snapshot) = &pull_res {
                apply_snapshot_tx(conn, snapshot)?;
            }
            Ok(ws)
        })?;
        self.emit(CoreEvent::StaffChanged);
        Ok(ws)
    }

    pub async fn disconnect_staff_workspace(&self) -> Result<(), CoreError> {
        self.inner
            .host
            .secrets
            .delete(&SecretKey::staff_sync_token())?;
        self.inner.store.with_conn(store::delete_staff_workspace)?;
        self.emit(CoreEvent::StaffChanged);
        Ok(())
    }

    pub async fn sync_staff_workspace(&self) -> Result<StaffWorkspaceView, CoreError> {
        let ws = self.get_staff_workspace().await?;
        if !ws.connected {
            return Err(CoreError::IllegalState {
                reason: "Staff Sync workspace is not connected".to_string(),
            });
        }
        let now = self.now();
        if ws.server_url.starts_with("local://") {
            self.inner.store.with_conn(|conn| {
                store::update_staff_sync_status(conn, StaffSyncStatus::Connected, Some(now), None)
            })?;
            self.emit(CoreEvent::StaffChanged);
            return self.get_staff_workspace().await;
        }

        let Some(token) = self
            .inner
            .host
            .secrets
            .get(&SecretKey::staff_sync_token())?
        else {
            self.inner.store.with_conn(|conn| {
                store::update_staff_sync_status(
                    conn,
                    StaffSyncStatus::Error,
                    None,
                    Some("Staff Sync token missing from OS keychain. Reconnect in Settings."),
                )
            })?;
            return self.get_staff_workspace().await;
        };

        let client = StaffSyncClient::new(&ws.server_url, token.expose())?;
        match client.pull_snapshot(ws.last_sync_at).await {
            Ok(snapshot) => {
                self.inner.store.with_conn(|conn| {
                    apply_snapshot_tx(conn, &snapshot)?;
                    store::update_staff_sync_status(
                        conn,
                        StaffSyncStatus::Connected,
                        Some(snapshot.server_time.unwrap_or(now)),
                        None,
                    )
                })?;
            }
            Err(err) => {
                let reason = err.to_string();
                self.inner.store.with_conn(|conn| {
                    store::update_staff_sync_status(
                        conn,
                        StaffSyncStatus::Offline,
                        None,
                        Some(&reason),
                    )
                })?;
            }
        }
        self.emit(CoreEvent::StaffChanged);
        self.get_staff_workspace().await
    }

    pub async fn list_staff_directory(&self) -> Result<Vec<StaffMemberView>, CoreError> {
        self.inner.store.with_conn(store::list_staff_directory)
    }

    // ---------------------------------------------------------------- shared threads & handoff

    pub async fn list_shared_threads(&self) -> Result<Vec<SharedThreadSummary>, CoreError> {
        self.inner.store.with_conn(store::list_shared_threads)
    }

    pub async fn open_shared_thread(
        &self,
        shared_thread_id: SharedThreadId,
    ) -> Result<SharedThreadView, CoreError> {
        self.inner
            .store
            .with_conn(|conn| store::get_shared_thread(conn, shared_thread_id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "sharedThread".to_string(),
            })
    }

    pub async fn find_shared_thread_for_local(
        &self,
        thread_id: ThreadId,
    ) -> Result<Option<SharedThreadView>, CoreError> {
        self.inner.store.with_conn(|conn| {
            let Some(shared_id) = store::find_shared_thread_by_source(conn, thread_id)? else {
                return Ok(None);
            };
            store::get_shared_thread(conn, shared_id)
        })
    }

    /// Share a local thread from a Pigment Staff Space with teammates or All Staff.
    /// Personal Spaces are blocked by the personal-space guardrail.
    pub async fn share_thread(
        &self,
        input: ShareThreadInput,
    ) -> Result<SharedThreadView, CoreError> {
        let ws = self.require_staff_workspace()?;
        let now = self.now();

        let (shared, initial_comment) = self.inner.store.with_conn(|conn| {
            let thread = store::get_thread(conn, input.thread_id)?.ok_or(CoreError::NotFound {
                entity: "thread".to_string(),
            })?;
            let account =
                store::get_account(conn, thread.account_id)?.ok_or(CoreError::NotFound {
                    entity: "account".to_string(),
                })?;
            // Personal Space guardrail: never allow sharing threads from Personal Spaces.
            if account.space_kind != SpaceKind::PigmentStaff {
                return Err(CoreError::IllegalState {
                    reason: "Staff Sharing is disabled for Personal spaces. Switch this account to Pigment Staff to share threads.".to_string(),
                });
            }

            let messages = store::load_messages(conn, input.thread_id)?;
            let mut participants: Vec<String> = Vec::new();
            let mut snapshots = Vec::with_capacity(messages.len());
            for msg in &messages {
                if !msg.from_email.is_empty() && !participants.contains(&msg.from_email) {
                    participants.push(msg.from_email.clone());
                }
                for addr in msg.to.iter().chain(msg.cc.iter()) {
                    if !addr.is_empty() && !participants.contains(addr) {
                        participants.push(addr.clone());
                    }
                }
                let raw = match &msg.raw_html {
                    Some(html) => html.clone(),
                    None => sanitize::text_to_html(&msg.body_text),
                };
                // Sanitize with BlockAll before snapshot leaves the local mailbox.
                let doc = sanitize::sanitize_html(&raw, RemoteContent::BlockAll);
                snapshots.push(SharedMessageSnapshot {
                    id: msg.id.to_string(),
                    from_name: msg.from_name.clone(),
                    from_email: msg.from_email.clone(),
                    to: msg.to.clone(),
                    cc: msg.cc.clone(),
                    date: msg.date,
                    snippet: msg.snippet.clone(),
                    body_text: msg.body_text.clone(),
                    body_srcdoc: doc.srcdoc.into_inner(),
                });
            }

            let invited_handles: Vec<String> = match input.visibility {
                ShareVisibility::AllStaff => Vec::new(),
                ShareVisibility::Specific => {
                    let mut handles: Vec<String> = input
                        .invited_handles
                        .iter()
                        .map(|h| normalize_handle(h))
                        .filter(|h| !h.is_empty())
                        .collect();
                    handles.sort();
                    handles.dedup();
                    if handles.is_empty() {
                        return Err(CoreError::InvalidInput {
                            field: "invitedHandles".to_string(),
                            reason: "select at least one staff member or choose All Staff"
                                .to_string(),
                        });
                    }
                    handles
                }
            };

            let requested_assignee = input
                .assignee_handle
                .as_deref()
                .map(normalize_handle)
                .filter(|h| !h.is_empty());

            let existing = match store::find_shared_thread_by_source(conn, input.thread_id)? {
                Some(id) => store::get_shared_thread(conn, id)?,
                None => None,
            };
            let shared_id = existing
                .as_ref()
                .map(|t| t.id)
                .unwrap_or_else(SharedThreadId::fresh);
            let assignee_handle = requested_assignee
                .or_else(|| existing.as_ref().and_then(|t| t.assignee_handle.clone()));
            let status = existing
                .as_ref()
                .map(|t| t.status)
                .unwrap_or(HandoffStatus::Open);
            let shared_at = existing.as_ref().map(|t| t.shared_at).unwrap_or(now);

            let snippet = messages
                .last()
                .map(|m| m.snippet.clone())
                .unwrap_or_default();
            let view = SharedThreadView {
                id: shared_id,
                source_thread_id: Some(thread.id),
                source_account_id: Some(account.id),
                owner_email: account.email,
                owner_name: ws.staff_name.clone(),
                subject: thread.subject,
                snippet,
                participants,
                messages: snapshots,
                visibility: input.visibility,
                invited_handles,
                assignee_handle,
                status,
                comments: Vec::new(),
                draft: None,
                shared_at,
                updated_at: now,
            };
            store::upsert_shared_thread(conn, &view)?;

            let initial_comment = if let Some(note) = input
                .initial_note
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let comment = SharedCommentView {
                    id: CommentId::fresh(),
                    shared_thread_id: shared_id,
                    author_handle: ws.staff_handle.clone(),
                    author_name: ws.staff_name.clone(),
                    body_text: note.to_string(),
                    mentions: extract_mentions(note),
                    created_at: now,
                };
                store::insert_shared_comment(conn, &comment)?;
                Some(comment)
            } else {
                None
            };

            let full = store::get_shared_thread(conn, shared_id)?.ok_or(CoreError::Internal {
                reason: "shared thread disappeared after insert".to_string(),
            })?;
            Ok((full, initial_comment))
        })?;

        if let Some(client) = self.remote_staff_client(&ws)? {
            if client.push_shared_thread(&shared).await.is_err() {
                self.mark_staff_offline("Thread saved locally (offline)");
            } else if let Some(comment) = &initial_comment {
                let _ = client.push_comment(comment).await;
            }
        }

        self.emit(CoreEvent::StaffChanged);
        Ok(shared)
    }

    pub async fn update_shared_handoff(
        &self,
        input: UpdateHandoffInput,
    ) -> Result<SharedThreadView, CoreError> {
        let ws = self.require_staff_workspace()?;
        let now = self.now();
        let assignee = input
            .assignee_handle
            .as_deref()
            .map(normalize_handle)
            .filter(|h| !h.is_empty());

        let updated = self.inner.store.with_conn(|conn| {
            let ok = store::update_shared_handoff(
                conn,
                input.shared_thread_id,
                assignee.as_deref(),
                input.status,
                now,
            )?;
            if !ok {
                return Err(CoreError::NotFound {
                    entity: "sharedThread".to_string(),
                });
            }
            store::get_shared_thread(conn, input.shared_thread_id)?.ok_or(CoreError::NotFound {
                entity: "sharedThread".to_string(),
            })
        })?;

        if let Some(client) = self.remote_staff_client(&ws)? {
            if client
                .push_handoff(input.shared_thread_id, assignee.as_deref(), input.status)
                .await
                .is_err()
            {
                self.mark_staff_offline("Handoff saved locally (offline)");
            }
        }

        self.emit(CoreEvent::StaffChanged);
        Ok(updated)
    }

    pub async fn add_shared_comment(
        &self,
        input: AddSharedCommentInput,
    ) -> Result<SharedCommentView, CoreError> {
        let ws = self.require_staff_workspace()?;
        let body = input.body_text.trim();
        if body.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "bodyText".to_string(),
                reason: "comment cannot be empty".to_string(),
            });
        }
        let now = self.now();
        let comment = SharedCommentView {
            id: CommentId::fresh(),
            shared_thread_id: input.shared_thread_id,
            author_handle: ws.staff_handle.clone(),
            author_name: ws.staff_name.clone(),
            body_text: body.to_string(),
            mentions: extract_mentions(body),
            created_at: now,
        };

        self.inner.store.with_conn(|conn| {
            if store::get_shared_thread(conn, input.shared_thread_id)?.is_none() {
                return Err(CoreError::NotFound {
                    entity: "sharedThread".to_string(),
                });
            }
            store::insert_shared_comment(conn, &comment)
        })?;

        if let Some(client) = self.remote_staff_client(&ws)? {
            if client.push_comment(&comment).await.is_err() {
                self.mark_staff_offline("Comment saved locally (offline)");
            }
        }

        self.emit(CoreEvent::StaffChanged);
        Ok(comment)
    }

    // ---------------------------------------------------------------- draft lock & handoff

    pub async fn update_shared_draft(
        &self,
        input: UpdateSharedDraftInput,
    ) -> Result<SharedDraftView, CoreError> {
        let ws = self.require_staff_workspace()?;
        let now = self.now();

        let (draft, auto_comment) = self.inner.store.with_conn(|conn| {
            let thread =
                store::get_shared_thread(conn, input.shared_thread_id)?.ok_or(CoreError::NotFound {
                    entity: "sharedThread".to_string(),
                })?;

            let default_subject = if thread.subject.to_ascii_lowercase().starts_with("re:") {
                thread.subject.clone()
            } else {
                format!("Re: {}", thread.subject)
            };
            let default_to = thread
                .messages
                .last()
                .map(|m| vec![m.from_email.clone()])
                .unwrap_or_default();

            let mut draft = thread.draft.unwrap_or(SharedDraftView {
                shared_thread_id: input.shared_thread_id,
                subject: default_subject,
                to: default_to,
                cc: Vec::new(),
                body_text: String::new(),
                editor_handle: None,
                editor_name: None,
                lock_state: DraftLockState::Unlocked,
                review_requested_by: None,
                approved_by: None,
                approved_at: None,
                rev: 1,
                updated_at: now,
            });

            let mut auto_comment: Option<SharedCommentView> = None;

            match input.action {
                DraftLockAction::Claim => {
                    if draft.lock_state == DraftLockState::Editing
                        && draft.editor_handle.as_deref() != Some(ws.staff_handle.as_str())
                    {
                        return Err(CoreError::Conflict {
                            current_rev: draft.rev,
                        });
                    }
                    draft.lock_state = DraftLockState::Editing;
                    draft.editor_handle = Some(ws.staff_handle.clone());
                    draft.editor_name = Some(ws.staff_name.clone());
                    draft.rev = draft.rev.saturating_add(1);
                    draft.updated_at = now;
                }
                DraftLockAction::TakeOver => {
                    draft.lock_state = DraftLockState::Editing;
                    draft.editor_handle = Some(ws.staff_handle.clone());
                    draft.editor_name = Some(ws.staff_name.clone());
                    draft.approved_by = None;
                    draft.approved_at = None;
                    draft.rev = draft.rev.saturating_add(1);
                    draft.updated_at = now;
                }
                DraftLockAction::Save {
                    subject,
                    to,
                    cc,
                    body_text,
                } => {
                    if draft.lock_state != DraftLockState::Editing
                        || draft.editor_handle.as_deref() != Some(ws.staff_handle.as_str())
                    {
                        return Err(CoreError::Conflict {
                            current_rev: draft.rev,
                        });
                    }
                    draft.subject = subject;
                    draft.to = to;
                    draft.cc = cc;
                    draft.body_text = body_text;
                    draft.rev = draft.rev.saturating_add(1);
                    draft.updated_at = now;
                }
                DraftLockAction::RequestReview {
                    reviewer_handles,
                    note,
                } => {
                    if draft.lock_state == DraftLockState::Editing
                        && draft.editor_handle.as_deref() != Some(ws.staff_handle.as_str())
                    {
                        return Err(CoreError::Conflict {
                            current_rev: draft.rev,
                        });
                    }
                    draft.lock_state = DraftLockState::ReviewRequested;
                    draft.editor_handle = None;
                    draft.editor_name = None;
                    draft.review_requested_by = Some(ws.staff_handle.clone());
                    draft.approved_by = None;
                    draft.approved_at = None;
                    draft.rev = draft.rev.saturating_add(1);
                    draft.updated_at = now;

                    let mentions: Vec<String> = reviewer_handles
                        .iter()
                        .map(|h| normalize_handle(h))
                        .filter(|h| !h.is_empty())
                        .collect();
                    let ping = if mentions.is_empty() {
                        "Requested review on the shared reply draft.".to_string()
                    } else {
                        let tags = mentions
                            .iter()
                            .map(|h| format!("@{h}"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        format!("Requested review on the shared reply draft ({tags}).")
                    };
                    let full_note = match note.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                        Some(extra) => format!("{ping} {extra}"),
                        None => ping,
                    };
                    let comment = SharedCommentView {
                        id: CommentId::fresh(),
                        shared_thread_id: input.shared_thread_id,
                        author_handle: ws.staff_handle.clone(),
                        author_name: ws.staff_name.clone(),
                        body_text: full_note.clone(),
                        mentions: extract_mentions(&full_note),
                        created_at: now,
                    };
                    store::insert_shared_comment(conn, &comment)?;
                    auto_comment = Some(comment);
                }
                DraftLockAction::Approve => {
                    if draft.lock_state == DraftLockState::Editing {
                        if let Some(holder) = draft.editor_handle.as_deref() {
                            if holder != ws.staff_handle {
                                let label = draft.editor_name.as_deref().unwrap_or(holder);
                                return Err(CoreError::IllegalState {
                                    reason: format!(
                                        "{label} is currently editing this draft. Request review or take over before approving."
                                    ),
                                });
                            }
                        }
                    }
                    if draft.body_text.trim().is_empty() {
                        return Err(CoreError::IllegalState {
                            reason: "Cannot approve an empty draft.".to_string(),
                        });
                    }
                    draft.lock_state = DraftLockState::Approved;
                    draft.editor_handle = None;
                    draft.editor_name = None;
                    draft.approved_by = Some(ws.staff_handle.clone());
                    draft.approved_at = Some(now);
                    draft.rev = draft.rev.saturating_add(1);
                    draft.updated_at = now;
                }
                DraftLockAction::Release => {
                    draft.lock_state = DraftLockState::Unlocked;
                    draft.editor_handle = None;
                    draft.editor_name = None;
                    draft.rev = draft.rev.saturating_add(1);
                    draft.updated_at = now;
                }
            }

            store::upsert_shared_draft(conn, &draft)?;
            Ok((draft, auto_comment))
        })?;

        if let Some(client) = self.remote_staff_client(&ws)? {
            if client.push_draft(&draft).await.is_err() {
                self.mark_staff_offline("Draft saved locally (offline)");
            } else if let Some(comment) = &auto_comment {
                let _ = client.push_comment(comment).await;
            }
        }

        self.emit(CoreEvent::StaffChanged);
        Ok(draft)
    }

    // ---------------------------------------------------------------- shared templates

    pub async fn list_shared_templates(&self) -> Result<Vec<SharedTemplateView>, CoreError> {
        self.inner.store.with_conn(store::list_shared_templates)
    }

    pub async fn save_shared_template(
        &self,
        input: SaveSharedTemplateInput,
    ) -> Result<SharedTemplateView, CoreError> {
        let ws = self.get_staff_workspace().await?;
        let actor = if ws.connected && !ws.staff_handle.is_empty() {
            ws.staff_handle.clone()
        } else {
            "staff".to_string()
        };
        let now = self.now();
        let saved = self
            .inner
            .store
            .with_conn(|conn| store::save_shared_template(conn, &input, &actor, now))?;
        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.push_shared_template(&saved).await;
            }
        }
        self.emit(CoreEvent::StaffChanged);
        Ok(saved)
    }

    pub async fn delete_shared_template(&self, id: SharedTemplateId) -> Result<(), CoreError> {
        let removed = self
            .inner
            .store
            .with_conn(|conn| store::delete_shared_template(conn, id))?;
        if !removed {
            return Err(CoreError::NotFound {
                entity: "sharedTemplate".to_string(),
            });
        }
        let ws = self.get_staff_workspace().await?;
        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.delete_shared_template(id).await;
            }
        }
        self.emit(CoreEvent::StaffChanged);
        Ok(())
    }

    // ---------------------------------------------------------------- show contacts

    pub async fn list_show_contacts(&self) -> Result<Vec<ShowContactView>, CoreError> {
        self.inner.store.with_conn(store::list_show_contacts)
    }

    pub async fn find_contact_by_email(
        &self,
        email: String,
    ) -> Result<Option<ShowContactView>, CoreError> {
        self.inner
            .store
            .with_conn(|conn| store::find_contact_by_email(conn, &email))
    }

    pub async fn save_show_contact(
        &self,
        input: SaveShowContactInput,
    ) -> Result<ShowContactView, CoreError> {
        let now = self.now();
        let saved = self
            .inner
            .store
            .with_conn(|conn| store::save_show_contact(conn, &input, now))?;
        let ws = self.get_staff_workspace().await?;
        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.push_contact(&saved).await;
            }
        }
        self.emit(CoreEvent::StaffChanged);
        Ok(saved)
    }

    pub async fn delete_show_contact(&self, id: ContactId) -> Result<(), CoreError> {
        let removed = self
            .inner
            .store
            .with_conn(|conn| store::delete_show_contact(conn, id))?;
        if !removed {
            return Err(CoreError::NotFound {
                entity: "showContact".to_string(),
            });
        }
        let ws = self.get_staff_workspace().await?;
        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.delete_contact(id).await;
            }
        }
        self.emit(CoreEvent::StaffChanged);
        Ok(())
    }

    // ---------------------------------------------------------------- pinned links

    pub async fn list_pinned_links(&self) -> Result<Vec<PinnedLinkView>, CoreError> {
        self.inner.store.with_conn(store::list_pinned_links)
    }

    pub async fn save_pinned_link(
        &self,
        input: SavePinnedLinkInput,
    ) -> Result<PinnedLinkView, CoreError> {
        let now = self.now();
        let saved = self
            .inner
            .store
            .with_conn(|conn| store::save_pinned_link(conn, &input, now))?;
        let ws = self.get_staff_workspace().await?;
        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.push_pinned_link(&saved).await;
            }
        }
        self.emit(CoreEvent::StaffChanged);
        Ok(saved)
    }

    pub async fn delete_pinned_link(&self, id: PinnedLinkId) -> Result<(), CoreError> {
        let removed = self
            .inner
            .store
            .with_conn(|conn| store::delete_pinned_link(conn, id))?;
        if !removed {
            return Err(CoreError::NotFound {
                entity: "pinnedLink".to_string(),
            });
        }
        let ws = self.get_staff_workspace().await?;
        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.delete_pinned_link(id).await;
            }
        }
        self.emit(CoreEvent::StaffChanged);
        Ok(())
    }

    /// Safely open a pinned staff link or external URL in the system browser / Discord.
    pub fn open_external_url(&self, url: &str) -> Result<(), CoreError> {
        let trimmed = url.trim();
        let allowed = trimmed.starts_with("https://")
            || trimmed.starts_with("http://")
            || trimmed.starts_with("discord://");
        if !allowed {
            return Err(CoreError::InvalidInput {
                field: "url".to_string(),
                reason: "only https://, http://, and discord:// links can be opened".to_string(),
            });
        }
        open::that(trimmed).map_err(|e| CoreError::Internal {
            reason: format!("failed to open link: {e}"),
        })
    }

    // ---------------------------------------------------------------- internal helpers

    fn require_staff_workspace(&self) -> Result<StaffWorkspaceView, CoreError> {
        let ws = self.inner.store.with_conn(store::get_staff_workspace)?;
        if !ws.connected {
            return Err(CoreError::IllegalState {
                reason: "Connect to Pigment Staff Sync in Settings first.".to_string(),
            });
        }
        Ok(ws)
    }

    pub(crate) fn remote_staff_client(
        &self,
        ws: &StaffWorkspaceView,
    ) -> Result<Option<StaffSyncClient>, CoreError> {
        if !ws.connected || ws.server_url.starts_with("local://") {
            return Ok(None);
        }
        let Some(token) = self
            .inner
            .host
            .secrets
            .get(&SecretKey::staff_sync_token())?
        else {
            return Ok(None);
        };
        Ok(Some(StaffSyncClient::new(&ws.server_url, token.expose())?))
    }

    pub(crate) fn mark_staff_offline(&self, reason: &str) {
        let _ = self.inner.store.with_conn(|conn| {
            store::update_staff_sync_status(conn, StaffSyncStatus::Offline, None, Some(reason))
        });
    }
}

fn apply_snapshot_tx(
    conn: &mut rusqlite::Connection,
    snapshot: &StaffSyncSnapshot,
) -> Result<(), CoreError> {
    for member in &snapshot.members {
        store::upsert_staff_member(conn, member)?;
    }
    for thread in &snapshot.shared_threads {
        store::upsert_shared_thread(conn, thread)?;
        for comment in &thread.comments {
            store::insert_shared_comment(conn, comment)?;
        }
        if let Some(draft) = &thread.draft {
            store::upsert_shared_draft(conn, draft)?;
        }
    }
    for tpl in &snapshot.shared_templates {
        store::upsert_shared_template_row(conn, tpl)?;
    }
    for contact in &snapshot.contacts {
        store::upsert_show_contact_row(conn, contact)?;
    }
    for link in &snapshot.pinned_links {
        store::upsert_pinned_link_row(conn, link)?;
    }
    for event in &snapshot.calendar_events {
        store::upsert_calendar_event_row(conn, event)?;
    }
    for doc in &snapshot.meeting_docs {
        store::upsert_meeting_doc_row(conn, doc)?;
    }
    for item in &snapshot.action_items {
        store::upsert_action_item_row(conn, item)?;
    }
    Ok(())
}
