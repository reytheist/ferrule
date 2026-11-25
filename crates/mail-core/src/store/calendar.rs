//! Local-first SQLite persistence for Phase C: Pigment Shared Calendar,
//! Meeting Docs, Action Items (with recurring carryover), inline .ics invites,
//! and strictly local-only Personal .ics Overlays.

use rusqlite::{params, Connection, Row};

use crate::calendar::{
    ActionItemStatus, ActionItemView, AgendaItem, CalendarEventKind, CalendarEventView,
    CalendarInviteView, IcsOverlayView, MeetingDocView, OverlayBusyBlock, ParsedIcsEvent,
    RecurrenceRule, RsvpStatus, SaveCalendarEventInput, SaveIcsOverlayInput,
};
use crate::error::CoreError;
use crate::ids::{ActionItemId, CalendarEventId, IcsOverlayId, MessageId, ThreadId, UnixMs};
use crate::store::{
    blob_to_id, from_json, id_to_blob, query_all, query_one, row_id, to_json, SqlResultExt,
};

// ── Calendar Events ─────────────────────────────────────────────────────────

fn map_calendar_event(row: &Row<'_>) -> Result<CalendarEventView, CoreError> {
    let attendees_raw: String = row.get(12).sq()?;
    Ok(CalendarEventView {
        id: row_id(row, 0, CalendarEventId::from_bytes)?,
        series_id: row.get(1).sq()?,
        title: row.get(2).sq()?,
        description: row.get(3).sq()?,
        start_at: UnixMs(row.get(4).sq()?),
        end_at: UnixMs(row.get(5).sq()?),
        all_day: row.get::<_, i64>(6).sq()? != 0,
        kind: CalendarEventKind::from_db(&row.get::<_, String>(7).sq()?),
        department: row.get(8).sq()?,
        location: row.get(9).sq()?,
        recurrence: RecurrenceRule::from_db(&row.get::<_, String>(10).sq()?),
        discord_url: row.get(11).sq()?,
        attendees: from_json(&attendees_raw)?,
        source_ics_uid: row.get(13).sq()?,
        created_by: row.get(14).sq()?,
        updated_at: UnixMs(row.get(15).sq()?),
    })
}

const EVENT_COLS: &str = "id, series_id, title, description, start_at, end_at, all_day, kind, department, location, recurrence, discord_url, attendees_json, source_ics_uid, created_by, updated_at";

pub(crate) fn list_all_calendar_events(
    conn: &mut Connection,
) -> Result<Vec<CalendarEventView>, CoreError> {
    let sql = format!("SELECT {EVENT_COLS} FROM calendar_event ORDER BY start_at ASC");
    query_all(conn, &sql, [], map_calendar_event)
}

pub(crate) fn get_calendar_event(
    conn: &mut Connection,
    id: CalendarEventId,
) -> Result<Option<CalendarEventView>, CoreError> {
    let sql = format!("SELECT {EVENT_COLS} FROM calendar_event WHERE id = ?1");
    query_one(
        conn,
        &sql,
        params![id_to_blob(id.as_bytes())],
        map_calendar_event,
    )
}

pub(crate) fn upsert_calendar_event_row(
    conn: &mut Connection,
    event: &CalendarEventView,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO calendar_event (
            id, series_id, title, description, start_at, end_at, all_day, kind,
            department, location, recurrence, discord_url, attendees_json,
            source_ics_uid, created_by, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
         ON CONFLICT(id) DO UPDATE SET
            series_id = excluded.series_id,
            title = excluded.title,
            description = excluded.description,
            start_at = excluded.start_at,
            end_at = excluded.end_at,
            all_day = excluded.all_day,
            kind = excluded.kind,
            department = excluded.department,
            location = excluded.location,
            recurrence = excluded.recurrence,
            discord_url = excluded.discord_url,
            attendees_json = excluded.attendees_json,
            source_ics_uid = excluded.source_ics_uid,
            updated_at = excluded.updated_at",
        params![
            id_to_blob(event.id.as_bytes()),
            event.series_id,
            event.title,
            event.description,
            event.start_at.0,
            event.end_at.0,
            if event.all_day { 1 } else { 0 },
            event.kind.as_str(),
            event.department,
            event.location,
            event.recurrence.as_str(),
            event.discord_url,
            to_json(&event.attendees)?,
            event.source_ics_uid,
            event.created_by,
            event.updated_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn save_calendar_event(
    conn: &mut Connection,
    input: &SaveCalendarEventInput,
    created_by: &str,
    now: UnixMs,
) -> Result<CalendarEventView, CoreError> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "title".to_string(),
            reason: "event title is required".to_string(),
        });
    }
    if input.end_at.0 <= input.start_at.0 {
        return Err(CoreError::InvalidInput {
            field: "endAt".to_string(),
            reason: "event end time must be after start time".to_string(),
        });
    }
    if let Some(url) = input
        .discord_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        let valid = url.starts_with("discord://")
            || url.starts_with("https://discord.com/")
            || url.starts_with("https://discord.gg/")
            || url.starts_with("https://ptb.discord.com/")
            || url.starts_with("https://canary.discord.com/");
        if !valid {
            return Err(CoreError::InvalidInput {
                field: "discordUrl".to_string(),
                reason: "Discord link must start with discord:// or https://discord.com/ (or discord.gg/)"
                    .to_string(),
            });
        }
    }

    let id = input.id.unwrap_or_else(CalendarEventId::fresh);
    let series_id = input
        .series_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            if input.recurrence != RecurrenceRule::None {
                Some(format!("series-{id}"))
            } else {
                None
            }
        });

    let event = CalendarEventView {
        id,
        series_id,
        title: title.to_string(),
        description: input.description.trim().to_string(),
        start_at: input.start_at,
        end_at: input.end_at,
        all_day: input.all_day,
        kind: input.kind,
        department: input
            .department
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches('#').to_ascii_lowercase()),
        location: input
            .location
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        recurrence: input.recurrence,
        discord_url: input
            .discord_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        attendees: input
            .attendees
            .iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect(),
        source_ics_uid: input.source_ics_uid.clone(),
        created_by: created_by.to_string(),
        updated_at: now,
    };
    upsert_calendar_event_row(conn, &event)?;
    Ok(event)
}

pub(crate) fn delete_calendar_event(
    conn: &mut Connection,
    id: CalendarEventId,
) -> Result<bool, CoreError> {
    let rows = conn
        .execute(
            "DELETE FROM calendar_event WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(rows > 0)
}

// ── Meeting Docs & Action Items (with Recurring Carryover) ──────────────────

fn map_action_item(row: &Row<'_>) -> Result<ActionItemView, CoreError> {
    let calendar_event_id = row
        .get::<_, Option<Vec<u8>>>(11)
        .sq()?
        .map(|b| blob_to_id(&b, CalendarEventId::from_bytes))
        .transpose()?;
    let source_event_id = row
        .get::<_, Option<Vec<u8>>>(12)
        .sq()?
        .map(|b| blob_to_id(&b, CalendarEventId::from_bytes))
        .transpose()?;
    let source_thread_id = row
        .get::<_, Option<Vec<u8>>>(14)
        .sq()?
        .map(|b| blob_to_id(&b, ThreadId::from_bytes))
        .transpose()?;
    Ok(ActionItemView {
        id: row_id(row, 0, ActionItemId::from_bytes)?,
        title: row.get(1).sq()?,
        department: row.get(2).sq()?,
        milestone: row.get(3).sq()?,
        assignee_handle: row.get(4).sq()?,
        status: ActionItemStatus::from_db(&row.get::<_, String>(5).sq()?),
        due_at: row.get::<_, Option<i64>>(6).sq()?.map(UnixMs),
        start_time: row.get(7).sq()?,
        end_time: row.get(8).sq()?,
        location: row.get(9).sq()?,
        cost_note: row.get(10).sq()?,
        calendar_event_id,
        source_event_id,
        source_series_id: row.get(13).sq()?,
        source_thread_id,
        created_by: row.get(15).sq()?,
        created_at: UnixMs(row.get(16).sq()?),
        updated_at: UnixMs(row.get(17).sq()?),
    })
}

const ACTION_COLS: &str = "id, title, department, milestone, assignee_handle, status, due_at, start_time, end_time, location, cost_note, calendar_event_id, source_event_id, source_series_id, source_thread_id, created_by, created_at, updated_at";

pub(crate) fn list_action_items(conn: &mut Connection) -> Result<Vec<ActionItemView>, CoreError> {
    let sql = format!(
        "SELECT {ACTION_COLS} FROM action_item ORDER BY status ASC, COALESCE(due_at, 9223372036854775807) ASC, created_at DESC"
    );
    query_all(conn, &sql, [], map_action_item)
}

pub(crate) fn get_action_item(
    conn: &mut Connection,
    id: ActionItemId,
) -> Result<Option<ActionItemView>, CoreError> {
    let sql = format!("SELECT {ACTION_COLS} FROM action_item WHERE id = ?1");
    query_one(
        conn,
        &sql,
        params![id_to_blob(id.as_bytes())],
        map_action_item,
    )
}

pub(crate) fn upsert_action_item_row(
    conn: &mut Connection,
    item: &ActionItemView,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO action_item (
            id, title, department, milestone, assignee_handle, status, due_at,
            start_time, end_time, location, cost_note, calendar_event_id,
            source_event_id, source_series_id, source_thread_id,
            created_by, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            department = excluded.department,
            milestone = excluded.milestone,
            assignee_handle = excluded.assignee_handle,
            status = excluded.status,
            due_at = excluded.due_at,
            start_time = excluded.start_time,
            end_time = excluded.end_time,
            location = excluded.location,
            cost_note = excluded.cost_note,
            calendar_event_id = excluded.calendar_event_id,
            source_event_id = excluded.source_event_id,
            source_series_id = excluded.source_series_id,
            source_thread_id = excluded.source_thread_id,
            updated_at = excluded.updated_at",
        params![
            id_to_blob(item.id.as_bytes()),
            item.title,
            item.department,
            item.milestone,
            item.assignee_handle,
            item.status.as_str(),
            item.due_at.map(|t| t.0),
            item.start_time,
            item.end_time,
            item.location,
            item.cost_note,
            item.calendar_event_id.map(|id| id_to_blob(id.as_bytes())),
            item.source_event_id.map(|id| id_to_blob(id.as_bytes())),
            item.source_series_id,
            item.source_thread_id.map(|id| id_to_blob(id.as_bytes())),
            item.created_by,
            item.created_at.0,
            item.updated_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn delete_action_item(
    conn: &mut Connection,
    id: ActionItemId,
) -> Result<bool, CoreError> {
    let rows = conn
        .execute(
            "DELETE FROM action_item WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(rows > 0)
}

pub(crate) fn update_action_item_status(
    conn: &mut Connection,
    id: ActionItemId,
    status: ActionItemStatus,
    now: UnixMs,
) -> Result<Option<ActionItemView>, CoreError> {
    let rows = conn
        .execute(
            "UPDATE action_item SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.as_str(), now.0, id_to_blob(id.as_bytes())],
        )
        .sq()?;
    if rows == 0 {
        return Ok(None);
    }
    get_action_item(conn, id)
}

pub(crate) fn get_meeting_doc(
    conn: &mut Connection,
    event_id: CalendarEventId,
    now: UnixMs,
) -> Result<Option<MeetingDocView>, CoreError> {
    let Some(event) = get_calendar_event(conn, event_id)? else {
        return Ok(None);
    };

    let row = query_one(
        conn,
        "SELECT series_id, agenda_json, notes_markdown, notes_editor_handle, notes_editor_name, rev, updated_at
         FROM meeting_doc WHERE event_id = ?1",
        params![id_to_blob(event_id.as_bytes())],
        |r| {
            let agenda_raw: String = r.get(1).sq()?;
            Ok((
                r.get::<_, Option<String>>(0).sq()?,
                from_json::<Vec<AgendaItem>>(&agenda_raw)?,
                r.get::<_, String>(2).sq()?,
                r.get::<_, Option<String>>(3).sq()?,
                r.get::<_, Option<String>>(4).sq()?,
                r.get::<_, u32>(5).sq()?,
                UnixMs(r.get(6).sq()?),
            ))
        },
    )?;

    let (series_id, agenda, notes_markdown, notes_editor_handle, notes_editor_name, rev, updated_at) =
        row.unwrap_or_else(|| {
            (
                event.series_id.clone(),
                Vec::new(),
                String::new(),
                None,
                None,
                1,
                now,
            )
        });

    let effective_series = series_id.or(event.series_id.clone());

    let action_items = query_all(
        conn,
        &format!(
            "SELECT {ACTION_COLS} FROM action_item WHERE source_event_id = ?1 ORDER BY created_at ASC"
        ),
        params![id_to_blob(event_id.as_bytes())],
        map_action_item,
    )?;

    // Automatic carryover: unfinished (status = 'open') action items from earlier
    // occurrences in the same recurring series (source_event_id != event_id).
    let carryover_items = if let Some(series) = &effective_series {
        query_all(
            conn,
            &format!(
                "SELECT {ACTION_COLS} FROM action_item
                 WHERE source_series_id = ?1
                   AND status = 'open'
                   AND (source_event_id IS NULL OR source_event_id != ?2)
                 ORDER BY created_at ASC"
            ),
            params![series, id_to_blob(event_id.as_bytes())],
            map_action_item,
        )?
    } else {
        Vec::new()
    };

    Ok(Some(MeetingDocView {
        event_id,
        series_id: effective_series,
        agenda,
        notes_markdown,
        notes_editor_handle,
        notes_editor_name,
        rev,
        updated_at,
        action_items,
        carryover_items,
    }))
}

pub(crate) fn upsert_meeting_doc_row(
    conn: &mut Connection,
    doc: &MeetingDocView,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO meeting_doc (
            event_id, series_id, agenda_json, notes_markdown,
            notes_editor_handle, notes_editor_name, rev, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(event_id) DO UPDATE SET
            series_id = excluded.series_id,
            agenda_json = excluded.agenda_json,
            notes_markdown = excluded.notes_markdown,
            notes_editor_handle = excluded.notes_editor_handle,
            notes_editor_name = excluded.notes_editor_name,
            rev = excluded.rev,
            updated_at = excluded.updated_at",
        params![
            id_to_blob(doc.event_id.as_bytes()),
            doc.series_id,
            to_json(&doc.agenda)?,
            doc.notes_markdown,
            doc.notes_editor_handle,
            doc.notes_editor_name,
            doc.rev,
            doc.updated_at.0,
        ],
    )
    .sq()?;
    Ok(())
}

// ── Inline .ics Message Invites ─────────────────────────────────────────────

fn map_invite(row: &Row<'_>) -> Result<CalendarInviteView, CoreError> {
    let added_event_id = row
        .get::<_, Option<Vec<u8>>>(11)
        .sq()?
        .map(|b| blob_to_id(&b, CalendarEventId::from_bytes))
        .transpose()?;
    Ok(CalendarInviteView {
        message_id: row_id(row, 0, MessageId::from_bytes)?,
        uid: row.get(1).sq()?,
        summary: row.get(2).sq()?,
        description: row.get(3).sq()?,
        location: row.get(4).sq()?,
        organizer_name: row.get(5).sq()?,
        organizer_email: row.get(6).sq()?,
        start_at: UnixMs(row.get(7).sq()?),
        end_at: UnixMs(row.get(8).sq()?),
        all_day: row.get::<_, i64>(9).sq()? != 0,
        rsvp_status: RsvpStatus::from_db(&row.get::<_, String>(10).sq()?),
        added_event_id,
    })
}

const INVITE_COLS: &str = "message_id, uid, summary, description, location, organizer_name, organizer_email, start_at, end_at, all_day, rsvp_status, added_event_id";

pub(crate) fn upsert_message_invite(
    conn: &mut Connection,
    message_id: MessageId,
    parsed: &ParsedIcsEvent,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT OR IGNORE INTO message_invite (
            message_id, uid, summary, description, location,
            organizer_name, organizer_email, start_at, end_at, all_day, rsvp_status
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'needs_action')",
        params![
            id_to_blob(message_id.as_bytes()),
            parsed.uid,
            parsed.summary,
            parsed.description,
            parsed.location,
            parsed.organizer_name,
            parsed.organizer_email,
            parsed.start_at.0,
            parsed.end_at.0,
            if parsed.all_day { 1 } else { 0 },
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn load_message_invites(
    conn: &mut Connection,
    message_id: MessageId,
) -> Result<Vec<CalendarInviteView>, CoreError> {
    let sql = format!(
        "SELECT {INVITE_COLS} FROM message_invite WHERE message_id = ?1 ORDER BY start_at ASC"
    );
    query_all(
        conn,
        &sql,
        params![id_to_blob(message_id.as_bytes())],
        map_invite,
    )
}

pub(crate) fn get_message_invite(
    conn: &mut Connection,
    message_id: MessageId,
    uid: &str,
) -> Result<Option<CalendarInviteView>, CoreError> {
    let sql = format!("SELECT {INVITE_COLS} FROM message_invite WHERE message_id = ?1 AND uid = ?2");
    query_one(
        conn,
        &sql,
        params![id_to_blob(message_id.as_bytes()), uid],
        map_invite,
    )
}

pub(crate) fn update_invite_rsvp(
    conn: &mut Connection,
    message_id: MessageId,
    uid: &str,
    status: RsvpStatus,
) -> Result<Option<CalendarInviteView>, CoreError> {
    let rows = conn
        .execute(
            "UPDATE message_invite SET rsvp_status = ?1 WHERE message_id = ?2 AND uid = ?3",
            params![status.as_str(), id_to_blob(message_id.as_bytes()), uid],
        )
        .sq()?;
    if rows == 0 {
        return Ok(None);
    }
    get_message_invite(conn, message_id, uid)
}

pub(crate) fn link_invite_event(
    conn: &mut Connection,
    message_id: MessageId,
    uid: &str,
    event_id: CalendarEventId,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE message_invite SET added_event_id = ?1 WHERE message_id = ?2 AND uid = ?3",
        params![
            id_to_blob(event_id.as_bytes()),
            id_to_blob(message_id.as_bytes()),
            uid,
        ],
    )
    .sq()?;
    Ok(())
}

// ── Local-Only Personal .ics Overlays ───────────────────────────────────────

pub(crate) fn list_ics_overlays(conn: &mut Connection) -> Result<Vec<IcsOverlayView>, CoreError> {
    query_all(
        conn,
        "SELECT o.id, o.name, o.url, o.color, o.enabled, o.last_fetched_at, o.last_error,
                (SELECT COUNT(*) FROM ics_overlay_event e WHERE e.overlay_id = o.id)
         FROM ics_overlay o
         ORDER BY o.name ASC",
        [],
        |r| {
            Ok(IcsOverlayView {
                id: row_id(r, 0, IcsOverlayId::from_bytes)?,
                name: r.get(1).sq()?,
                url: r.get(2).sq()?,
                color: r.get(3).sq()?,
                enabled: r.get::<_, i64>(4).sq()? != 0,
                last_fetched_at: r.get::<_, Option<i64>>(5).sq()?.map(UnixMs),
                last_error: r.get(6).sq()?,
                event_count: r.get::<_, u32>(7).sq()?,
            })
        },
    )
}

pub(crate) fn get_ics_overlay(
    conn: &mut Connection,
    id: IcsOverlayId,
) -> Result<Option<IcsOverlayView>, CoreError> {
    query_one(
        conn,
        "SELECT o.id, o.name, o.url, o.color, o.enabled, o.last_fetched_at, o.last_error,
                (SELECT COUNT(*) FROM ics_overlay_event e WHERE e.overlay_id = o.id)
         FROM ics_overlay o
         WHERE o.id = ?1",
        params![id_to_blob(id.as_bytes())],
        |r| {
            Ok(IcsOverlayView {
                id: row_id(r, 0, IcsOverlayId::from_bytes)?,
                name: r.get(1).sq()?,
                url: r.get(2).sq()?,
                color: r.get(3).sq()?,
                enabled: r.get::<_, i64>(4).sq()? != 0,
                last_fetched_at: r.get::<_, Option<i64>>(5).sq()?.map(UnixMs),
                last_error: r.get(6).sq()?,
                event_count: r.get::<_, u32>(7).sq()?,
            })
        },
    )
}

pub(crate) fn save_ics_overlay(
    conn: &mut Connection,
    input: &SaveIcsOverlayInput,
) -> Result<IcsOverlayView, CoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "name".to_string(),
            reason: "overlay name is required".to_string(),
        });
    }
    let url = input.url.trim();
    let valid_url = url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("webcal://")
        || url.starts_with("local://");
    if !valid_url {
        return Err(CoreError::InvalidInput {
            field: "url".to_string(),
            reason: "feed URL must start with https://, http://, webcal://, or local://".to_string(),
        });
    }
    let id = input.id.unwrap_or_else(IcsOverlayId::fresh);
    let color = if input.color.trim().is_empty() {
        "violet"
    } else {
        input.color.trim()
    };
    conn.execute(
        "INSERT INTO ics_overlay (id, name, url, color, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            url = excluded.url,
            color = excluded.color,
            enabled = excluded.enabled",
        params![
            id_to_blob(id.as_bytes()),
            name,
            url,
            color,
            if input.enabled { 1 } else { 0 },
        ],
    )
    .sq()?;
    get_ics_overlay(conn, id)?.ok_or_else(|| CoreError::Internal {
        reason: "ics overlay disappeared after save".to_string(),
    })
}

pub(crate) fn delete_ics_overlay(
    conn: &mut Connection,
    id: IcsOverlayId,
) -> Result<bool, CoreError> {
    let rows = conn
        .execute(
            "DELETE FROM ics_overlay WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(rows > 0)
}

pub(crate) fn replace_overlay_events(
    conn: &mut Connection,
    overlay_id: IcsOverlayId,
    events: &[ParsedIcsEvent],
    fetched_at: UnixMs,
    error: Option<&str>,
) -> Result<(), CoreError> {
    if error.is_none() {
        conn.execute(
            "DELETE FROM ics_overlay_event WHERE overlay_id = ?1",
            params![id_to_blob(overlay_id.as_bytes())],
        )
        .sq()?;
        for ev in events {
            conn.execute(
                "INSERT OR REPLACE INTO ics_overlay_event (
                    overlay_id, uid, summary, start_at, end_at, all_day
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id_to_blob(overlay_id.as_bytes()),
                    ev.uid,
                    ev.summary,
                    ev.start_at.0,
                    ev.end_at.0,
                    if ev.all_day { 1 } else { 0 },
                ],
            )
            .sq()?;
        }
    }
    conn.execute(
        "UPDATE ics_overlay SET last_fetched_at = ?1, last_error = ?2 WHERE id = ?3",
        params![fetched_at.0, error, id_to_blob(overlay_id.as_bytes())],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn list_overlay_busy_blocks(
    conn: &mut Connection,
    range_start: UnixMs,
    range_end: UnixMs,
) -> Result<Vec<OverlayBusyBlock>, CoreError> {
    query_all(
        conn,
        "SELECT e.overlay_id, o.name, o.color, e.uid, e.summary, e.start_at, e.end_at, e.all_day
         FROM ics_overlay_event e
         JOIN ics_overlay o ON o.id = e.overlay_id
         WHERE o.enabled = 1 AND e.end_at >= ?1 AND e.start_at <= ?2
         ORDER BY e.start_at ASC",
        params![range_start.0, range_end.0],
        |r| {
            Ok(OverlayBusyBlock {
                overlay_id: row_id(r, 0, IcsOverlayId::from_bytes)?,
                overlay_name: r.get(1).sq()?,
                overlay_color: r.get(2).sq()?,
                uid: r.get(3).sq()?,
                summary: r.get(4).sq()?,
                start_at: UnixMs(r.get(5).sq()?),
                end_at: UnixMs(r.get(6).sq()?),
                all_day: r.get::<_, i64>(7).sq()? != 0,
            })
        },
    )
}

// ── Local Pigment Demo Seeding ──────────────────────────────────────────────

pub(crate) fn seed_local_calendar_demo(
    conn: &mut Connection,
    now: UnixMs,
) -> Result<(), CoreError> {
    let existing: i64 = conn
        .query_row("SELECT COUNT(*) FROM calendar_event", [], |r| r.get(0))
        .sq()?;
    if existing > 0 {
        return Ok(());
    }

    let day_ms: i64 = 86_400_000;
    let hour_ms: i64 = 3_600_000;
    // Align to start of current UTC day
    let base_day = (now.0 / day_ms) * day_ms;

    let series = "series-pigment-weekly-staff".to_string();
    let prev_sync_id = CalendarEventId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:demo:cal:prev-sync");
    let this_sync_id = CalendarEventId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:demo:cal:this-sync");
    let sponsor_deadline_id =
        CalendarEventId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:demo:cal:sponsor-deadline");
    let room_walk_id =
        CalendarEventId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:demo:cal:room-walk");

    let events = [
        CalendarEventView {
            id: prev_sync_id,
            series_id: Some(series.clone()),
            title: "Pigment Weekly Staff Sync".to_string(),
            description: "Previous week's staff operations sync.".to_string(),
            start_at: UnixMs(base_day - 7 * day_ms + 19 * hour_ms),
            end_at: UnixMs(base_day - 7 * day_ms + 20 * hour_ms),
            all_day: false,
            kind: CalendarEventKind::Meeting,
            department: Some("operations".to_string()),
            location: Some("Discord #staff-voice".to_string()),
            recurrence: RecurrenceRule::None,
            discord_url: Some("https://discord.com/channels/pigment-staff/staff-voice".to_string()),
            attendees: vec!["reyth".into(), "elena".into(), "marcus".into(), "priya".into()],
            source_ics_uid: None,
            created_by: "reyth".to_string(),
            updated_at: now,
        },
        CalendarEventView {
            id: this_sync_id,
            series_id: Some(series.clone()),
            title: "Pigment Weekly Staff Sync".to_string(),
            description: "Weekly operations, sponsor, and exhibition check-in.".to_string(),
            start_at: UnixMs(base_day + day_ms + 19 * hour_ms),
            end_at: UnixMs(base_day + day_ms + 20 * hour_ms),
            all_day: false,
            kind: CalendarEventKind::Meeting,
            department: Some("operations".to_string()),
            location: Some("Discord #staff-voice".to_string()),
            recurrence: RecurrenceRule::Weekly,
            discord_url: Some("https://discord.com/channels/pigment-staff/staff-voice".to_string()),
            attendees: vec!["reyth".into(), "elena".into(), "marcus".into(), "priya".into()],
            source_ics_uid: None,
            created_by: "reyth".to_string(),
            updated_at: now,
        },
        CalendarEventView {
            id: sponsor_deadline_id,
            series_id: None,
            title: "Gold Sponsor Deposit Deadline".to_string(),
            description: "Confirm Monument Hobbies & Vallejo booth deposits and logo files.".to_string(),
            start_at: UnixMs(base_day + 3 * day_ms + 17 * hour_ms),
            end_at: UnixMs(base_day + 3 * day_ms + 18 * hour_ms),
            all_day: false,
            kind: CalendarEventKind::Deadline,
            department: Some("sponsors".to_string()),
            location: None,
            recurrence: RecurrenceRule::None,
            discord_url: None,
            attendees: vec!["elena".into(), "reyth".into()],
            source_ics_uid: None,
            created_by: "elena".to_string(),
            updated_at: now,
        },
        CalendarEventView {
            id: room_walk_id,
            series_id: None,
            title: "Ballroom A & Paint Lab AV Walkthrough".to_string(),
            description: "Verify power drops, lighting rigs, and judging display table spacing.".to_string(),
            start_at: UnixMs(base_day + 2 * day_ms + 14 * hour_ms),
            end_at: UnixMs(base_day + 2 * day_ms + 15 * hour_ms + 1_800_000),
            all_day: false,
            kind: CalendarEventKind::RoomSchedule,
            department: Some("exhibition".to_string()),
            location: Some("Ballroom A".to_string()),
            recurrence: RecurrenceRule::None,
            discord_url: None,
            attendees: vec!["marcus".into(), "reyth".into()],
            source_ics_uid: None,
            created_by: "marcus".to_string(),
            updated_at: now,
        },
    ];

    for ev in &events {
        upsert_calendar_event_row(conn, ev)?;
    }

    // Seed an unfinished action item on the PREVIOUS weekly sync so Fellow-style
    // automatic carryover surfaces immediately on THIS week's sync!
    let carryover = ActionItemView {
        id: ActionItemId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:demo:act:carryover-1"),
        title: "Confirm Ballroom A power drop map with hotel AV".to_string(),
        department: Some("operations".to_string()),
        milestone: Some("PMS 2027".to_string()),
        assignee_handle: Some("marcus".to_string()),
        status: ActionItemStatus::Open,
        due_at: Some(UnixMs(base_day + 2 * day_ms)),
        start_time: Some("14:00".to_string()),
        end_time: Some("15:30".to_string()),
        location: Some("Ballroom A".to_string()),
        cost_note: Some("$450 AV drop".to_string()),
        calendar_event_id: Some(room_walk_id),
        source_event_id: Some(prev_sync_id),
        source_series_id: Some(series.clone()),
        source_thread_id: None,
        created_by: "reyth".to_string(),
        created_at: UnixMs(base_day - 7 * day_ms),
        updated_at: now,
    };
    upsert_action_item_row(conn, &carryover)?;

    let workbook_seed = [
        ActionItemView {
            id: ActionItemId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:demo:act:wb-1"),
            title: "Collect Gold Sponsor logos for program print".to_string(),
            department: Some("sponsors".to_string()),
            milestone: Some("PMS 2027".to_string()),
            assignee_handle: Some("maya".to_string()),
            status: ActionItemStatus::Open,
            due_at: Some(UnixMs(base_day + 3 * day_ms)),
            start_time: None,
            end_time: None,
            location: None,
            cost_note: None,
            calendar_event_id: Some(sponsor_deadline_id),
            source_event_id: Some(this_sync_id),
            source_series_id: Some(series.clone()),
            source_thread_id: None,
            created_by: "reyth".to_string(),
            created_at: UnixMs(base_day - 2 * day_ms),
            updated_at: now,
        },
        ActionItemView {
            id: ActionItemId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:demo:act:wb-2"),
            title: "Order 24 acrylic display plinths & velvet risers".to_string(),
            department: Some("shopping-list".to_string()),
            milestone: Some("PMS 2027".to_string()),
            assignee_handle: None,
            status: ActionItemStatus::Open,
            due_at: Some(UnixMs(base_day - day_ms)),
            start_time: None,
            end_time: None,
            location: Some("Exhibition Hall".to_string()),
            cost_note: Some("$680".to_string()),
            calendar_event_id: None,
            source_event_id: None,
            source_series_id: None,
            source_thread_id: None,
            created_by: "reyth".to_string(),
            created_at: UnixMs(base_day - 5 * day_ms),
            updated_at: now,
        },
        ActionItemView {
            id: ActionItemId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:demo:act:wb-3"),
            title: "Publish workshop registration banner & Discord announcement".to_string(),
            department: Some("marketing".to_string()),
            milestone: Some("PMS 2027".to_string()),
            assignee_handle: Some("reyth".to_string()),
            status: ActionItemStatus::Open,
            due_at: Some(UnixMs(base_day + 5 * day_ms)),
            start_time: Some("10:00".to_string()),
            end_time: Some("11:00".to_string()),
            location: None,
            cost_note: None,
            calendar_event_id: None,
            source_event_id: None,
            source_series_id: None,
            source_thread_id: None,
            created_by: "reyth".to_string(),
            created_at: UnixMs(base_day - 3 * day_ms),
            updated_at: now,
        },
        ActionItemView {
            id: ActionItemId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:demo:act:wb-4"),
            title: "Book promo table & badges for ReaperCon outreach".to_string(),
            department: Some("marketing".to_string()),
            milestone: Some("ReaperCon 2027".to_string()),
            assignee_handle: Some("tessa".to_string()),
            status: ActionItemStatus::Done,
            due_at: Some(UnixMs(base_day - 4 * day_ms)),
            start_time: None,
            end_time: None,
            location: Some("Denton Convention Center".to_string()),
            cost_note: Some("$250".to_string()),
            calendar_event_id: None,
            source_event_id: None,
            source_series_id: None,
            source_thread_id: None,
            created_by: "reyth".to_string(),
            created_at: UnixMs(base_day - 10 * day_ms),
            updated_at: now,
        },
    ];
    for item in &workbook_seed {
        upsert_action_item_row(conn, item)?;
    }

    let doc = MeetingDocView {
        event_id: this_sync_id,
        series_id: Some(series),
        agenda: vec![
            AgendaItem {
                id: "ag-1".to_string(),
                text: "Review Monument Hobbies & Vallejo booth deposits".to_string(),
                checked: false,
                added_by: "elena".to_string(),
                promoted_action_id: None,
            },
            AgendaItem {
                id: "ag-2".to_string(),
                text: "Finalize volunteer shift schedule for Friday setup".to_string(),
                checked: false,
                added_by: "priya".to_string(),
                promoted_action_id: None,
            },
        ],
        notes_markdown: "## Quick Updates\n- Exhibition display cases confirmed for Ballroom A.\n- Workshop registration opens next Monday.".to_string(),
        notes_editor_handle: None,
        notes_editor_name: None,
        rev: 1,
        updated_at: now,
        action_items: Vec::new(),
        carryover_items: Vec::new(),
    };
    upsert_meeting_doc_row(conn, &doc)?;

    Ok(())
}
