//! Personal Calendar, Email ↔ Calendar Bridges (.ics invites,
//! read-only personal .ics overlays, and availability slots).

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::ids::{CalendarEventId, IcsOverlayId, MessageId, UnixMs};

// ── Calendar Events & Recurrence ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalendarEventKind {
    #[default]
    Meeting,
    Milestone,
    Deadline,
    RoomSchedule,
    ShowEvent,
}

impl CalendarEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Meeting => "meeting",
            Self::Milestone => "milestone",
            Self::Deadline => "deadline",
            Self::RoomSchedule => "room_schedule",
            Self::ShowEvent => "show_event",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "milestone" => Self::Milestone,
            "deadline" => Self::Deadline,
            "room_schedule" => Self::RoomSchedule,
            "show_event" => Self::ShowEvent,
            _ => Self::Meeting,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecurrenceRule {
    #[default]
    None,
    Weekly,
    Biweekly,
    Monthly,
}

impl RecurrenceRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Weekly => "weekly",
            Self::Biweekly => "biweekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "weekly" => Self::Weekly,
            "biweekly" => Self::Biweekly,
            "monthly" => Self::Monthly,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventView {
    pub id: CalendarEventId,
    pub series_id: Option<String>,
    pub title: String,
    pub description: String,
    pub start_at: UnixMs,
    pub end_at: UnixMs,
    pub all_day: bool,
    pub kind: CalendarEventKind,
    pub department: Option<String>,
    pub location: Option<String>,
    pub recurrence: RecurrenceRule,
    pub discord_url: Option<String>,
    pub attendees: Vec<String>,
    pub source_ics_uid: Option<String>,
    pub created_by: String,
    pub updated_at: UnixMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCalendarEventInput {
    pub id: Option<CalendarEventId>,
    #[serde(default)]
    pub series_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub start_at: UnixMs,
    pub end_at: UnixMs,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub kind: CalendarEventKind,
    #[serde(default)]
    pub department: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub recurrence: RecurrenceRule,
    #[serde(default)]
    pub discord_url: Option<String>,
    #[serde(default)]
    pub attendees: Vec<String>,
    #[serde(default)]
    pub source_ics_uid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarOccurrence {
    pub event: CalendarEventView,
    pub occurrence_start: UnixMs,
    pub occurrence_end: UnixMs,
    pub is_recurring_instance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarRangeQuery {
    pub range_start: UnixMs,
    pub range_end: UnixMs,
}

// ── Inline .ics Invites ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RsvpStatus {
    #[default]
    NeedsAction,
    Accepted,
    Tentative,
    Declined,
}

impl RsvpStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedsAction => "needs_action",
            Self::Accepted => "accepted",
            Self::Tentative => "tentative",
            Self::Declined => "declined",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "accepted" => Self::Accepted,
            "tentative" => Self::Tentative,
            "declined" => Self::Declined,
            _ => Self::NeedsAction,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarInviteView {
    pub message_id: MessageId,
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub location: Option<String>,
    pub organizer_name: Option<String>,
    pub organizer_email: String,
    pub start_at: UnixMs,
    pub end_at: UnixMs,
    pub all_day: bool,
    pub rsvp_status: RsvpStatus,
    pub added_event_id: Option<CalendarEventId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RsvpInviteInput {
    pub message_id: MessageId,
    pub uid: String,
    pub status: RsvpStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddInviteToCalendarInput {
    pub message_id: MessageId,
    pub uid: String,
    #[serde(default)]
    pub department: Option<String>,
}

// ── Local-Only Personal .ics Overlays ───────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IcsOverlayView {
    pub id: IcsOverlayId,
    pub name: String,
    pub url: String,
    pub color: String,
    pub enabled: bool,
    pub last_fetched_at: Option<UnixMs>,
    pub last_error: Option<String>,
    pub event_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveIcsOverlayInput {
    pub id: Option<IcsOverlayId>,
    pub name: String,
    pub url: String,
    pub color: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayBusyBlock {
    pub overlay_id: IcsOverlayId,
    pub overlay_name: String,
    pub overlay_color: String,
    pub uid: String,
    pub summary: String,
    pub start_at: UnixMs,
    pub end_at: UnixMs,
    pub all_day: bool,
}

// ── Availability Picker ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilityQuery {
    pub range_start: UnixMs,
    pub range_end: UnixMs,
    pub slot_minutes: u32,
    pub workday_start_hour: u8,
    pub workday_end_hour: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilitySlot {
    pub start_at: UnixMs,
    pub end_at: UnixMs,
    pub label: String,
}

// ── Pure RFC 5545 .ics Parser ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedIcsEvent {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub location: Option<String>,
    pub organizer_name: Option<String>,
    pub organizer_email: String,
    pub start_at: UnixMs,
    pub end_at: UnixMs,
    pub all_day: bool,
}

#[derive(Debug, Default, Clone)]
struct VTimezoneDef {
    standard_offset_secs: Option<i32>,
    daylight_offset_secs: Option<i32>,
    standard_start_md: Option<(u32, u32)>,
    daylight_start_md: Option<(u32, u32)>,
}

/// Unfold RFC 5545 continuation lines (CRLF or LF followed by space/tab) and parse VEVENT blocks,
/// honoring embedded `VTIMEZONE` definitions and `TZID=...` parameters.
pub fn parse_ics(raw: &str) -> Vec<ParsedIcsEvent> {
    let mut unfolded: Vec<String> = Vec::new();
    for line in raw.replace("\r\n", "\n").split('\n') {
        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            if let Some(last) = unfolded.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        unfolded.push(line.to_string());
    }

    let vtimezones = parse_vtimezones(&unfolded);

    let mut events = Vec::new();
    let mut in_event = false;
    let mut uid = String::new();
    let mut summary = String::new();
    let mut description = String::new();
    let mut location: Option<String> = None;
    let mut organizer_name: Option<String> = None;
    let mut organizer_email = String::new();
    let mut start_at: Option<(UnixMs, bool)> = None;
    let mut end_at: Option<UnixMs> = None;

    for line in unfolded {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("BEGIN:VEVENT") {
            in_event = true;
            uid.clear();
            summary.clear();
            description.clear();
            location = None;
            organizer_name = None;
            organizer_email.clear();
            start_at = None;
            end_at = None;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END:VEVENT") {
            if in_event {
                if let Some((start, all_day)) = start_at {
                    let default_end = if all_day {
                        UnixMs(start.0 + 86_400_000)
                    } else {
                        UnixMs(start.0 + 3_600_000)
                    };
                    let end = end_at.filter(|e| e.0 > start.0).unwrap_or(default_end);
                    let final_uid = if uid.is_empty() {
                        format!("ics-{}-{}", start.0, summary.len())
                    } else {
                        uid.clone()
                    };
                    events.push(ParsedIcsEvent {
                        uid: final_uid,
                        summary: if summary.is_empty() {
                            "(Untitled event)".to_string()
                        } else {
                            summary.clone()
                        },
                        description: description.clone(),
                        location: location.clone(),
                        organizer_name: organizer_name.clone(),
                        organizer_email: organizer_email.clone(),
                        start_at: start,
                        end_at: end,
                        all_day,
                    });
                }
            }
            in_event = false;
            continue;
        }
        if !in_event {
            continue;
        }
        let Some((prop_part, value_raw)) = trimmed.split_once(':') else {
            continue;
        };
        let mut parts = prop_part.split(';');
        let prop_name = parts.next().unwrap_or("").to_ascii_uppercase();
        let params: Vec<&str> = parts.collect();
        let value = unescape_ics_text(value_raw);

        match prop_name.as_str() {
            "UID" => uid = value,
            "SUMMARY" => summary = value,
            "DESCRIPTION" => description = value,
            "LOCATION" => {
                if !value.trim().is_empty() {
                    location = Some(value);
                }
            }
            "ORGANIZER" => {
                organizer_email = value_raw
                    .trim()
                    .strip_prefix("mailto:")
                    .or_else(|| value_raw.trim().strip_prefix("MAILTO:"))
                    .unwrap_or(value_raw.trim())
                    .to_string();
                for param in &params {
                    if let Some((k, v)) = param.split_once('=') {
                        if k.eq_ignore_ascii_case("CN") {
                            let cleaned = v.trim_matches('"').trim();
                            if !cleaned.is_empty() {
                                organizer_name = Some(cleaned.to_string());
                            }
                        }
                    }
                }
            }
            "DTSTART" => {
                let is_date = params
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case("VALUE=DATE"));
                let tzid = extract_tzid_param(&params);
                if let Some((ts, all_day)) =
                    parse_ics_datetime(value_raw.trim(), is_date, tzid, &vtimezones)
                {
                    start_at = Some((ts, all_day));
                }
            }
            "DTEND" => {
                let is_date = params
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case("VALUE=DATE"));
                let tzid = extract_tzid_param(&params);
                if let Some((ts, _)) =
                    parse_ics_datetime(value_raw.trim(), is_date, tzid, &vtimezones)
                {
                    end_at = Some(ts);
                }
            }
            _ => {}
        }
    }

    events
}

fn parse_vtimezones(lines: &[String]) -> HashMap<String, VTimezoneDef> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum SubSection {
        None,
        Standard,
        Daylight,
    }

    let mut map = HashMap::new();
    let mut in_vtz = false;
    let mut section = SubSection::None;
    let mut current_tzid = String::new();
    let mut current = VTimezoneDef::default();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("BEGIN:VTIMEZONE") {
            in_vtz = true;
            section = SubSection::None;
            current_tzid.clear();
            current = VTimezoneDef::default();
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END:VTIMEZONE") {
            if in_vtz && !current_tzid.is_empty() {
                map.insert(current_tzid.to_ascii_lowercase(), current.clone());
            }
            in_vtz = false;
            section = SubSection::None;
            continue;
        }
        if !in_vtz {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("BEGIN:STANDARD") {
            section = SubSection::Standard;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END:STANDARD") {
            section = SubSection::None;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("BEGIN:DAYLIGHT") {
            section = SubSection::Daylight;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END:DAYLIGHT") {
            section = SubSection::None;
            continue;
        }

        let Some((prop_part, value_raw)) = trimmed.split_once(':') else {
            continue;
        };
        let prop_name = prop_part.split(';').next().unwrap_or("").to_ascii_uppercase();
        let val = value_raw.trim().trim_matches('"');

        match prop_name.as_str() {
            "TZID" if section == SubSection::None => {
                current_tzid = val.to_string();
            }
            "TZOFFSETTO" => {
                if let Some(secs) = parse_utc_offset_secs(val) {
                    match section {
                        SubSection::Standard => current.standard_offset_secs = Some(secs),
                        SubSection::Daylight => current.daylight_offset_secs = Some(secs),
                        SubSection::None => {}
                    }
                }
            }
            "DTSTART" => {
                if let Ok(dt) = NaiveDateTime::parse_from_str(val.trim_end_matches('Z'), "%Y%m%dT%H%M%S") {
                    match section {
                        SubSection::Standard => current.standard_start_md = Some((dt.month(), dt.day())),
                        SubSection::Daylight => current.daylight_start_md = Some((dt.month(), dt.day())),
                        SubSection::None => {}
                    }
                }
            }
            _ => {}
        }
    }

    map
}

fn extract_tzid_param<'a>(params: &[&'a str]) -> Option<&'a str> {
    for param in params {
        if let Some((k, v)) = param.split_once('=') {
            if k.eq_ignore_ascii_case("TZID") {
                let cleaned = v.trim().trim_matches('"').trim();
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
        }
    }
    None
}

fn unescape_ics_text(raw: &str) -> String {
    raw.replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn parse_ics_datetime(
    raw: &str,
    force_date: bool,
    tzid: Option<&str>,
    vtimezones: &HashMap<String, VTimezoneDef>,
) -> Option<(UnixMs, bool)> {
    let trimmed = raw.trim();
    if force_date || (trimmed.len() == 8 && trimmed.bytes().all(|b| b.is_ascii_digit())) {
        let date = NaiveDate::parse_from_str(trimmed, "%Y%m%d").ok()?;
        let dt = date.and_hms_opt(0, 0, 0)?;
        return Some((UnixMs(Utc.from_utc_datetime(&dt).timestamp_millis()), true));
    }
    if let Some(utc_raw) = trimmed.strip_suffix('Z') {
        let dt = NaiveDateTime::parse_from_str(utc_raw, "%Y%m%dT%H%M%S").ok()?;
        return Some((UnixMs(Utc.from_utc_datetime(&dt).timestamp_millis()), false));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y%m%dT%H%M%S") {
        let offset_secs = tzid
            .and_then(|tz| resolve_tzid_offset_secs(tz, &dt, vtimezones))
            .unwrap_or(0);
        let utc_dt = dt - Duration::seconds(i64::from(offset_secs));
        return Some((UnixMs(Utc.from_utc_datetime(&utc_dt).timestamp_millis()), false));
    }
    None
}

/// Parse an RFC 5545 / ISO UTC offset string like `-0600`, `+0530`, `-05:00`, or `UTC+02:00`.
fn parse_utc_offset_secs(raw: &str) -> Option<i32> {
    let cleaned = raw
        .trim()
        .strip_prefix("UTC")
        .or_else(|| raw.trim().strip_prefix("GMT"))
        .unwrap_or(raw.trim())
        .trim();
    if cleaned.is_empty() {
        return None;
    }
    let (sign, rest) = if let Some(r) = cleaned.strip_prefix('+') {
        (1i32, r)
    } else {
        let r = cleaned.strip_prefix('-')?;
        (-1i32, r)
    };
    let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    let (hours, mins) = match digits.len() {
        2 => (digits.parse::<i32>().ok()?, 0),
        4 => (
            digits[0..2].parse::<i32>().ok()?,
            digits[2..4].parse::<i32>().ok()?,
        ),
        6 => (
            digits[0..2].parse::<i32>().ok()?,
            digits[2..4].parse::<i32>().ok()?,
        ),
        _ => return None,
    };
    if hours > 14 || mins >= 60 {
        return None;
    }
    Some(sign * (hours * 3600 + mins * 60))
}

fn nth_weekday_of_month(year: i32, month: u32, weekday: Weekday, n: u32) -> Option<u32> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let offset = (weekday.num_days_from_monday() as i32
        - first.weekday().num_days_from_monday() as i32)
        .rem_euclid(7) as u32;
    Some(1 + offset + (n.saturating_sub(1)) * 7)
}

fn last_weekday_of_month(year: i32, month: u32, weekday: Weekday) -> Option<u32> {
    let next_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    let last = next_month - Duration::days(1);
    let back = (last.weekday().num_days_from_monday() as i32
        - weekday.num_days_from_monday() as i32)
        .rem_euclid(7) as u32;
    Some(last.day().saturating_sub(back))
}

fn is_us_dst(dt: &NaiveDateTime) -> bool {
    let m = dt.month();
    if !(3..=11).contains(&m) {
        return false;
    }
    if m > 3 && m < 11 {
        return true;
    }
    if m == 3 {
        let start_day = nth_weekday_of_month(dt.year(), 3, Weekday::Sun, 2).unwrap_or(14);
        return dt.day() > start_day || (dt.day() == start_day && dt.hour() >= 2);
    }
    let end_day = nth_weekday_of_month(dt.year(), 11, Weekday::Sun, 1).unwrap_or(7);
    dt.day() < end_day || (dt.day() == end_day && dt.hour() < 2)
}

fn is_eu_dst(dt: &NaiveDateTime) -> bool {
    let m = dt.month();
    if !(3..=10).contains(&m) {
        return false;
    }
    if m > 3 && m < 10 {
        return true;
    }
    if m == 3 {
        let start_day = last_weekday_of_month(dt.year(), 3, Weekday::Sun).unwrap_or(28);
        return dt.day() > start_day || (dt.day() == start_day && dt.hour() >= 1);
    }
    let end_day = last_weekday_of_month(dt.year(), 10, Weekday::Sun).unwrap_or(28);
    dt.day() < end_day || (dt.day() == end_day && dt.hour() < 1)
}

/// AEDT (Sydney/Melbourne): first Sunday in October 02:00 to first Sunday in April 03:00.
fn is_au_dst(dt: &NaiveDateTime) -> bool {
    let m = dt.month();
    if (5..=9).contains(&m) {
        return false;
    }
    if m >= 11 || m <= 3 {
        return true;
    }
    if m == 10 {
        let start_day = nth_weekday_of_month(dt.year(), 10, Weekday::Sun, 1).unwrap_or(7);
        return dt.day() > start_day || (dt.day() == start_day && dt.hour() >= 2);
    }
    // April
    let end_day = nth_weekday_of_month(dt.year(), 4, Weekday::Sun, 1).unwrap_or(7);
    dt.day() < end_day || (dt.day() == end_day && dt.hour() < 3)
}

/// NZDT (Auckland): last Sunday in September 02:00 to first Sunday in April 03:00.
fn is_nz_dst(dt: &NaiveDateTime) -> bool {
    let m = dt.month();
    if (5..=8).contains(&m) {
        return false;
    }
    if m >= 10 || m <= 3 {
        return true;
    }
    if m == 9 {
        let start_day = last_weekday_of_month(dt.year(), 9, Weekday::Sun).unwrap_or(28);
        return dt.day() > start_day || (dt.day() == start_day && dt.hour() >= 2);
    }
    // April
    let end_day = nth_weekday_of_month(dt.year(), 4, Weekday::Sun, 1).unwrap_or(7);
    dt.day() < end_day || (dt.day() == end_day && dt.hour() < 3)
}

fn resolve_tzid_offset_secs(
    tzid: &str,
    dt: &NaiveDateTime,
    vtimezones: &HashMap<String, VTimezoneDef>,
) -> Option<i32> {
    let key = tzid.trim().trim_matches('"').to_ascii_lowercase();
    if matches!(key.as_str(), "utc" | "gmt" | "etc/utc" | "etc/gmt" | "z") {
        return Some(0);
    }

    if let Some(vtz) = vtimezones.get(&key) {
        match (vtz.standard_offset_secs, vtz.daylight_offset_secs) {
            (Some(std_s), Some(dst_s)) => {
                let dst_md = vtz.daylight_start_md.unwrap_or((3, 8));
                let std_md = vtz.standard_start_md.unwrap_or((11, 1));
                let cur_md = (dt.month(), dt.day());
                let in_dst = if dst_md.0 == 3 && std_md.0 == 11 {
                    is_us_dst(dt)
                } else if dst_md.0 == 3 && std_md.0 == 10 {
                    is_eu_dst(dt)
                } else if dst_md < std_md {
                    cur_md >= dst_md && cur_md < std_md
                } else {
                    cur_md >= dst_md || cur_md < std_md
                };
                return Some(if in_dst { dst_s } else { std_s });
            }
            (Some(std_s), None) => return Some(std_s),
            (None, Some(dst_s)) => return Some(dst_s),
            (None, None) => {}
        }
    }

    if let Some(secs) = parse_utc_offset_secs(tzid) {
        return Some(secs);
    }

    // Well-known IANA and Windows TZID fallback table.
    match key.as_str() {
        "america/new_york"
        | "america/toronto"
        | "us/eastern"
        | "eastern standard time"
        | "est5edt" => Some(if is_us_dst(dt) { -4 * 3600 } else { -5 * 3600 }),
        "america/chicago"
        | "america/winnipeg"
        | "us/central"
        | "central standard time"
        | "cst6cdt" => Some(if is_us_dst(dt) { -5 * 3600 } else { -6 * 3600 }),
        "america/denver"
        | "america/edmonton"
        | "us/mountain"
        | "mountain standard time"
        | "mst7mdt" => Some(if is_us_dst(dt) { -6 * 3600 } else { -7 * 3600 }),
        "america/los_angeles"
        | "america/vancouver"
        | "us/pacific"
        | "pacific standard time"
        | "pst8pdt" => Some(if is_us_dst(dt) { -7 * 3600 } else { -8 * 3600 }),
        "america/phoenix" | "us mountain standard time" => Some(-7 * 3600),
        "america/anchorage" | "alaskan standard time" => {
            Some(if is_us_dst(dt) { -8 * 3600 } else { -9 * 3600 })
        }
        "pacific/honolulu" | "hawaiian standard time" => Some(-10 * 3600),
        "america/sao_paulo" | "e. south america standard time" => Some(-3 * 3600),
        "europe/london" | "gmt standard time" => {
            Some(if is_eu_dst(dt) { 3600 } else { 0 })
        }
        "europe/paris"
        | "europe/berlin"
        | "europe/amsterdam"
        | "europe/madrid"
        | "europe/rome"
        | "w. europe standard time"
        | "central european standard time"
        | "romance standard time" => Some(if is_eu_dst(dt) { 2 * 3600 } else { 3600 }),
        "asia/dubai" | "arabian standard time" => Some(4 * 3600),
        "asia/kolkata" | "india standard time" => Some(5 * 3600 + 30 * 60),
        "asia/shanghai"
        | "asia/hong_kong"
        | "asia/singapore"
        | "asia/taipei"
        | "china standard time"
        | "singapore standard time"
        | "taipei standard time" => Some(8 * 3600),
        "asia/tokyo" | "asia/seoul" | "tokyo standard time" | "korea standard time" => {
            Some(9 * 3600)
        }
        "australia/sydney" | "australia/melbourne" | "aus eastern standard time" => {
            Some(if is_au_dst(dt) { 11 * 3600 } else { 10 * 3600 })
        }
        "pacific/auckland" | "new zealand standard time" => {
            Some(if is_nz_dst(dt) { 13 * 3600 } else { 12 * 3600 })
        }
        _ => None,
    }
}

// ── Recurrence Expansion ────────────────────────────────────────────────────

pub fn expand_occurrences(
    events: &[CalendarEventView],
    range_start: UnixMs,
    range_end: UnixMs,
) -> Vec<CalendarOccurrence> {
    let mut out = Vec::new();
    for event in events {
        let duration_ms = (event.end_at.0 - event.start_at.0).max(900_000);
        if event.recurrence == RecurrenceRule::None {
            if event.end_at.0 >= range_start.0 && event.start_at.0 <= range_end.0 {
                out.push(CalendarOccurrence {
                    event: event.clone(),
                    occurrence_start: event.start_at,
                    occurrence_end: event.end_at,
                    is_recurring_instance: false,
                });
            }
            continue;
        }

        let Some(mut cursor) = DateTime::from_timestamp_millis(event.start_at.0) else {
            continue;
        };
        let mut idx = 0usize;
        // Cap expansion to 120 occurrences to stay fast even on wide ranges.
        while cursor.timestamp_millis() <= range_end.0 && idx < 120 {
            let occ_start = UnixMs(cursor.timestamp_millis());
            let occ_end = UnixMs(occ_start.0 + duration_ms);
            if occ_end.0 >= range_start.0 && occ_start.0 <= range_end.0 {
                out.push(CalendarOccurrence {
                    event: event.clone(),
                    occurrence_start: occ_start,
                    occurrence_end: occ_end,
                    is_recurring_instance: idx > 0,
                });
            }
            cursor = match event.recurrence {
                RecurrenceRule::None => break,
                RecurrenceRule::Weekly => cursor + Duration::days(7),
                RecurrenceRule::Biweekly => cursor + Duration::days(14),
                RecurrenceRule::Monthly => advance_one_month(cursor),
            };
            idx += 1;
        }
    }
    out.sort_by_key(|o| o.occurrence_start.0);
    out
}

fn advance_one_month(dt: DateTime<Utc>) -> DateTime<Utc> {
    let (year, month) = if dt.month() == 12 {
        (dt.year() + 1, 1)
    } else {
        (dt.year(), dt.month() + 1)
    };
    let mut day = dt.day();
    while day > 28 {
        if let Some(d) = NaiveDate::from_ymd_opt(year, month, day) {
            let naive = d.and_hms_opt(dt.hour(), dt.minute(), dt.second()).unwrap();
            return Utc.from_utc_datetime(&naive);
        }
        day -= 1;
    }
    let d = NaiveDate::from_ymd_opt(year, month, day).expect("valid day <= 28");
    let naive = d.and_hms_opt(dt.hour(), dt.minute(), dt.second()).unwrap();
    Utc.from_utc_datetime(&naive)
}

// ── Natural-Language Quick-Add Parser ───────────────────────────────────────

/// Parse a natural-language quick-add string like:
/// `"Staff sync Thu 7pm #operations @Ballroom A weekly"`
pub fn parse_quick_add(raw: &str, now: UnixMs) -> Result<SaveCalendarEventInput, CoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "text".to_string(),
            reason: "Enter an event like \"Staff sync Thu 7pm #operations\"".to_string(),
        });
    }

    let base_dt = DateTime::from_timestamp_millis(now.0)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).expect("epoch"));
    let mut target_date = base_dt.date_naive();
    let mut hour: u32 = 10;
    let mut minute: u32 = 0;
    let mut duration_mins: i64 = 60;
    let mut department: Option<String> = None;
    let mut location_tokens: Vec<String> = Vec::new();
    let mut collecting_location = false;
    let mut recurrence = RecurrenceRule::None;
    let mut kind = CalendarEventKind::Meeting;
    let mut title_tokens: Vec<String> = Vec::new();

    for token in trimmed.split_whitespace() {
        let lower = token.to_ascii_lowercase();

        if let Some(tag) = token.strip_prefix('#') {
            collecting_location = false;
            let cleaned = tag
                .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                .to_ascii_lowercase();
            if !cleaned.is_empty() {
                department = Some(cleaned);
            }
            continue;
        }

        if let Some(loc) = token.strip_prefix('@') {
            collecting_location = true;
            if !loc.is_empty() {
                location_tokens.push(loc.to_string());
            }
            continue;
        }

        if lower == "weekly" {
            collecting_location = false;
            recurrence = RecurrenceRule::Weekly;
            continue;
        }
        if lower == "biweekly" {
            collecting_location = false;
            recurrence = RecurrenceRule::Biweekly;
            continue;
        }
        if lower == "monthly" {
            collecting_location = false;
            recurrence = RecurrenceRule::Monthly;
            continue;
        }

        if lower == "today" {
            collecting_location = false;
            target_date = base_dt.date_naive();
            continue;
        }
        if lower == "tomorrow" || lower == "tmrw" {
            collecting_location = false;
            target_date = base_dt.date_naive() + Duration::days(1);
            continue;
        }
        if let Some(wd) = parse_weekday(&lower) {
            collecting_location = false;
            target_date = next_weekday(base_dt.date_naive(), wd);
            continue;
        }
        if let Ok(parsed_date) = NaiveDate::parse_from_str(token, "%Y-%m-%d") {
            collecting_location = false;
            target_date = parsed_date;
            continue;
        }
        if let Some((h, m)) = parse_time_token(&lower) {
            collecting_location = false;
            hour = h;
            minute = m;
            continue;
        }
        if let Some(mins) = parse_duration_token(&lower) {
            collecting_location = false;
            duration_mins = mins;
            continue;
        }

        if lower.contains("deadline") {
            kind = CalendarEventKind::Deadline;
        } else if lower.contains("milestone") {
            kind = CalendarEventKind::Milestone;
        }

        if collecting_location {
            location_tokens.push(token.to_string());
        } else {
            title_tokens.push(token.to_string());
        }
    }

    let title = if title_tokens.is_empty() {
        "Staff Event".to_string()
    } else {
        title_tokens.join(" ")
    };
    let location = if location_tokens.is_empty() {
        None
    } else {
        Some(location_tokens.join(" "))
    };
    if location.is_some() && kind == CalendarEventKind::Meeting && title.to_ascii_lowercase().contains("room") {
        kind = CalendarEventKind::RoomSchedule;
    }

    let start_naive = target_date
        .and_hms_opt(hour, minute, 0)
        .unwrap_or_else(|| target_date.and_hms_opt(10, 0, 0).expect("10:00"));
    let start_at = UnixMs(Utc.from_utc_datetime(&start_naive).timestamp_millis());
    let end_at = UnixMs(start_at.0 + duration_mins * 60_000);

    Ok(SaveCalendarEventInput {
        id: None,
        series_id: None,
        title,
        description: String::new(),
        start_at,
        end_at,
        all_day: false,
        kind,
        department,
        location,
        recurrence,
        discord_url: None,
        attendees: Vec::new(),
        source_ics_uid: None,
    })
}

fn parse_weekday(token: &str) -> Option<Weekday> {
    match token.trim_end_matches(',') {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn next_weekday(from: NaiveDate, target: Weekday) -> NaiveDate {
    let from_num = from.weekday().num_days_from_monday() as i64;
    let target_num = target.num_days_from_monday() as i64;
    let mut delta = (target_num - from_num).rem_euclid(7);
    if delta == 0 {
        delta = 7;
    }
    from + Duration::days(delta)
}

fn parse_time_token(token: &str) -> Option<(u32, u32)> {
    if token == "noon" {
        return Some((12, 0));
    }
    let (body, is_pm, has_meridiem) = if let Some(b) = token.strip_suffix("pm") {
        (b, true, true)
    } else if let Some(b) = token.strip_suffix("am") {
        (b, false, true)
    } else if token.contains(':') {
        (token, false, false)
    } else {
        return None;
    };

    let (h_str, m_str) = match body.split_once(':') {
        Some((h, m)) => (h, m),
        None => (body, "0"),
    };
    let mut h: u32 = h_str.parse().ok()?;
    let m: u32 = m_str.parse().ok()?;
    if m >= 60 {
        return None;
    }
    if has_meridiem {
        if h == 0 || h > 12 {
            return None;
        }
        if is_pm && h != 12 {
            h += 12;
        } else if !is_pm && h == 12 {
            h = 0;
        }
    } else if h >= 24 {
        return None;
    }
    Some((h, m))
}

fn parse_duration_token(token: &str) -> Option<i64> {
    if let Some(h) = token.strip_suffix('h') {
        let hours: i64 = h.parse().ok()?;
        if (1..=12).contains(&hours) {
            return Some(hours * 60);
        }
    }
    if let Some(m) = token.strip_suffix('m') {
        let mins: i64 = m.parse().ok()?;
        if (15..=480).contains(&mins) {
            return Some(mins);
        }
    }
    None
}

// ── Availability Slot Computation ───────────────────────────────────────────

pub fn compute_availability_slots(
    query: &AvailabilityQuery,
    busy_intervals: &[(UnixMs, UnixMs)],
) -> Vec<AvailabilitySlot> {
    let slot_ms = i64::from(query.slot_minutes.clamp(15, 240)) * 60_000;
    let start_hour = u32::from(query.workday_start_hour.min(22));
    let end_hour = u32::from(query.workday_end_hour.clamp(query.workday_start_hour + 1, 23));

    let Some(start_dt) = DateTime::from_timestamp_millis(query.range_start.0) else {
        return Vec::new();
    };
    let Some(end_dt) = DateTime::from_timestamp_millis(query.range_end.0) else {
        return Vec::new();
    };

    let mut merged: Vec<(i64, i64)> = busy_intervals
        .iter()
        .filter(|(s, e)| e.0 > s.0)
        .map(|(s, e)| (s.0, e.0))
        .collect();
    merged.sort_by_key(|(s, _)| *s);

    let mut slots = Vec::new();
    let mut day = start_dt.date_naive();
    let last_day = end_dt.date_naive();
    let mut day_count = 0;

    while day <= last_day && day_count < 14 && slots.len() < 24 {
        let day_start = Utc
            .from_utc_datetime(&day.and_hms_opt(start_hour, 0, 0).expect("valid start hour"))
            .timestamp_millis();
        let day_end = Utc
            .from_utc_datetime(&day.and_hms_opt(end_hour, 0, 0).expect("valid end hour"))
            .timestamp_millis();

        let mut cursor = day_start.max(query.range_start.0);
        while cursor + slot_ms <= day_end && slots.len() < 24 {
            let candidate_end = cursor + slot_ms;
            let conflict = merged
                .iter()
                .find(|(bs, be)| *bs < candidate_end && *be > cursor);
            if let Some((_, busy_end)) = conflict {
                cursor = (*busy_end).max(cursor + 900_000);
                // Align to 15-minute boundary
                let rem = cursor % 900_000;
                if rem != 0 {
                    cursor += 900_000 - rem;
                }
            } else {
                let s_dt = DateTime::from_timestamp_millis(cursor).expect("valid ts");
                let e_dt = DateTime::from_timestamp_millis(candidate_end).expect("valid ts");
                let label = format!(
                    "{} · {} – {} UTC",
                    s_dt.format("%a, %b %-d"),
                    s_dt.format("%-I:%M %p"),
                    e_dt.format("%-I:%M %p")
                );
                slots.push(AvailabilitySlot {
                    start_at: UnixMs(cursor),
                    end_at: UnixMs(candidate_end),
                    label,
                });
                cursor = candidate_end;
            }
        }
        day += Duration::days(1);
        day_count += 1;
    }

    slots
}
