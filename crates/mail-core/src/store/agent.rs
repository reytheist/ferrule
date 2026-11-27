//! Phase 4 persistence: agent permissions, undoable agent activity log, bridge/AI settings,
//! message lookup helpers, and pending agent draft management.

use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::ids::{AccountId, AgentActivityId, DraftId, MessageId, ThreadId, UnixMs};
use crate::store::{
    blob_to_id, draft_view, from_json, get_draft, id_to_blob, list_account_rows, query_all,
    query_one, row_id, to_json, SqlResultExt,
};
use crate::types::{
    AgentAccountPermissionView, AgentActivityView, AgentBridgeConfig, AgentUndoPayload,
    AiConfigView, DraftOrigin, DraftState, DraftView, FolderInfoView,
    SaveAgentAccountPermissionInput, ThreadFlagSnapshot,
};

const BRIDGE_CONFIG_KEY: &str = "agent_bridge_config";
const AI_CONFIG_KEY: &str = "ai_config";

#[derive(Debug, Clone)]
pub(crate) struct AccountPermissionRow {
    pub enabled: bool,
    pub allow_read: bool,
    pub allow_triage: bool,
    pub allow_drafts: bool,
    pub blocked_folders: Vec<String>,
}

impl Default for AccountPermissionRow {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_read: true,
            allow_triage: true,
            allow_drafts: true,
            blocked_folders: Vec::new(),
        }
    }
}

impl AccountPermissionRow {
    pub(crate) fn is_folder_allowed(&self, folder: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let lower = folder.trim().to_ascii_lowercase();
        !self
            .blocked_folders
            .iter()
            .any(|b| b.trim().to_ascii_lowercase() == lower)
    }
}

pub(crate) fn get_account_permission(
    conn: &mut Connection,
    account_id: AccountId,
) -> Result<AccountPermissionRow, CoreError> {
    let row = query_one(
        conn,
        "SELECT enabled, allow_read, allow_triage, allow_drafts, blocked_folders_json
         FROM agent_account_permission WHERE account_id = ?1",
        params![id_to_blob(account_id.as_bytes())],
        |row| {
            let blocked_raw: String = row.get(4).sq()?;
            Ok(AccountPermissionRow {
                enabled: row.get::<_, i64>(0).sq()? != 0,
                allow_read: row.get::<_, i64>(1).sq()? != 0,
                allow_triage: row.get::<_, i64>(2).sq()? != 0,
                allow_drafts: row.get::<_, i64>(3).sq()? != 0,
                blocked_folders: from_json(&blocked_raw)?,
            })
        },
    )?;
    Ok(row.unwrap_or_default())
}

pub(crate) fn save_account_permission(
    conn: &mut Connection,
    input: &SaveAgentAccountPermissionInput,
    now: UnixMs,
) -> Result<AgentAccountPermissionView, CoreError> {
    let cleaned_blocked: Vec<String> = input
        .blocked_folders
        .iter()
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    conn.execute(
        "INSERT INTO agent_account_permission
            (account_id, enabled, allow_read, allow_triage, allow_drafts, blocked_folders_json, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(account_id) DO UPDATE SET
            enabled = excluded.enabled,
            allow_read = excluded.allow_read,
            allow_triage = excluded.allow_triage,
            allow_drafts = excluded.allow_drafts,
            blocked_folders_json = excluded.blocked_folders_json,
            updated_at = excluded.updated_at",
        params![
            id_to_blob(input.account_id.as_bytes()),
            input.enabled as i64,
            input.allow_read as i64,
            input.allow_triage as i64,
            input.allow_drafts as i64,
            to_json(&cleaned_blocked)?,
            now.0,
        ],
    )
    .sq()?;
    let all = list_agent_permissions(conn)?;
    all.into_iter()
        .find(|p| p.account_id == input.account_id)
        .ok_or_else(|| CoreError::NotFound {
            entity: "account".to_string(),
        })
}

pub(crate) fn list_agent_permissions(
    conn: &mut Connection,
) -> Result<Vec<AgentAccountPermissionView>, CoreError> {
    let accounts = list_account_rows(conn)?;
    let mut out = Vec::with_capacity(accounts.len());
    for account in accounts {
        let perm = get_account_permission(conn, account.id)?;
        let folders = known_folder_names_for_account(conn, account.id)?;
        out.push(AgentAccountPermissionView {
            account_id: account.id,
            account_label: account.display_name,
            account_email: account.email,
            enabled: perm.enabled,
            allow_read: perm.allow_read,
            allow_triage: perm.allow_triage,
            allow_drafts: perm.allow_drafts,
            blocked_folders: perm.blocked_folders,
            known_folders: folders,
        });
    }
    Ok(out)
}

pub(crate) fn known_folder_names_for_account(
    conn: &mut Connection,
    account_id: AccountId,
) -> Result<Vec<String>, CoreError> {
    let mut names: BTreeSet<String> = ["INBOX", "Sent", "Archive", "Trash", "Spam"]
        .into_iter()
        .map(String::from)
        .collect();
    let cursor_folders: Vec<String> = query_all(
        conn,
        "SELECT folder_name FROM folder_cursor WHERE account_id = ?1",
        params![id_to_blob(account_id.as_bytes())],
        |row| row.get(0).sq(),
    )?;
    for f in cursor_folders {
        if !f.trim().is_empty() {
            names.insert(f);
        }
    }
    let msg_folders: Vec<String> = query_all(
        conn,
        "SELECT DISTINCT folder_name FROM message WHERE account_id = ?1",
        params![id_to_blob(account_id.as_bytes())],
        |row| row.get(0).sq(),
    )?;
    for f in msg_folders {
        if !f.trim().is_empty() {
            names.insert(f);
        }
    }
    Ok(names.into_iter().collect())
}

pub(crate) fn list_account_folders(
    conn: &mut Connection,
    account_id: AccountId,
    perm: &AccountPermissionRow,
) -> Result<Vec<FolderInfoView>, CoreError> {
    let mut rows = query_all(
        conn,
        "SELECT folder_name, messages_count, unseen_count
         FROM folder_cursor WHERE account_id = ?1 ORDER BY folder_name ASC",
        params![id_to_blob(account_id.as_bytes())],
        |row| {
            let name: String = row.get(0).sq()?;
            let messages_count = row.get::<_, i64>(1).sq()?.max(0) as u32;
            let unseen_count = row.get::<_, i64>(2).sq()?.max(0) as u32;
            Ok(FolderInfoView {
                account_id,
                agent_allowed: perm.is_folder_allowed(&name),
                name,
                messages_count,
                unseen_count,
            })
        },
    )?;
    if rows.is_empty() {
        rows.push(FolderInfoView {
            account_id,
            name: "INBOX".to_string(),
            messages_count: 0,
            unseen_count: 0,
            agent_allowed: perm.is_folder_allowed("INBOX"),
        });
    }
    Ok(rows)
}

// ── Agent Activity Log ──────────────────────────────────────────────────────

pub(crate) struct AgentActivityRow {
    pub id: AgentActivityId,
    pub agent_id: String,
    pub tool_name: String,
    pub summary: String,
    pub account_id: Option<AccountId>,
    pub thread_id: Option<ThreadId>,
    pub undo_payload: Option<AgentUndoPayload>,
    pub undone: bool,
    pub undone_at: Option<UnixMs>,
    pub created_at: UnixMs,
}

impl AgentActivityRow {
    pub(crate) fn to_view(&self) -> AgentActivityView {
        AgentActivityView {
            id: self.id,
            agent_id: self.agent_id.clone(),
            tool_name: self.tool_name.clone(),
            summary: self.summary.clone(),
            account_id: self.account_id,
            thread_id: self.thread_id,
            can_undo: self.undo_payload.is_some() && !self.undone,
            undone: self.undone,
            undone_at: self.undone_at,
            created_at: self.created_at,
        }
    }
}

fn map_activity(row: &Row<'_>) -> Result<AgentActivityRow, CoreError> {
    let account_blob: Option<Vec<u8>> = row.get(4).sq()?;
    let thread_blob: Option<Vec<u8>> = row.get(5).sq()?;
    let undo_raw: Option<String> = row.get(6).sq()?;
    let undone_at_raw: Option<i64> = row.get(8).sq()?;
    Ok(AgentActivityRow {
        id: row_id(row, 0, AgentActivityId::from_bytes)?,
        agent_id: row.get(1).sq()?,
        tool_name: row.get(2).sq()?,
        summary: row.get(3).sq()?,
        account_id: match account_blob {
            Some(b) => Some(blob_to_id(&b, AccountId::from_bytes)?),
            None => None,
        },
        thread_id: match thread_blob {
            Some(b) => Some(blob_to_id(&b, ThreadId::from_bytes)?),
            None => None,
        },
        undo_payload: match undo_raw {
            Some(raw) => Some(from_json(&raw)?),
            None => None,
        },
        undone: row.get::<_, i64>(7).sq()? != 0,
        undone_at: undone_at_raw.map(UnixMs),
        created_at: UnixMs(row.get(9).sq()?),
    })
}

pub(crate) struct NewAgentActivity<'a> {
    pub agent_id: &'a str,
    pub tool_name: &'a str,
    pub summary: String,
    pub account_id: Option<AccountId>,
    pub thread_id: Option<ThreadId>,
    pub undo_payload: Option<AgentUndoPayload>,
    pub created_at: UnixMs,
}

pub(crate) fn insert_agent_activity(
    conn: &mut Connection,
    input: &NewAgentActivity<'_>,
) -> Result<AgentActivityId, CoreError> {
    let id = AgentActivityId::fresh();
    let undo_json = match &input.undo_payload {
        Some(payload) => Some(to_json(payload)?),
        None => None,
    };
    conn.execute(
        "INSERT INTO agent_activity_log
            (id, agent_id, tool_name, summary, account_id, thread_id, undo_payload_json, undone, undone_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, NULL, ?8)",
        params![
            id_to_blob(id.as_bytes()),
            input.agent_id,
            input.tool_name,
            input.summary,
            input.account_id.map(|a| id_to_blob(a.as_bytes())),
            input.thread_id.map(|t| id_to_blob(t.as_bytes())),
            undo_json,
            input.created_at.0,
        ],
    )
    .sq()?;
    Ok(id)
}

pub(crate) fn list_agent_activity(
    conn: &mut Connection,
    limit: u32,
) -> Result<Vec<AgentActivityView>, CoreError> {
    let rows = query_all(
        conn,
        "SELECT id, agent_id, tool_name, summary, account_id, thread_id, undo_payload_json, undone, undone_at, created_at
         FROM agent_activity_log
         ORDER BY created_at DESC, rowid DESC
         LIMIT ?1",
        params![limit.clamp(1, 200) as i64],
        map_activity,
    )?;
    Ok(rows.iter().map(AgentActivityRow::to_view).collect())
}

pub(crate) fn get_agent_activity(
    conn: &mut Connection,
    id: AgentActivityId,
) -> Result<Option<AgentActivityRow>, CoreError> {
    query_one(
        conn,
        "SELECT id, agent_id, tool_name, summary, account_id, thread_id, undo_payload_json, undone, undone_at, created_at
         FROM agent_activity_log WHERE id = ?1",
        params![id_to_blob(id.as_bytes())],
        map_activity,
    )
}

pub(crate) fn mark_agent_activity_undone(
    conn: &mut Connection,
    id: AgentActivityId,
    now: UnixMs,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE agent_activity_log SET undone = 1, undone_at = ?2 WHERE id = ?1",
        params![id_to_blob(id.as_bytes()), now.0],
    )
    .sq()?;
    Ok(())
}

/// Capture exact thread + message read flags before a triage mutation so it can be
/// reversed with 1-click undo.
pub(crate) fn snapshot_thread_flags(
    conn: &mut Connection,
    thread_ids: &[ThreadId],
) -> Result<Vec<ThreadFlagSnapshot>, CoreError> {
    let mut out = Vec::with_capacity(thread_ids.len());
    for &thread_id in thread_ids {
        let row = query_one(
            conn,
            "SELECT archived, trashed, spam, starred, unread_count FROM thread WHERE id = ?1",
            params![id_to_blob(thread_id.as_bytes())],
            |r| {
                Ok((
                    r.get::<_, i64>(0).sq()? != 0,
                    r.get::<_, i64>(1).sq()? != 0,
                    r.get::<_, i64>(2).sq()? != 0,
                    r.get::<_, i64>(3).sq()? != 0,
                    r.get::<_, i64>(4).sq()?.max(0) as u32,
                ))
            },
        )?;
        let Some((archived, trashed, spam, starred, unread_count)) = row else {
            continue;
        };
        let message_reads = query_all(
            conn,
            "SELECT id, is_read FROM message WHERE thread_id = ?1",
            params![id_to_blob(thread_id.as_bytes())],
            |r| {
                Ok((
                    row_id(r, 0, MessageId::from_bytes)?,
                    r.get::<_, i64>(1).sq()? != 0,
                ))
            },
        )?;
        out.push(ThreadFlagSnapshot {
            thread_id,
            archived,
            trashed,
            spam,
            starred,
            unread_count,
            message_reads,
        });
    }
    Ok(out)
}

/// Restore exact thread + message flags captured in a `ThreadFlagSnapshot`.
pub(crate) fn restore_thread_flags(
    conn: &mut Connection,
    snapshots: &[ThreadFlagSnapshot],
) -> Result<Vec<ThreadId>, CoreError> {
    let tx = conn.transaction().sq()?;
    let mut restored = Vec::with_capacity(snapshots.len());
    for snap in snapshots {
        tx.execute(
            "UPDATE thread SET archived = ?2, trashed = ?3, spam = ?4, starred = ?5, unread_count = ?6
             WHERE id = ?1",
            params![
                id_to_blob(snap.thread_id.as_bytes()),
                snap.archived as i64,
                snap.trashed as i64,
                snap.spam as i64,
                snap.starred as i64,
                snap.unread_count as i64,
            ],
        )
        .sq()?;
        for &(msg_id, is_read) in &snap.message_reads {
            tx.execute(
                "UPDATE message SET is_read = ?2 WHERE id = ?1",
                params![id_to_blob(msg_id.as_bytes()), is_read as i64],
            )
            .sq()?;
        }
        restored.push(snap.thread_id);
    }
    tx.commit().sq()?;
    Ok(restored)
}

// ── Message Lookup & Folder Helpers for MCP ─────────────────────────────────

pub(crate) struct SingleMessageRecord {
    pub id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub folder_name: String,
    pub subject: String,
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub date: UnixMs,
    pub body_text: String,
}

pub(crate) fn get_single_message(
    conn: &mut Connection,
    message_id: MessageId,
) -> Result<Option<SingleMessageRecord>, CoreError> {
    query_one(
        conn,
        "SELECT id, account_id, thread_id, folder_name, subject, from_name, from_email, to_json, date, body_text
         FROM message WHERE id = ?1",
        params![id_to_blob(message_id.as_bytes())],
        |row| {
            let to_raw: String = row.get(7).sq()?;
            Ok(SingleMessageRecord {
                id: row_id(row, 0, MessageId::from_bytes)?,
                account_id: row_id(row, 1, AccountId::from_bytes)?,
                thread_id: row_id(row, 2, ThreadId::from_bytes)?,
                folder_name: row.get(3).sq()?,
                subject: row.get(4).sq()?,
                from_name: row.get(5).sq()?,
                from_email: row.get(6).sq()?,
                to: from_json(&to_raw)?,
                date: UnixMs(row.get(8).sq()?),
                body_text: row.get(9).sq()?,
            })
        },
    )
}

pub(crate) fn thread_folders(
    conn: &mut Connection,
    thread_id: ThreadId,
) -> Result<Vec<String>, CoreError> {
    query_all(
        conn,
        "SELECT DISTINCT folder_name FROM message WHERE thread_id = ?1",
        params![id_to_blob(thread_id.as_bytes())],
        |row| row.get(0).sq(),
    )
}

pub(crate) fn resolve_message_thread_ids(
    conn: &mut Connection,
    message_ids: &[MessageId],
) -> Result<Vec<ThreadId>, CoreError> {
    let mut seen = BTreeSet::new();
    for &mid in message_ids {
        let tid: Option<Vec<u8>> = conn
            .query_row(
                "SELECT thread_id FROM message WHERE id = ?1",
                params![id_to_blob(mid.as_bytes())],
                |r| r.get(0),
            )
            .optional()
            .sq()?;
        if let Some(bytes) = tid {
            seen.insert(blob_to_id(&bytes, ThreadId::from_bytes)?);
        }
    }
    Ok(seen.into_iter().collect())
}

// ── Pending Agent Drafts ────────────────────────────────────────────────────

pub(crate) fn list_pending_agent_drafts(conn: &mut Connection) -> Result<Vec<DraftView>, CoreError> {
    let ids: Vec<DraftId> = query_all(
        conn,
        "SELECT id FROM draft
         WHERE origin_kind = 'agent'
           AND json_extract(state_json, '$.kind') = 'pendingApproval'
         ORDER BY updated_at DESC",
        [],
        |r| row_id(r, 0, DraftId::from_bytes),
    )?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(row) = get_draft(conn, id)? {
            out.push(draft_view(conn, &row)?);
        }
    }
    Ok(out)
}

pub(crate) struct UpdateAgentDraftParams {
    pub draft_id: DraftId,
    pub to: Option<Vec<String>>,
    pub cc: Option<Vec<String>>,
    pub subject: Option<String>,
    pub body_text: Option<String>,
    pub rationale: Option<String>,
    pub updated_at: UnixMs,
}

pub(crate) fn update_pending_agent_draft(
    conn: &mut Connection,
    input: &UpdateAgentDraftParams,
) -> Result<DraftView, CoreError> {
    let existing = get_draft(conn, input.draft_id)?.ok_or_else(|| CoreError::NotFound {
        entity: "draft".to_string(),
    })?;
    let (agent_id, existing_rationale) = match &existing.origin {
        DraftOrigin::Agent {
            agent_id,
            rationale,
        } => (agent_id.clone(), rationale.clone()),
        DraftOrigin::Human => {
            return Err(CoreError::PermissionDenied {
                reason: "agents may only update agent-originated drafts".to_string(),
            });
        }
    };
    if !matches!(existing.state, DraftState::PendingApproval) {
        return Err(CoreError::IllegalState {
            reason: "only drafts in PendingApproval can be updated by an agent".to_string(),
        });
    }
    let to = input.to.clone().unwrap_or(existing.to);
    let cc = input.cc.clone().unwrap_or(existing.cc);
    let subject = input.subject.clone().unwrap_or(existing.subject);
    let body_text = input.body_text.clone().unwrap_or(existing.body_text);
    let rationale = input.rationale.clone().unwrap_or(existing_rationale);

    conn.execute(
        "UPDATE draft
         SET to_json = ?2, cc_json = ?3, subject = ?4, body_text = ?5,
             origin_agent = ?6, rationale = ?7, rev = rev + 1, updated_at = ?8
         WHERE id = ?1",
        params![
            id_to_blob(input.draft_id.as_bytes()),
            to_json(&to)?,
            to_json(&cc)?,
            subject,
            body_text,
            agent_id,
            rationale,
            input.updated_at.0,
        ],
    )
    .sq()?;
    let updated = get_draft(conn, input.draft_id)?.ok_or_else(|| CoreError::Internal {
        reason: "draft disappeared after update".to_string(),
    })?;
    draft_view(conn, &updated)
}

// ── Bridge & AI Settings (non-secret fields only) ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredBridgeConfig {
    mcp_enabled: bool,
    sse_port: u16,
    webhook_enabled: bool,
    webhook_url: String,
}

pub(crate) fn get_bridge_config(conn: &mut Connection) -> Result<AgentBridgeConfig, CoreError> {
    let raw: Option<String> = query_one(
        conn,
        "SELECT value_json FROM app_setting WHERE key = ?1",
        params![BRIDGE_CONFIG_KEY],
        |row| row.get(0).sq(),
    )?;
    match raw {
        Some(json) => {
            let stored: StoredBridgeConfig = from_json(&json)?;
            Ok(AgentBridgeConfig {
                mcp_enabled: stored.mcp_enabled,
                sse_port: stored.sse_port,
                webhook_enabled: stored.webhook_enabled,
                webhook_url: stored.webhook_url,
                has_webhook_secret: false,
                ipc_endpoint: String::new(),
            })
        }
        None => Ok(AgentBridgeConfig::default()),
    }
}

pub(crate) fn save_bridge_config(
    conn: &mut Connection,
    mcp_enabled: bool,
    sse_port: u16,
    webhook_enabled: bool,
    webhook_url: &str,
) -> Result<(), CoreError> {
    let stored = StoredBridgeConfig {
        mcp_enabled,
        sse_port,
        webhook_enabled,
        webhook_url: webhook_url.to_string(),
    };
    conn.execute(
        "INSERT INTO app_setting (key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![BRIDGE_CONFIG_KEY, to_json(&stored)?],
    )
    .sq()?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAiConfig {
    enabled: bool,
    base_url: String,
    model: String,
}

pub(crate) fn get_ai_config(conn: &mut Connection) -> Result<AiConfigView, CoreError> {
    let raw: Option<String> = query_one(
        conn,
        "SELECT value_json FROM app_setting WHERE key = ?1",
        params![AI_CONFIG_KEY],
        |row| row.get(0).sq(),
    )?;
    match raw {
        Some(json) => {
            let stored: StoredAiConfig = from_json(&json)?;
            Ok(AiConfigView {
                enabled: stored.enabled,
                base_url: stored.base_url,
                model: stored.model,
                has_api_key: false,
            })
        }
        None => Ok(AiConfigView::default()),
    }
}

pub(crate) fn save_ai_config(
    conn: &mut Connection,
    enabled: bool,
    base_url: &str,
    model: &str,
) -> Result<(), CoreError> {
    let stored = StoredAiConfig {
        enabled,
        base_url: base_url.to_string(),
        model: model.to_string(),
    };
    conn.execute(
        "INSERT INTO app_setting (key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![AI_CONFIG_KEY, to_json(&stored)?],
    )
    .sq()?;
    Ok(())
}
