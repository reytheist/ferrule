//! Local-first SQLite persistence for Phase B Staff Sync & Collaboration.

use rusqlite::{params, Connection, Row};

use crate::error::CoreError;
use crate::ids::{
    AccountId, CommentId, ContactId, PinnedLinkId, SharedTemplateId, SharedThreadId,
    StaffMemberId, ThreadId, UnixMs,
};
use crate::staff::{
    normalize_handle, ContactCategory, DraftLockState, HandoffStatus, PinnedLinkView,
    SavePinnedLinkInput, SaveSharedTemplateInput, SaveShowContactInput, ShareVisibility,
    SharedCommentView, SharedDraftView, SharedMessageSnapshot, SharedTemplateView,
    SharedThreadSummary, SharedThreadView, ShowContactView, StaffMemberView, StaffSyncStatus,
    StaffWorkspaceView,
};
use crate::store::{
    blob_to_id, from_json, id_to_blob, query_all, query_one, row_id, to_json, SqlResultExt,
};

// ---------------------------------------------------------------- workspace

pub(crate) fn get_staff_workspace(conn: &mut Connection) -> Result<StaffWorkspaceView, CoreError> {
    let pending_outbox_count = count_staff_outbox(conn)?.min(u32::MAX as usize) as u32;
    let row = query_one(
        conn,
        "SELECT server_url, staff_name, staff_handle, staff_email, staff_role,
                connected_at, last_sync_at, sync_status, last_error
         FROM staff_workspace WHERE id = 1",
        [],
        |r| {
            Ok(StaffWorkspaceView {
                connected: true,
                server_url: r.get(0).sq()?,
                staff_name: r.get(1).sq()?,
                staff_handle: r.get(2).sq()?,
                staff_email: r.get(3).sq()?,
                staff_role: r.get(4).sq()?,
                connected_at: Some(UnixMs(r.get(5).sq()?)),
                last_sync_at: r.get::<_, Option<i64>>(6).sq()?.map(UnixMs),
                status: StaffSyncStatus::from_db(&r.get::<_, String>(7).sq()?),
                last_error: r.get(8).sq()?,
                pending_outbox_count,
            })
        },
    )?;
    Ok(row.unwrap_or(StaffWorkspaceView {
        pending_outbox_count,
        ..StaffWorkspaceView::default()
    }))
}

pub(crate) struct StaffWorkspaceUpsert<'a> {
    pub server_url: &'a str,
    pub staff_name: &'a str,
    pub staff_handle: &'a str,
    pub staff_email: &'a str,
    pub staff_role: &'a str,
    pub now: UnixMs,
    pub status: StaffSyncStatus,
    pub last_error: Option<&'a str>,
}

pub(crate) fn upsert_staff_workspace(
    conn: &mut Connection,
    input: &StaffWorkspaceUpsert<'_>,
) -> Result<StaffWorkspaceView, CoreError> {
    conn.execute(
        "INSERT INTO staff_workspace (
            id, server_url, staff_name, staff_handle, staff_email, staff_role,
            connected_at, last_sync_at, sync_status, last_error
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            server_url = excluded.server_url,
            staff_name = excluded.staff_name,
            staff_handle = excluded.staff_handle,
            staff_email = excluded.staff_email,
            staff_role = excluded.staff_role,
            last_sync_at = excluded.last_sync_at,
            sync_status = excluded.sync_status,
            last_error = excluded.last_error",
        params![
            input.server_url,
            input.staff_name,
            input.staff_handle,
            input.staff_email,
            input.staff_role,
            input.now.0,
            if matches!(input.status, StaffSyncStatus::Connected) {
                Some(input.now.0)
            } else {
                None
            },
            input.status.as_str(),
            input.last_error,
        ],
    )
    .sq()?;
    get_staff_workspace(conn)
}

pub(crate) fn update_staff_sync_status(
    conn: &mut Connection,
    status: StaffSyncStatus,
    last_sync_at: Option<UnixMs>,
    last_error: Option<&str>,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE staff_workspace
         SET sync_status = ?1,
             last_sync_at = COALESCE(?2, last_sync_at),
             last_error = ?3
         WHERE id = 1",
        params![status.as_str(), last_sync_at.map(|t| t.0), last_error],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn delete_staff_workspace(conn: &mut Connection) -> Result<(), CoreError> {
    conn.execute("DELETE FROM staff_workspace WHERE id = 1", [])
        .sq()?;
    Ok(())
}

// ---------------------------------------------------------------- directory

fn map_member(row: &Row<'_>) -> Result<StaffMemberView, CoreError> {
    Ok(StaffMemberView {
        id: row_id(row, 0, StaffMemberId::from_bytes)?,
        name: row.get(1).sq()?,
        role: row.get(2).sq()?,
        handle: row.get(3).sq()?,
        email: row.get(4).sq()?,
        department: row.get(5).sq()?,
        updated_at: UnixMs(row.get(6).sq()?),
    })
}

pub(crate) fn list_staff_directory(conn: &mut Connection) -> Result<Vec<StaffMemberView>, CoreError> {
    query_all(
        conn,
        "SELECT id, name, role, handle, email, department, updated_at
         FROM staff_member
         ORDER BY name ASC, handle ASC",
        [],
        map_member,
    )
}

pub(crate) fn upsert_staff_member(
    conn: &mut Connection,
    member: &StaffMemberView,
) -> Result<(), CoreError> {
    let handle = normalize_handle(&member.handle);
    conn.execute(
        "INSERT INTO staff_member (id, name, role, handle, email, department, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(handle) DO UPDATE SET
            name = excluded.name,
            role = excluded.role,
            email = excluded.email,
            department = excluded.department,
            updated_at = excluded.updated_at",
        params![
            id_to_blob(member.id.as_bytes()),
            member.name.trim(),
            member.role.trim(),
            handle,
            member.email.trim().to_ascii_lowercase(),
            member.department.as_deref().map(str::trim),
            member.updated_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

// ---------------------------------------------------------------- shared threads

fn opt_row_id<T>(
    row: &Row<'_>,
    idx: usize,
    map: impl FnOnce([u8; 16]) -> T,
) -> Result<Option<T>, CoreError> {
    let bytes: Option<Vec<u8>> = row.get(idx).sq()?;
    match bytes {
        Some(raw) => Ok(Some(blob_to_id(&raw, map)?)),
        None => Ok(None),
    }
}

pub(crate) fn upsert_shared_thread(
    conn: &mut Connection,
    thread: &SharedThreadView,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO shared_thread (
            id, source_thread_id, source_account_id, owner_email, owner_name,
            subject, snippet, participants_json, messages_json, visibility,
            invited_handles_json, assignee_handle, handoff_status, shared_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
            owner_email = excluded.owner_email,
            owner_name = excluded.owner_name,
            subject = excluded.subject,
            snippet = excluded.snippet,
            participants_json = excluded.participants_json,
            messages_json = excluded.messages_json,
            visibility = excluded.visibility,
            invited_handles_json = excluded.invited_handles_json,
            assignee_handle = excluded.assignee_handle,
            handoff_status = excluded.handoff_status,
            updated_at = excluded.updated_at",
        params![
            id_to_blob(thread.id.as_bytes()),
            thread.source_thread_id.map(|id| id_to_blob(id.as_bytes())),
            thread.source_account_id.map(|id| id_to_blob(id.as_bytes())),
            thread.owner_email,
            thread.owner_name,
            thread.subject,
            thread.snippet,
            to_json(&thread.participants)?,
            to_json(&thread.messages)?,
            thread.visibility.as_str(),
            to_json(&thread.invited_handles)?,
            thread.assignee_handle,
            thread.status.as_str(),
            thread.shared_at.0,
            thread.updated_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn find_shared_thread_by_source(
    conn: &mut Connection,
    source_thread_id: ThreadId,
) -> Result<Option<SharedThreadId>, CoreError> {
    query_one(
        conn,
        "SELECT id FROM shared_thread WHERE source_thread_id = ?1 LIMIT 1",
        params![id_to_blob(source_thread_id.as_bytes())],
        |r| row_id(r, 0, SharedThreadId::from_bytes),
    )
}

pub(crate) fn list_shared_threads(
    conn: &mut Connection,
) -> Result<Vec<SharedThreadSummary>, CoreError> {
    query_all(
        conn,
        "SELECT st.id, st.source_thread_id, st.source_account_id, st.owner_email, st.owner_name,
                st.subject, st.snippet, st.participants_json, st.messages_json, st.visibility,
                st.invited_handles_json, st.assignee_handle, st.handoff_status, st.shared_at, st.updated_at,
                (SELECT COUNT(*) FROM shared_comment c WHERE c.shared_thread_id = st.id) AS comment_count,
                d.lock_state, d.editor_name
         FROM shared_thread st
         LEFT JOIN shared_draft d ON d.shared_thread_id = st.id
         ORDER BY st.updated_at DESC, st.shared_at DESC",
        [],
        |r| {
            let participants: Vec<String> = from_json(&r.get::<_, String>(7).sq()?)?;
            let messages: Vec<SharedMessageSnapshot> = from_json(&r.get::<_, String>(8).sq()?)?;
            let visibility = ShareVisibility::from_db(&r.get::<_, String>(9).sq()?);
            let invited_handles: Vec<String> = from_json(&r.get::<_, String>(10).sq()?)?;
            let status = HandoffStatus::from_db(&r.get::<_, String>(12).sq()?);
            let comment_count: i64 = r.get(15).sq()?;
            let draft_lock_state = r
                .get::<_, Option<String>>(16)
                .sq()?
                .map(|s| DraftLockState::from_db(&s))
                .unwrap_or(DraftLockState::Unlocked);
            let draft_editor_name: Option<String> = r.get(17).sq()?;
            Ok(SharedThreadSummary {
                id: row_id(r, 0, SharedThreadId::from_bytes)?,
                source_thread_id: opt_row_id(r, 1, ThreadId::from_bytes)?,
                source_account_id: opt_row_id(r, 2, AccountId::from_bytes)?,
                owner_email: r.get(3).sq()?,
                owner_name: r.get(4).sq()?,
                subject: r.get(5).sq()?,
                snippet: r.get(6).sq()?,
                participants,
                message_count: messages.len() as u32,
                visibility,
                invited_handles,
                assignee_handle: r.get(11).sq()?,
                status,
                comment_count: comment_count.max(0) as u32,
                draft_lock_state,
                draft_editor_name,
                shared_at: UnixMs(r.get(13).sq()?),
                updated_at: UnixMs(r.get(14).sq()?),
            })
        },
    )
}

pub(crate) fn get_shared_thread(
    conn: &mut Connection,
    id: SharedThreadId,
) -> Result<Option<SharedThreadView>, CoreError> {
    let base = query_one(
        conn,
        "SELECT id, source_thread_id, source_account_id, owner_email, owner_name,
                subject, snippet, participants_json, messages_json, visibility,
                invited_handles_json, assignee_handle, handoff_status, shared_at, updated_at
         FROM shared_thread WHERE id = ?1",
        params![id_to_blob(id.as_bytes())],
        |r| {
            let participants: Vec<String> = from_json(&r.get::<_, String>(7).sq()?)?;
            let messages: Vec<SharedMessageSnapshot> = from_json(&r.get::<_, String>(8).sq()?)?;
            let visibility = ShareVisibility::from_db(&r.get::<_, String>(9).sq()?);
            let invited_handles: Vec<String> = from_json(&r.get::<_, String>(10).sq()?)?;
            let status = HandoffStatus::from_db(&r.get::<_, String>(12).sq()?);
            Ok(SharedThreadView {
                id: row_id(r, 0, SharedThreadId::from_bytes)?,
                source_thread_id: opt_row_id(r, 1, ThreadId::from_bytes)?,
                source_account_id: opt_row_id(r, 2, AccountId::from_bytes)?,
                owner_email: r.get(3).sq()?,
                owner_name: r.get(4).sq()?,
                subject: r.get(5).sq()?,
                snippet: r.get(6).sq()?,
                participants,
                messages,
                visibility,
                invited_handles,
                assignee_handle: r.get(11).sq()?,
                status,
                comments: Vec::new(),
                draft: None,
                shared_at: UnixMs(r.get(13).sq()?),
                updated_at: UnixMs(r.get(14).sq()?),
            })
        },
    )?;

    let Some(mut view) = base else {
        return Ok(None);
    };
    view.comments = list_shared_comments(conn, id)?;
    view.draft = get_shared_draft(conn, id)?;
    Ok(Some(view))
}

pub(crate) fn update_shared_handoff(
    conn: &mut Connection,
    id: SharedThreadId,
    assignee_handle: Option<&str>,
    status: HandoffStatus,
    now: UnixMs,
) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "UPDATE shared_thread
             SET assignee_handle = ?2, handoff_status = ?3, updated_at = ?4
             WHERE id = ?1",
            params![
                id_to_blob(id.as_bytes()),
                assignee_handle,
                status.as_str(),
                now.0,
            ],
        )
        .sq()?;
    Ok(changed > 0)
}

// ---------------------------------------------------------------- comments

pub(crate) fn list_shared_comments(
    conn: &mut Connection,
    shared_thread_id: SharedThreadId,
) -> Result<Vec<SharedCommentView>, CoreError> {
    query_all(
        conn,
        "SELECT id, shared_thread_id, author_handle, author_name, body_text, mentions_json, created_at
         FROM shared_comment
         WHERE shared_thread_id = ?1
         ORDER BY created_at ASC, rowid ASC",
        params![id_to_blob(shared_thread_id.as_bytes())],
        |r| {
            let mentions: Vec<String> = from_json(&r.get::<_, String>(5).sq()?)?;
            Ok(SharedCommentView {
                id: row_id(r, 0, CommentId::from_bytes)?,
                shared_thread_id: row_id(r, 1, SharedThreadId::from_bytes)?,
                author_handle: r.get(2).sq()?,
                author_name: r.get(3).sq()?,
                body_text: r.get(4).sq()?,
                mentions,
                created_at: UnixMs(r.get(6).sq()?),
            })
        },
    )
}

pub(crate) fn insert_shared_comment(
    conn: &mut Connection,
    comment: &SharedCommentView,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO shared_comment (
            id, shared_thread_id, author_handle, author_name, body_text, mentions_json, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id_to_blob(comment.id.as_bytes()),
            id_to_blob(comment.shared_thread_id.as_bytes()),
            comment.author_handle,
            comment.author_name,
            comment.body_text,
            to_json(&comment.mentions)?,
            comment.created_at.0,
        ],
    )
    .sq()?;
    conn.execute(
        "UPDATE shared_thread SET updated_at = ?2 WHERE id = ?1",
        params![
            id_to_blob(comment.shared_thread_id.as_bytes()),
            comment.created_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

// ---------------------------------------------------------------- shared draft

pub(crate) fn get_shared_draft(
    conn: &mut Connection,
    shared_thread_id: SharedThreadId,
) -> Result<Option<SharedDraftView>, CoreError> {
    query_one(
        conn,
        "SELECT shared_thread_id, subject, to_json, cc_json, body_text,
                editor_handle, editor_name, lock_state, review_requested_by,
                approved_by, approved_at, rev, updated_at
         FROM shared_draft WHERE shared_thread_id = ?1",
        params![id_to_blob(shared_thread_id.as_bytes())],
        |r| {
            let to: Vec<String> = from_json(&r.get::<_, String>(2).sq()?)?;
            let cc: Vec<String> = from_json(&r.get::<_, String>(3).sq()?)?;
            Ok(SharedDraftView {
                shared_thread_id: row_id(r, 0, SharedThreadId::from_bytes)?,
                subject: r.get(1).sq()?,
                to,
                cc,
                body_text: r.get(4).sq()?,
                editor_handle: r.get(5).sq()?,
                editor_name: r.get(6).sq()?,
                lock_state: DraftLockState::from_db(&r.get::<_, String>(7).sq()?),
                review_requested_by: r.get(8).sq()?,
                approved_by: r.get(9).sq()?,
                approved_at: r.get::<_, Option<i64>>(10).sq()?.map(UnixMs),
                rev: r.get::<_, i64>(11).sq()?.max(1) as u32,
                updated_at: UnixMs(r.get(12).sq()?),
            })
        },
    )
}

pub(crate) fn upsert_shared_draft(
    conn: &mut Connection,
    draft: &SharedDraftView,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO shared_draft (
            shared_thread_id, subject, to_json, cc_json, body_text,
            editor_handle, editor_name, lock_state, review_requested_by,
            approved_by, approved_at, rev, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(shared_thread_id) DO UPDATE SET
            subject = excluded.subject,
            to_json = excluded.to_json,
            cc_json = excluded.cc_json,
            body_text = excluded.body_text,
            editor_handle = excluded.editor_handle,
            editor_name = excluded.editor_name,
            lock_state = excluded.lock_state,
            review_requested_by = excluded.review_requested_by,
            approved_by = excluded.approved_by,
            approved_at = excluded.approved_at,
            rev = excluded.rev,
            updated_at = excluded.updated_at
         WHERE excluded.rev > shared_draft.rev
            OR (excluded.rev = shared_draft.rev AND excluded.updated_at >= shared_draft.updated_at)",
        params![
            id_to_blob(draft.shared_thread_id.as_bytes()),
            draft.subject,
            to_json(&draft.to)?,
            to_json(&draft.cc)?,
            draft.body_text,
            draft.editor_handle,
            draft.editor_name,
            draft.lock_state.as_str(),
            draft.review_requested_by,
            draft.approved_by,
            draft.approved_at.map(|t| t.0),
            draft.rev as i64,
            draft.updated_at.0,
        ],
    )
    .sq()?;
    conn.execute(
        "UPDATE shared_thread SET updated_at = ?2 WHERE id = ?1",
        params![
            id_to_blob(draft.shared_thread_id.as_bytes()),
            draft.updated_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

// ---------------------------------------------------------------- shared templates

fn map_shared_template(row: &Row<'_>) -> Result<SharedTemplateView, CoreError> {
    Ok(SharedTemplateView {
        id: row_id(row, 0, SharedTemplateId::from_bytes)?,
        name: row.get(1).sq()?,
        shortcut: row.get(2).sq()?,
        category: row.get(3).sq()?,
        subject: row.get(4).sq()?,
        body_text: row.get(5).sq()?,
        updated_by: row.get(6).sq()?,
        updated_at: UnixMs(row.get(7).sq()?),
    })
}

pub(crate) fn list_shared_templates(
    conn: &mut Connection,
) -> Result<Vec<SharedTemplateView>, CoreError> {
    query_all(
        conn,
        "SELECT id, name, shortcut, category, subject, body_text, updated_by, updated_at
         FROM shared_template
         ORDER BY category ASC, name ASC",
        [],
        map_shared_template,
    )
}

pub(crate) fn upsert_shared_template_row(
    conn: &mut Connection,
    template: &SharedTemplateView,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO shared_template (
            id, name, shortcut, category, subject, body_text, updated_by, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            shortcut = excluded.shortcut,
            category = excluded.category,
            subject = excluded.subject,
            body_text = excluded.body_text,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
        params![
            id_to_blob(template.id.as_bytes()),
            template.name,
            template.shortcut,
            template.category,
            template.subject,
            template.body_text,
            template.updated_by,
            template.updated_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn save_shared_template(
    conn: &mut Connection,
    input: &SaveSharedTemplateInput,
    updated_by: &str,
    now: UnixMs,
) -> Result<SharedTemplateView, CoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "name".to_string(),
            reason: "shared template name is required".to_string(),
        });
    }
    let category = if input.category.trim().is_empty() {
        "general".to_string()
    } else {
        input.category.trim().to_ascii_lowercase()
    };
    let id = input.id.unwrap_or_else(SharedTemplateId::fresh);
    let view = SharedTemplateView {
        id,
        name: name.to_string(),
        shortcut: input
            .shortcut
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        category,
        subject: input.subject.clone(),
        body_text: input.body_text.clone(),
        updated_by: updated_by.to_string(),
        updated_at: now,
    };
    upsert_shared_template_row(conn, &view)?;
    Ok(view)
}

pub(crate) fn delete_shared_template(
    conn: &mut Connection,
    id: SharedTemplateId,
) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "DELETE FROM shared_template WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(changed > 0)
}

// ---------------------------------------------------------------- show contacts

fn map_contact(row: &Row<'_>) -> Result<ShowContactView, CoreError> {
    let emails: Vec<String> = from_json(&row.get::<_, String>(2).sq()?)?;
    Ok(ShowContactView {
        id: row_id(row, 0, ContactId::from_bytes)?,
        name: row.get(1).sq()?,
        emails,
        category: ContactCategory::from_db(&row.get::<_, String>(3).sq()?),
        organization: row.get(4).sq()?,
        tier: row.get(5).sq()?,
        notes: row.get(6).sq()?,
        updated_at: UnixMs(row.get(7).sq()?),
    })
}

pub(crate) fn list_show_contacts(conn: &mut Connection) -> Result<Vec<ShowContactView>, CoreError> {
    query_all(
        conn,
        "SELECT id, name, emails_json, category, organization, tier, notes, updated_at
         FROM show_contact
         ORDER BY name ASC",
        [],
        map_contact,
    )
}

pub(crate) fn upsert_show_contact_row(
    conn: &mut Connection,
    contact: &ShowContactView,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO show_contact (
            id, name, emails_json, category, organization, tier, notes, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            emails_json = excluded.emails_json,
            category = excluded.category,
            organization = excluded.organization,
            tier = excluded.tier,
            notes = excluded.notes,
            updated_at = excluded.updated_at",
        params![
            id_to_blob(contact.id.as_bytes()),
            contact.name,
            to_json(&contact.emails)?,
            contact.category.as_str(),
            contact.organization,
            contact.tier,
            contact.notes,
            contact.updated_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn save_show_contact(
    conn: &mut Connection,
    input: &SaveShowContactInput,
    now: UnixMs,
) -> Result<ShowContactView, CoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "name".to_string(),
            reason: "contact name is required".to_string(),
        });
    }
    let mut emails: Vec<String> = input
        .emails
        .iter()
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| e.contains('@') && e.len() >= 3)
        .collect();
    emails.sort();
    emails.dedup();
    if emails.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "emails".to_string(),
            reason: "at least one valid email address is required".to_string(),
        });
    }
    let id = input.id.unwrap_or_else(ContactId::fresh);
    let view = ShowContactView {
        id,
        name: name.to_string(),
        emails,
        category: input.category,
        organization: input
            .organization
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        tier: input
            .tier
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        notes: input.notes.trim().to_string(),
        updated_at: now,
    };
    upsert_show_contact_row(conn, &view)?;
    Ok(view)
}

pub(crate) fn delete_show_contact(conn: &mut Connection, id: ContactId) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "DELETE FROM show_contact WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(changed > 0)
}

pub(crate) fn find_contact_by_email(
    conn: &mut Connection,
    email: &str,
) -> Result<Option<ShowContactView>, CoreError> {
    let target = email.trim().to_ascii_lowercase();
    if target.is_empty() {
        return Ok(None);
    }
    let all = list_show_contacts(conn)?;
    Ok(all
        .into_iter()
        .find(|c| c.emails.iter().any(|e| e.eq_ignore_ascii_case(&target))))
}

// ---------------------------------------------------------------- pinned links

fn map_pinned_link(row: &Row<'_>) -> Result<PinnedLinkView, CoreError> {
    Ok(PinnedLinkView {
        id: row_id(row, 0, PinnedLinkId::from_bytes)?,
        title: row.get(1).sq()?,
        url: row.get(2).sq()?,
        category: row.get(3).sq()?,
        description: row.get(4).sq()?,
        position: row.get::<_, i64>(5).sq()?.max(0) as u32,
        updated_at: UnixMs(row.get(6).sq()?),
    })
}

pub(crate) fn list_pinned_links(conn: &mut Connection) -> Result<Vec<PinnedLinkView>, CoreError> {
    query_all(
        conn,
        "SELECT id, title, url, category, description, position, updated_at
         FROM pinned_link
         ORDER BY position ASC, title ASC",
        [],
        map_pinned_link,
    )
}

pub(crate) fn upsert_pinned_link_row(
    conn: &mut Connection,
    link: &PinnedLinkView,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO pinned_link (
            id, title, url, category, description, position, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            url = excluded.url,
            category = excluded.category,
            description = excluded.description,
            position = excluded.position,
            updated_at = excluded.updated_at",
        params![
            id_to_blob(link.id.as_bytes()),
            link.title,
            link.url,
            link.category,
            link.description,
            link.position as i64,
            link.updated_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn save_pinned_link(
    conn: &mut Connection,
    input: &SavePinnedLinkInput,
    now: UnixMs,
) -> Result<PinnedLinkView, CoreError> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "title".to_string(),
            reason: "link title is required".to_string(),
        });
    }
    let url = crate::staff_core::validate_external_url(&input.url)?;
    let next_pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM pinned_link",
            [],
            |r| r.get(0),
        )
        .sq()?;
    let id = input.id.unwrap_or_else(PinnedLinkId::fresh);
    let existing_pos: Option<i64> = query_one(
        conn,
        "SELECT position FROM pinned_link WHERE id = ?1",
        params![id_to_blob(id.as_bytes())],
        |r| r.get(0).sq(),
    )?;
    let view = PinnedLinkView {
        id,
        title: title.to_string(),
        url: url.to_string(),
        category: if input.category.trim().is_empty() {
            "operations".to_string()
        } else {
            input.category.trim().to_ascii_lowercase()
        },
        description: input
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        position: existing_pos.unwrap_or(next_pos).max(0) as u32,
        updated_at: now,
    };
    upsert_pinned_link_row(conn, &view)?;
    Ok(view)
}

pub(crate) fn delete_pinned_link(
    conn: &mut Connection,
    id: PinnedLinkId,
) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "DELETE FROM pinned_link WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(changed > 0)
}

/// Seeds starter Pigment Staff directory, shared templates, contacts, and pinned links
/// when connecting to `local://pigment-demo` (idempotent; does nothing if already populated).
pub(crate) fn seed_local_pigment_demo(
    conn: &mut Connection,
    current_member: &StaffMemberView,
    now: UnixMs,
) -> Result<(), CoreError> {
    upsert_staff_member(conn, current_member)?;

    if list_staff_directory(conn)?.len() <= 1 {
        let members = [
            (
                "Reyth",
                "Show Director",
                "reyth",
                "info@pigmentminiatures.com",
                "operations",
            ),
            (
                "Maya Chen",
                "Sponsors Lead",
                "maya",
                "sponsors@pigmentminiatures.com",
                "sponsors",
            ),
            (
                "Devon Brooks",
                "Exhibition Coordinator",
                "devon",
                "exhibition@pigmentminiatures.com",
                "exhibition",
            ),
            (
                "Tessa Morales",
                "Marketing Lead",
                "tessa",
                "marketing@pigmentminiatures.com",
                "marketing",
            ),
            (
                "Sam Patel",
                "Volunteer Coordinator",
                "sam",
                "volunteers@pigmentminiatures.com",
                "volunteers",
            ),
        ];
        for (name, role, handle, email, dept) in members {
            let member = StaffMemberView {
                id: StaffMemberId::derive(
                    uuid::Uuid::NAMESPACE_URL,
                    format!("ferrule:staff:{handle}").as_bytes(),
                ),
                name: name.to_string(),
                role: role.to_string(),
                handle: handle.to_string(),
                email: email.to_string(),
                department: Some(dept.to_string()),
                updated_at: now,
            };
            upsert_staff_member(conn, &member)?;
        }
    }

    if list_shared_templates(conn)?.is_empty() {
        let templates = [
            (
                "Sponsor Prospectus Reply",
                Some("/sponsor"),
                "sponsors",
                "Pigment Miniatures Show 2027 — Sponsorship Prospectus",
                "Hi there,\n\nThank you for your interest in sponsoring Pigment Miniatures Show 2027! I've attached our 2027 Sponsor Prospectus with booth dimensions, Gold/Silver/Bronze tiers, and print deadlines.\n\nLet us know which tier looks best and we'll reserve your spot on the floor map.\n\nWarmly,\nPigment Sponsors Team",
            ),
            (
                "Exhibitor Load-In FAQ",
                Some("/loadin"),
                "exhibitors",
                "Pigment 2027 — Exhibitor Load-In & Table Details",
                "Hi,\n\nExhibitor load-in opens Friday at 1:00 PM via the East Loading Dock. Each 6ft table includes black draping, two chairs, and two staff badges.\n\nReply here if you need power drops or extra badge passes.\n\nBest,\nPigment Exhibition Team",
            ),
            (
                "Volunteer Onboarding Welcome",
                Some("/vol"),
                "volunteers",
                "Welcome to the Pigment 2027 Volunteer Crew!",
                "Hi,\n\nThank you so much for signing up to help run Pigment 2027! Shift sign-ups and the Discord #volunteers invite link are below. Volunteers who work two 4-hour shifts receive a full weekend badge and staff shirt.\n\nSee you at the show!\nPigment Volunteer Team",
            ),
        ];
        for (name, shortcut, category, subject, body) in templates {
            let _ = save_shared_template(
                conn,
                &SaveSharedTemplateInput {
                    id: None,
                    name: name.to_string(),
                    shortcut: shortcut.map(str::to_string),
                    category: category.to_string(),
                    subject: subject.to_string(),
                    body_text: body.to_string(),
                },
                &current_member.handle,
                now,
            )?;
        }
    }

    if list_show_contacts(conn)?.is_empty() {
        let contacts = [
            (
                "Dana Okafor",
                vec!["dana@ferngate.co"],
                ContactCategory::Sponsor,
                Some("Ferngate Miniatures"),
                Some("Gold Sponsor"),
                "Returning 2026 sponsor; prefers corner booth near lighting demo tables.",
            ),
            (
                "Marco Bellini",
                vec!["marco@vallejo-atelier.example"],
                ContactCategory::Instructor,
                Some("Bellini Studio"),
                Some("Keynote Workshop"),
                "Teaching NMM & Skin Tones Saturday 10am–2pm. Needs overhead camera rig.",
            ),
            (
                "Priya Nair",
                vec!["priya@austinconvention.example"],
                ContactCategory::Venue,
                Some("Cedar Creek Event Center"),
                Some("Venue Ops Lead"),
                "Main hall A/V, loading dock schedule, and room keycard contact.",
            ),
        ];
        for (name, emails, category, org, tier, notes) in contacts {
            let _ = save_show_contact(
                conn,
                &SaveShowContactInput {
                    id: None,
                    name: name.to_string(),
                    emails: emails.into_iter().map(str::to_string).collect(),
                    category,
                    organization: org.map(str::to_string),
                    tier: tier.map(str::to_string),
                    notes: notes.to_string(),
                },
                now,
            )?;
        }
    }

    if list_pinned_links(conn)?.is_empty() {
        let links = [
            (
                "2027 Operations Workbook",
                "https://docs.google.com/spreadsheets/d/pigment-2027-ops",
                "operations",
                Some("Master floor plan, vendor map/payment, and room supplies sheets"),
            ),
            (
                "Pigment Admin Portal",
                "https://pigmentminiatures.com/admin",
                "admin",
                Some("Registrations, exhibitor badges, and ticket check-in"),
            ),
            (
                "Canva Marketing Assets",
                "https://www.canva.com/folder/pigment-2027-marketing",
                "marketing",
                Some("Social graphics, badges, and print banners"),
            ),
            (
                "Staff Discord",
                "https://discord.com/channels/@me",
                "comms",
                Some("Live staff chat and weekly planning voice channel"),
            ),
        ];
        for (title, url, category, desc) in links {
            let _ = save_pinned_link(
                conn,
                &SavePinnedLinkInput {
                    id: None,
                    title: title.to_string(),
                    url: url.to_string(),
                    category: category.to_string(),
                    description: desc.map(str::to_string),
                },
                now,
            )?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------- offline outbox

pub(crate) struct StaffOutboxEntry {
    pub seq: i64,
    pub op: crate::sync::staff::StaffOutboxOp,
}

pub(crate) fn enqueue_staff_outbox(
    conn: &mut Connection,
    op: &crate::sync::staff::StaffOutboxOp,
    now: UnixMs,
) -> Result<(), CoreError> {
    let payload = to_json(op)?;
    conn.execute(
        "INSERT INTO staff_outbox (op_kind, entity_key, payload_json, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![op.kind_key(), op.entity_key(), payload, now.0],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn list_staff_outbox(
    conn: &mut Connection,
) -> Result<Vec<StaffOutboxEntry>, CoreError> {
    query_all(
        conn,
        "SELECT seq, payload_json FROM staff_outbox ORDER BY seq ASC",
        [],
        |r| {
            let seq: i64 = r.get(0).sq()?;
            let raw: String = r.get(1).sq()?;
            let op = from_json(&raw)?;
            Ok(StaffOutboxEntry { seq, op })
        },
    )
}

pub(crate) fn delete_staff_outbox_item(conn: &mut Connection, seq: i64) -> Result<(), CoreError> {
    conn.execute("DELETE FROM staff_outbox WHERE seq = ?1", params![seq])
        .sq()?;
    Ok(())
}

pub(crate) fn clear_staff_outbox(conn: &mut Connection) -> Result<(), CoreError> {
    conn.execute("DELETE FROM staff_outbox", []).sq()?;
    Ok(())
}

pub(crate) fn count_staff_outbox(conn: &mut Connection) -> Result<usize, CoreError> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM staff_outbox", [], |r| r.get(0))
        .sq()?;
    Ok(count.max(0) as usize)
}
