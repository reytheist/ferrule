//! Local-first SQLite persistence for personal Calendar events, inline .ics invites,
//! and strictly local-only Personal .ics Overlays.

use rusqlite::{params, Connection, Row};

use crate::calendar::{
    CalendarEventKind, CalendarEventView, CalendarInviteView, IcsOverlayView, OverlayBusyBlock,
    ParsedIcsEvent, RecurrenceRule, RsvpStatus, SaveCalendarEventInput, SaveIcsOverlayInput,
};
use crate::error::CoreError;
use crate::ids::{CalendarEventId, IcsOverlayId, MessageId, UnixMs};
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

pub(crate) fn upsert_calendar_event_row(
    conn: &mut Connection,
    event: &CalendarEventView,
) -> Result<(), CoreError> {
    if event
        .discord_url
        .as_deref()
        .is_some_and(|u| u.to_ascii_lowercase().contains("/api/webhooks/"))
    {
        return Err(CoreError::InvalidInput {
            field: "discordUrl".to_string(),
            reason: "Discord webhook URLs contain secrets and cannot be stored on calendar events"
                .to_string(),
        });
    }
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
        if url.to_ascii_lowercase().contains("/api/webhooks/") {
            return Err(CoreError::InvalidInput {
                field: "discordUrl".to_string(),
                reason: "Discord webhook URLs contain secrets and cannot be stored on calendar events"
                    .to_string(),
            });
        }
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
    let id_blob = id_to_blob(id.as_bytes());
    let rows = conn
        .execute("DELETE FROM calendar_event WHERE id = ?1", params![id_blob])
        .sq()?;
    Ok(rows > 0)
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
        || url.starts_with("webcal://")
        || url.starts_with("local://");
    if !valid_url {
        return Err(CoreError::InvalidInput {
            field: "url".to_string(),
            reason: "feed URL must start with https://, webcal://, or local://".to_string(),
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
