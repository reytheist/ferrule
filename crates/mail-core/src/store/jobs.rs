//! Timer job persistence: outbox sends, snoozes, follow-ups.

use rusqlite::{params, Connection, Row};

use crate::error::CoreError;
use crate::ids::{AccountId, DraftId, JobId, ThreadId, UnixMs};
use crate::store::{id_to_blob, query_all, query_one, row_id, SqlResultExt};
use crate::types::JobView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobKind {
    Send,
    Snooze,
    FollowUp,
}

impl JobKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Snooze => "snooze",
            Self::FollowUp => "follow_up",
        }
    }

    fn from_db(raw: &str) -> Result<Self, CoreError> {
        match raw {
            "send" => Ok(Self::Send),
            "snooze" => Ok(Self::Snooze),
            "follow_up" => Ok(Self::FollowUp),
            other => Err(CoreError::Storage {
                reason: format!("unknown job kind: {other}"),
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct JobRow {
    pub id: JobId,
    pub kind: JobKind,
    pub account_id: AccountId,
    pub thread_id: Option<ThreadId>,
    pub draft_id: Option<DraftId>,
    pub due_at: UnixMs,
    pub live: bool,
    pub state_json: String,
    pub summary: String,
    pub payload: Option<String>,
    pub message_id_hdr: Option<String>,
}

impl JobRow {
    pub(crate) fn state_kind(&self) -> String {
        serde_json::from_str::<serde_json::Value>(&self.state_json)
            .ok()
            .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub(crate) fn to_view(&self) -> JobView {
        JobView {
            id: self.id,
            kind: self.kind.as_str().to_string(),
            account_id: self.account_id,
            thread_id: self.thread_id,
            due_at: self.due_at,
            state: self.state_kind(),
            summary: self.summary.clone(),
        }
    }
}

const JOB_COLS: &str =
    "id, kind, account_id, thread_id, draft_id, due_at, live, state_json, summary, payload, message_id_hdr";

fn map_job(row: &Row<'_>) -> Result<JobRow, CoreError> {
    let kind_raw: String = row.get(1).sq()?;
    Ok(JobRow {
        id: row_id(row, 0, JobId::from_bytes)?,
        kind: JobKind::from_db(&kind_raw)?,
        account_id: row_id(row, 2, AccountId::from_bytes)?,
        thread_id: match row.get::<_, Option<Vec<u8>>>(3).sq()? {
            Some(bytes) => Some(crate::store::blob_to_id(&bytes, ThreadId::from_bytes)?),
            None => None,
        },
        draft_id: match row.get::<_, Option<Vec<u8>>>(4).sq()? {
            Some(bytes) => Some(crate::store::blob_to_id(&bytes, DraftId::from_bytes)?),
            None => None,
        },
        due_at: UnixMs(row.get(5).sq()?),
        live: row.get::<_, i64>(6).sq()? != 0,
        state_json: row.get(7).sq()?,
        summary: row.get(8).sq()?,
        payload: row.get(9).sq()?,
        message_id_hdr: row.get(10).sq()?,
    })
}

pub(crate) struct NewJob {
    pub id: JobId,
    pub kind: JobKind,
    pub account_id: AccountId,
    pub thread_id: Option<ThreadId>,
    pub draft_id: Option<DraftId>,
    pub due_at: UnixMs,
    pub live: bool,
    pub state_json: String,
    pub summary: String,
    pub payload: Option<String>,
    pub message_id_hdr: Option<String>,
    pub created_at: UnixMs,
}

pub(crate) fn insert_job(conn: &mut Connection, job: &NewJob) -> Result<JobId, CoreError> {
    conn.execute(
        "INSERT INTO job (id, kind, account_id, thread_id, draft_id, due_at, live, state_json, summary, payload, message_id_hdr, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id_to_blob(job.id.as_bytes()),
            job.kind.as_str(),
            id_to_blob(job.account_id.as_bytes()),
            job.thread_id.map(|t| id_to_blob(t.as_bytes())),
            job.draft_id.map(|d| id_to_blob(d.as_bytes())),
            job.due_at.0,
            job.live as i64,
            job.state_json,
            job.summary,
            job.payload,
            job.message_id_hdr,
            job.created_at.0,
        ],
    )
    .sq()?;
    Ok(job.id)
}

pub(crate) fn get_job(conn: &mut Connection, id: JobId) -> Result<Option<JobRow>, CoreError> {
    let sql = format!("SELECT {JOB_COLS} FROM job WHERE id = ?1");
    query_one(conn, &sql, params![id_to_blob(id.as_bytes())], map_job)
}

pub(crate) fn update_job_state(
    conn: &mut Connection,
    id: JobId,
    state_json: &str,
    due_at: UnixMs,
    live: bool,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE job SET state_json = ?2, due_at = ?3, live = ?4 WHERE id = ?1",
        params![id_to_blob(id.as_bytes()), state_json, due_at.0, live as i64],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn list_live_jobs(conn: &mut Connection) -> Result<Vec<JobRow>, CoreError> {
    let sql =
        format!("SELECT {JOB_COLS} FROM job WHERE live = 1 ORDER BY due_at ASC, created_at ASC");
    query_all(conn, &sql, [], map_job)
}

/// Live jobs whose deadline has passed (plus in-flight sends, whose lease governs them).
pub(crate) fn due_jobs(conn: &mut Connection, now: UnixMs) -> Result<Vec<JobRow>, CoreError> {
    let sql = format!(
        "SELECT {JOB_COLS} FROM job WHERE live = 1 AND due_at <= ?1 ORDER BY due_at ASC, created_at ASC"
    );
    query_all(conn, &sql, params![now.0], map_job)
}

fn live_job_for_thread(
    conn: &mut Connection,
    thread_id: ThreadId,
    kind: JobKind,
    state_filter: Option<&str>,
) -> Result<Option<JobRow>, CoreError> {
    let state_clause = match state_filter {
        Some(filter) => format!("AND json_extract(state_json, '$.kind') IN ({filter})"),
        None => String::new(),
    };
    let sql = format!(
        "SELECT {JOB_COLS} FROM job
         WHERE thread_id = ?1 AND kind = ?2 AND live = 1 {state_clause}
         ORDER BY due_at ASC LIMIT 1"
    );
    query_one(
        conn,
        &sql,
        params![id_to_blob(thread_id.as_bytes()), kind.as_str()],
        map_job,
    )
}

pub(crate) fn live_snooze_for_thread(
    conn: &mut Connection,
    thread_id: ThreadId,
) -> Result<Option<JobRow>, CoreError> {
    live_job_for_thread(conn, thread_id, JobKind::Snooze, Some("'armed'"))
}

pub(crate) fn live_follow_up_for_thread(
    conn: &mut Connection,
    thread_id: ThreadId,
) -> Result<Option<JobRow>, CoreError> {
    live_job_for_thread(conn, thread_id, JobKind::FollowUp, Some("'armed','fired'"))
}
