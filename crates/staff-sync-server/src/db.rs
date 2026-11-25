use std::path::Path;
use std::sync::Mutex;

use mail_core::calendar::{ActionItemView, CalendarEventView, MeetingDocView};
use mail_core::ids::{
    ActionItemId, CalendarEventId, ContactId, PinnedLinkId, SharedTemplateId, SharedThreadId,
    UnixMs,
};
use mail_core::staff::{
    HandoffStatus, PinnedLinkView, SharedCommentView, SharedDraftView, SharedTemplateView,
    SharedThreadView, ShowContactView, StaffMemberView,
};
use mail_core::StaffSyncSnapshot;
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;

const SCHEMA_SQL: &str = include_str!("schema.sql");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("storage error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database lock poisoned")]
    LockPoisoned,
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("stale revision (current: {current_rev})")]
    Conflict { current_rev: u32 },
}

pub struct SyncDb {
    conn: Mutex<Connection>,
}

impl SyncDb {
    pub fn open(path: Option<&Path>) -> Result<Self, DbError> {
        let conn = match path {
            Some(p) => {
                if let Some(parent) = p.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }
                Connection::open(p)?
            }
            None => Connection::open_in_memory()?,
        };
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn with_conn<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T, DbError>) -> Result<T, DbError> {
        let mut guard = self.conn.lock().map_err(|_| DbError::LockPoisoned)?;
        f(&mut guard)
    }

    pub fn snapshot(&self, since: Option<UnixMs>, server_time: UnixMs) -> Result<StaffSyncSnapshot, DbError> {
        self.with_conn(|conn| {
            let min_ts = since.map(|t| t.0).unwrap_or(i64::MIN);
            let members: Vec<StaffMemberView> = query_json_rows(
                conn,
                "SELECT payload_json FROM staff_member WHERE updated_at >= ?1 ORDER BY updated_at ASC",
                min_ts,
            )?;
            let mut shared_threads: Vec<SharedThreadView> = query_json_rows(
                conn,
                "SELECT payload_json FROM shared_thread WHERE updated_at >= ?1 ORDER BY updated_at ASC",
                min_ts,
            )?;
            for thread in &mut shared_threads {
                let tid = thread.id.to_string();
                thread.comments = query_comments(conn, &tid)?;
                thread.draft = query_draft(conn, &tid)?;
            }
            let shared_templates: Vec<SharedTemplateView> = query_json_rows(
                conn,
                "SELECT payload_json FROM shared_template WHERE updated_at >= ?1 ORDER BY updated_at ASC",
                min_ts,
            )?;
            let contacts: Vec<ShowContactView> = query_json_rows(
                conn,
                "SELECT payload_json FROM show_contact WHERE updated_at >= ?1 ORDER BY updated_at ASC",
                min_ts,
            )?;
            let pinned_links: Vec<PinnedLinkView> = query_json_rows(
                conn,
                "SELECT payload_json FROM pinned_link WHERE updated_at >= ?1 ORDER BY updated_at ASC",
                min_ts,
            )?;
            let calendar_events: Vec<CalendarEventView> = query_json_rows(
                conn,
                "SELECT payload_json FROM calendar_event WHERE updated_at >= ?1 ORDER BY updated_at ASC",
                min_ts,
            )?;
            let meeting_docs: Vec<MeetingDocView> = query_json_rows(
                conn,
                "SELECT payload_json FROM meeting_doc WHERE updated_at >= ?1 ORDER BY updated_at ASC",
                min_ts,
            )?;
            let action_items: Vec<ActionItemView> = query_json_rows(
                conn,
                "SELECT payload_json FROM action_item WHERE updated_at >= ?1 ORDER BY updated_at ASC",
                min_ts,
            )?;
            Ok(StaffSyncSnapshot {
                members,
                shared_threads,
                shared_templates,
                contacts,
                pinned_links,
                calendar_events,
                meeting_docs,
                action_items,
                server_time: Some(server_time),
            })
        })
    }

    pub fn upsert_member(&self, member: &StaffMemberView) -> Result<(), DbError> {
        let json = serde_json::to_string(member)?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO staff_member (id, handle, email, updated_at, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(handle) DO UPDATE SET
                    id = excluded.id,
                    email = excluded.email,
                    updated_at = excluded.updated_at,
                    payload_json = excluded.payload_json",
                params![
                    member.id.to_string(),
                    member.handle.to_ascii_lowercase(),
                    member.email.to_ascii_lowercase(),
                    member.updated_at.0,
                    json,
                ],
            )?;
            Ok(())
        })
    }

    pub fn upsert_shared_thread(&self, thread: &SharedThreadView) -> Result<(), DbError> {
        let mut base = thread.clone();
        let comments = std::mem::take(&mut base.comments);
        let draft = base.draft.take();
        let json = serde_json::to_string(&base)?;
        let tid = thread.id.to_string();

        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO shared_thread (id, updated_at, payload_json)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET
                    updated_at = excluded.updated_at,
                    payload_json = excluded.payload_json",
                params![tid, thread.updated_at.0, json],
            )?;
            for comment in &comments {
                let cjson = serde_json::to_string(comment)?;
                conn.execute(
                    "INSERT OR REPLACE INTO shared_comment (id, shared_thread_id, created_at, payload_json)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        comment.id.to_string(),
                        tid,
                        comment.created_at.0,
                        cjson,
                    ],
                )?;
            }
            if let Some(d) = &draft {
                upsert_draft_inner(conn, d)?;
            }
            Ok(())
        })
    }

    pub fn update_handoff(
        &self,
        id: SharedThreadId,
        assignee_handle: Option<String>,
        status: HandoffStatus,
        now: UnixMs,
    ) -> Result<(), DbError> {
        let tid = id.to_string();
        self.with_conn(|conn| {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT payload_json FROM shared_thread WHERE id = ?1",
                    params![tid],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(raw) = existing else {
                return Err(DbError::NotFound("sharedThread"));
            };
            let mut thread: SharedThreadView = serde_json::from_str(&raw)?;
            thread.assignee_handle = assignee_handle;
            thread.status = status;
            thread.updated_at = now;
            let updated_json = serde_json::to_string(&thread)?;
            conn.execute(
                "UPDATE shared_thread SET updated_at = ?2, payload_json = ?3 WHERE id = ?1",
                params![tid, now.0, updated_json],
            )?;
            Ok(())
        })
    }

    pub fn insert_comment(
        &self,
        shared_thread_id: SharedThreadId,
        comment: &SharedCommentView,
        now: UnixMs,
    ) -> Result<(), DbError> {
        let tid = shared_thread_id.to_string();
        let mut normalized = comment.clone();
        normalized.shared_thread_id = shared_thread_id;
        let json = serde_json::to_string(&normalized)?;
        self.with_conn(|conn| {
            let exists: Option<i64> = conn
                .query_row(
                    "SELECT 1 FROM shared_thread WHERE id = ?1",
                    params![tid],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Err(DbError::NotFound("sharedThread"));
            }
            conn.execute(
                "INSERT OR REPLACE INTO shared_comment (id, shared_thread_id, created_at, payload_json)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    normalized.id.to_string(),
                    tid,
                    normalized.created_at.0,
                    json,
                ],
            )?;
            let touch_ts = now.0.max(normalized.created_at.0);
            conn.execute(
                "UPDATE shared_thread SET updated_at = MAX(updated_at, ?2) WHERE id = ?1",
                params![tid, touch_ts],
            )?;
            Ok(())
        })
    }

    pub fn upsert_draft(
        &self,
        shared_thread_id: SharedThreadId,
        draft: &SharedDraftView,
        now: UnixMs,
    ) -> Result<(), DbError> {
        let mut normalized = draft.clone();
        normalized.shared_thread_id = shared_thread_id;
        let tid = shared_thread_id.to_string();
        self.with_conn(|conn| {
            let exists: Option<i64> = conn
                .query_row(
                    "SELECT 1 FROM shared_thread WHERE id = ?1",
                    params![tid],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Err(DbError::NotFound("sharedThread"));
            }
            upsert_draft_inner(conn, &normalized)?;
            let touch_ts = now.0.max(normalized.updated_at.0);
            conn.execute(
                "UPDATE shared_thread SET updated_at = MAX(updated_at, ?2) WHERE id = ?1",
                params![tid, touch_ts],
            )?;
            Ok(())
        })
    }

    pub fn upsert_template(&self, id: SharedTemplateId, tpl: &SharedTemplateView) -> Result<(), DbError> {
        let mut normalized = tpl.clone();
        normalized.id = id;
        self.upsert_simple("shared_template", &id.to_string(), normalized.updated_at, &normalized)
    }

    pub fn delete_template(&self, id: SharedTemplateId) -> Result<(), DbError> {
        self.delete_simple("shared_template", &id.to_string())
    }

    pub fn upsert_contact(&self, id: ContactId, contact: &ShowContactView) -> Result<(), DbError> {
        let mut normalized = contact.clone();
        normalized.id = id;
        self.upsert_simple("show_contact", &id.to_string(), normalized.updated_at, &normalized)
    }

    pub fn delete_contact(&self, id: ContactId) -> Result<(), DbError> {
        self.delete_simple("show_contact", &id.to_string())
    }

    pub fn upsert_pinned_link(&self, id: PinnedLinkId, link: &PinnedLinkView) -> Result<(), DbError> {
        let mut normalized = link.clone();
        normalized.id = id;
        self.upsert_simple("pinned_link", &id.to_string(), normalized.updated_at, &normalized)
    }

    pub fn delete_pinned_link(&self, id: PinnedLinkId) -> Result<(), DbError> {
        self.delete_simple("pinned_link", &id.to_string())
    }

    pub fn upsert_calendar_event(&self, id: CalendarEventId, event: &CalendarEventView) -> Result<(), DbError> {
        let mut normalized = event.clone();
        normalized.id = id;
        self.upsert_simple("calendar_event", &id.to_string(), normalized.updated_at, &normalized)
    }

    pub fn delete_calendar_event(&self, id: CalendarEventId) -> Result<(), DbError> {
        self.delete_simple("calendar_event", &id.to_string())
    }

    pub fn upsert_meeting_doc(&self, event_id: CalendarEventId, doc: &MeetingDocView) -> Result<(), DbError> {
        let mut normalized = doc.clone();
        normalized.event_id = event_id;
        let eid = event_id.to_string();
        let json = serde_json::to_string(&normalized)?;
        self.with_conn(|conn| {
            let existing: Option<(i64, i64)> = conn
                .query_row(
                    "SELECT rev, updated_at FROM meeting_doc WHERE event_id = ?1",
                    params![eid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((cur_rev, cur_ts)) = existing {
                let incoming_rev = i64::from(normalized.rev);
                if incoming_rev < cur_rev || (incoming_rev == cur_rev && normalized.updated_at.0 < cur_ts) {
                    return Err(DbError::Conflict {
                        current_rev: cur_rev.max(1) as u32,
                    });
                }
            }
            conn.execute(
                "INSERT INTO meeting_doc (event_id, rev, updated_at, payload_json)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(event_id) DO UPDATE SET
                    rev = excluded.rev,
                    updated_at = excluded.updated_at,
                    payload_json = excluded.payload_json",
                params![eid, i64::from(normalized.rev), normalized.updated_at.0, json],
            )?;
            Ok(())
        })
    }

    pub fn upsert_action_item(&self, id: ActionItemId, item: &ActionItemView) -> Result<(), DbError> {
        let mut normalized = item.clone();
        normalized.id = id;
        self.upsert_simple("action_item", &id.to_string(), normalized.updated_at, &normalized)
    }

    pub fn delete_action_item(&self, id: ActionItemId) -> Result<(), DbError> {
        self.delete_simple("action_item", &id.to_string())
    }

    fn upsert_simple<T: Serialize>(
        &self,
        table: &'static str,
        id: &str,
        updated_at: UnixMs,
        value: &T,
    ) -> Result<(), DbError> {
        let json = serde_json::to_string(value)?;
        let sql = format!(
            "INSERT INTO {table} (id, updated_at, payload_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                updated_at = excluded.updated_at,
                payload_json = excluded.payload_json"
        );
        self.with_conn(|conn| {
            conn.execute(&sql, params![id, updated_at.0, json])?;
            Ok(())
        })
    }

    fn delete_simple(&self, table: &'static str, id: &str) -> Result<(), DbError> {
        let sql = format!("DELETE FROM {table} WHERE id = ?1");
        self.with_conn(|conn| {
            conn.execute(&sql, params![id])?;
            Ok(())
        })
    }
}

fn upsert_draft_inner(conn: &mut Connection, draft: &SharedDraftView) -> Result<(), DbError> {
    let tid = draft.shared_thread_id.to_string();
    let existing: Option<(i64, i64)> = conn
        .query_row(
            "SELECT rev, updated_at FROM shared_draft WHERE shared_thread_id = ?1",
            params![tid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((cur_rev, cur_ts)) = existing {
        let incoming_rev = i64::from(draft.rev);
        if incoming_rev < cur_rev || (incoming_rev == cur_rev && draft.updated_at.0 < cur_ts) {
            return Err(DbError::Conflict {
                current_rev: cur_rev.max(1) as u32,
            });
        }
    }
    let json = serde_json::to_string(draft)?;
    conn.execute(
        "INSERT INTO shared_draft (shared_thread_id, rev, updated_at, payload_json)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(shared_thread_id) DO UPDATE SET
            rev = excluded.rev,
            updated_at = excluded.updated_at,
            payload_json = excluded.payload_json",
        params![tid, i64::from(draft.rev), draft.updated_at.0, json],
    )?;
    Ok(())
}

fn query_json_rows<T: DeserializeOwned>(
    conn: &mut Connection,
    sql: &str,
    min_ts: i64,
) -> Result<Vec<T>, DbError> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params![min_ts])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        out.push(serde_json::from_str(&raw)?);
    }
    Ok(out)
}

fn query_comments(conn: &mut Connection, tid: &str) -> Result<Vec<SharedCommentView>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT payload_json FROM shared_comment WHERE shared_thread_id = ?1 ORDER BY created_at ASC",
    )?;
    let mut rows = stmt.query(params![tid])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        out.push(serde_json::from_str(&raw)?);
    }
    Ok(out)
}

fn query_draft(conn: &mut Connection, tid: &str) -> Result<Option<SharedDraftView>, DbError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM shared_draft WHERE shared_thread_id = ?1",
            params![tid],
            |r| r.get(0),
        )
        .optional()?;
    match raw {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}
