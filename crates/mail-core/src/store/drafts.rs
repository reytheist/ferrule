//! Draft persistence (human and agent-originated).

use rusqlite::{params, Connection, Row};

use crate::error::CoreError;
use crate::ids::{AccountId, DraftId, IdentityId, ThreadId, UnixMs};
use crate::store::{
    from_json, get_account, id_to_blob, identity, query_one, row_id, to_json, SqlResultExt,
};
use crate::types::{DraftOrigin, DraftState, DraftView};

pub(crate) struct DraftRow {
    pub id: DraftId,
    pub account_id: AccountId,
    pub from_identity_id: IdentityId,
    pub from_locked: bool,
    pub reply_to_thread: Option<ThreadId>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    pub rev: u32,
    pub origin: DraftOrigin,
    pub state: DraftState,
}

const DRAFT_COLS: &str = "id, account_id, from_identity_id, from_locked, reply_to_thread, to_json, cc_json, bcc_json, subject, body_text, rev, origin_kind, origin_agent, rationale, state_json";

fn map_draft(row: &Row<'_>) -> Result<DraftRow, CoreError> {
    let to_raw: String = row.get(5).sq()?;
    let cc_raw: String = row.get(6).sq()?;
    let bcc_raw: String = row.get(7).sq()?;
    let origin_kind: String = row.get(11).sq()?;
    let origin_agent: Option<String> = row.get(12).sq()?;
    let rationale: Option<String> = row.get(13).sq()?;
    let state_raw: String = row.get(14).sq()?;
    Ok(DraftRow {
        id: row_id(row, 0, DraftId::from_bytes)?,
        account_id: row_id(row, 1, AccountId::from_bytes)?,
        from_identity_id: row_id(row, 2, IdentityId::from_bytes)?,
        from_locked: row.get::<_, i64>(3).sq()? != 0,
        reply_to_thread: match row.get::<_, Option<Vec<u8>>>(4).sq()? {
            Some(bytes) => Some(blob_to_thread(&bytes)?),
            None => None,
        },
        to: from_json(&to_raw)?,
        cc: from_json(&cc_raw)?,
        bcc: from_json(&bcc_raw)?,
        subject: row.get(8).sq()?,
        body_text: row.get(9).sq()?,
        rev: row.get::<_, i64>(10).sq()?.max(0) as u32,
        origin: match origin_kind.as_str() {
            "agent" => DraftOrigin::Agent {
                agent_id: origin_agent.unwrap_or_default(),
                rationale: rationale.unwrap_or_default(),
            },
            _ => DraftOrigin::Human,
        },
        state: from_json(&state_raw)?,
    })
}

fn blob_to_thread(bytes: &[u8]) -> Result<ThreadId, CoreError> {
    crate::store::blob_to_id(bytes, ThreadId::from_bytes)
}

pub(crate) struct InsertDraft {
    pub account_id: AccountId,
    pub from_identity_id: IdentityId,
    pub from_locked: bool,
    pub reply_to_thread: Option<ThreadId>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    pub origin: DraftOrigin,
    pub state: DraftState,
    pub updated_at: UnixMs,
}

pub(crate) fn insert_draft(conn: &mut Connection, d: &InsertDraft) -> Result<DraftId, CoreError> {
    let (origin_kind, origin_agent, rationale) = match &d.origin {
        DraftOrigin::Human => ("human".to_string(), None, None),
        DraftOrigin::Agent {
            agent_id,
            rationale,
        } => (
            "agent".to_string(),
            Some(agent_id.clone()),
            Some(rationale.clone()),
        ),
    };
    let id = DraftId::fresh();
    conn.execute(
        "INSERT INTO draft (id, account_id, from_identity_id, from_locked, reply_to_thread, to_json, cc_json, bcc_json, subject, body_text, rev, origin_kind, origin_agent, rationale, state_json, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?12, ?13, ?14, ?15)",
        params![
            id_to_blob(id.as_bytes()),
            id_to_blob(d.account_id.as_bytes()),
            id_to_blob(d.from_identity_id.as_bytes()),
            d.from_locked as i64,
            d.reply_to_thread.map(|t| id_to_blob(t.as_bytes())),
            to_json(&d.to)?,
            to_json(&d.cc)?,
            to_json(&d.bcc)?,
            d.subject,
            d.body_text,
            origin_kind,
            origin_agent,
            rationale,
            to_json(&d.state)?,
            d.updated_at.0,
        ],
    )
    .sq()?;
    Ok(id)
}

pub(crate) fn get_draft(conn: &mut Connection, id: DraftId) -> Result<Option<DraftRow>, CoreError> {
    let sql = format!("SELECT {DRAFT_COLS} FROM draft WHERE id = ?1");
    query_one(conn, &sql, params![id_to_blob(id.as_bytes())], map_draft)
}

pub(crate) fn set_draft_state(
    conn: &mut Connection,
    id: DraftId,
    state: &DraftState,
    updated_at: UnixMs,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE draft SET state_json = ?2, updated_at = ?3 WHERE id = ?1",
        params![id_to_blob(id.as_bytes()), to_json(state)?, updated_at.0],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn draft_view(conn: &mut Connection, draft: &DraftRow) -> Result<DraftView, CoreError> {
    let account = get_account(conn, draft.account_id)?.ok_or(CoreError::NotFound {
        entity: "account".to_string(),
    })?;
    let ident = identity(conn, draft.from_identity_id)?.ok_or(CoreError::NotFound {
        entity: "identity".to_string(),
    })?;
    if ident.account_id != draft.account_id {
        return Err(CoreError::IdentityMismatch);
    }
    Ok(DraftView {
        id: draft.id,
        account_id: draft.account_id,
        space_color: account.color,
        from_identity: ident.to_view(),
        from_locked: draft.from_locked,
        reply_to_thread: draft.reply_to_thread,
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        bcc: draft.bcc.clone(),
        subject: draft.subject.clone(),
        body_text: draft.body_text.clone(),
        rev: draft.rev,
        origin: draft.origin.clone(),
        state: draft.state.clone(),
    })
}
