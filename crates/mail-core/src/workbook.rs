//! Phase D — Operations Workbook `.xlsx` / CSV importer and RFC 4180 CSV exporter.
//!
//! Imports:
//! - `2027 Operations Workbook` rows (`Dept.`, `What` / Milestone, `Title / Task`,
//!   `Assignee`, `Date`, `Start/End Time`, `Where`, `$$$`, `Done`) -> `action_item`
//!   (and optionally Pigment Shared Calendar milestone / room-schedule events).
//! - Contact sheets (`Sponsors / Vendors`, `Instructors`, `VIPs`, `Volunteers`, `Venue`)
//!   -> `show_contact`, deduping cleanly by email or normalized name.
//!
//! Dense spatial/matrix sheets (`Floor Plan`, `Vendor MapPayment`, `Room Supplies`,
//! `Class Schedule`, fulfillment matrices) are explicitly skipped so they remain in
//! Google Drive via Pinned Staff Links.

use std::collections::HashMap;
use std::io::Cursor;

use base64::Engine;
use calamine::{open_workbook_auto_from_rs, Data, Reader};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::calendar::{ActionItemStatus, ActionItemView, CalendarEventView};
use crate::calendar_core::{calendar_input_from_action, parse_hhmm};
use crate::error::CoreError;
use crate::events::CoreEvent;
use crate::ids::{ActionItemId, ContactId, UnixMs};
use crate::staff::{normalize_handle, ContactCategory, ShowContactView};
use crate::store;
use crate::MailCore;

pub const PIGMENT_DEPARTMENTS: &[&str] = &[
    "marketing",
    "sponsors",
    "exhibition",
    "volunteers",
    "operations",
    "workshops",
    "shopping-list",
    "code-monkeys",
    "general-business",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkbookImportTarget {
    #[default]
    Auto,
    ActionItems,
    Contacts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWorkbookInput {
    pub filename: String,
    /// Base64-encoded bytes of the `.xlsx`, `.xls`, `.ods`, or UTF-8 `.csv` file.
    pub data_base64: String,
    #[serde(default)]
    pub target: WorkbookImportTarget,
    #[serde(default)]
    pub sync_to_calendar: bool,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreviewRowKind {
    ActionItem,
    Contact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreviewRowStatus {
    Create,
    Update,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookPreviewRow {
    pub kind: PreviewRowKind,
    pub sheet: String,
    pub title: String,
    pub detail: String,
    pub status: PreviewRowStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookImportReport {
    pub dry_run: bool,
    pub action_items_created: u32,
    pub action_items_updated: u32,
    pub action_items_skipped: u32,
    pub calendar_events_created: u32,
    pub contacts_created: u32,
    pub contacts_updated: u32,
    pub contacts_skipped: u32,
    pub skipped_sheets: Vec<String>,
    pub warnings: Vec<String>,
    pub preview_rows: Vec<WorkbookPreviewRow>,
}

// ── Pure RFC 4180 CSV Parser & Formatter ────────────────────────────────────

/// Parse an RFC 4180 CSV document into rows of cell strings (strips UTF-8 BOM).
pub fn parse_csv(raw: &str) -> Vec<Vec<String>> {
    parse_delimited(raw, ',')
}

/// Parse an RFC 4180 delimited text document (e.g. `,` for CSV or `\t` for TSV) into rows.
pub fn parse_delimited(raw: &str, delimiter: char) -> Vec<Vec<String>> {
    let input = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cell.push(ch);
            }
        } else {
            match ch {
                '"' if cell.is_empty() => {
                    in_quotes = true;
                }
                c if c == delimiter => {
                    current_row.push(cell);
                    cell = String::new();
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    current_row.push(cell);
                    cell = String::new();
                    if current_row.iter().any(|c| !c.trim().is_empty()) {
                        rows.push(current_row);
                    }
                    current_row = Vec::new();
                }
                '\n' => {
                    current_row.push(cell);
                    cell = String::new();
                    if current_row.iter().any(|c| !c.trim().is_empty()) {
                        rows.push(current_row);
                    }
                    current_row = Vec::new();
                }
                _ => cell.push(ch),
            }
        }
    }

    if !cell.is_empty() || !current_row.is_empty() {
        current_row.push(cell);
        if current_row.iter().any(|c| !c.trim().is_empty()) {
            rows.push(current_row);
        }
    }
    rows
}

fn csv_escape(field: &str) -> String {
    // Neutralize spreadsheet formula injection (OWASP CSV injection) on leading
    // formula-triggering characters before applying RFC 4180 quoting.
    let neutralized = match field.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{field}"),
        _ => field.to_string(),
    };
    if neutralized.contains(',')
        || neutralized.contains('"')
        || neutralized.contains('\n')
        || neutralized.contains('\r')
    {
        format!("\"{}\"", neutralized.replace('"', "\"\""))
    } else {
        neutralized
    }
}

fn csv_row(fields: &[String]) -> String {
    fields
        .iter()
        .map(|f| csv_escape(f))
        .collect::<Vec<_>>()
        .join(",")
}

/// Format Action Items as an RFC 4180 CSV matching the `2027 Operations Workbook` schema.
pub fn format_action_items_csv(items: &[ActionItemView]) -> String {
    let mut lines = Vec::with_capacity(items.len() + 1);
    lines.push(
        "Dept.,What,Title / Task,Assignee,Done,Date,Start Time,End Time,Where,$$$,Source"
            .to_string(),
    );
    for item in items {
        let due = item
            .due_at
            .and_then(|ts| DateTime::from_timestamp_millis(ts.0))
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let source = if item.source_thread_id.is_some() {
            "email".to_string()
        } else if item.source_event_id.is_some() {
            "meeting".to_string()
        } else {
            String::new()
        };
        let fields = [
            item.department.clone().unwrap_or_default(),
            item.milestone.clone().unwrap_or_default(),
            item.title.clone(),
            item.assignee_handle
                .as_ref()
                .map(|h| format!("@{h}"))
                .unwrap_or_default(),
            if item.status == ActionItemStatus::Done {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            },
            due,
            item.start_time.clone().unwrap_or_default(),
            item.end_time.clone().unwrap_or_default(),
            item.location.clone().unwrap_or_default(),
            item.cost_note.clone().unwrap_or_default(),
            source,
        ];
        lines.push(csv_row(&fields));
    }
    lines.join("\r\n") + "\r\n"
}

/// Format Shared Show Contacts as an RFC 4180 CSV for spreadsheet backup.
pub fn format_show_contacts_csv(contacts: &[ShowContactView]) -> String {
    let mut lines = Vec::with_capacity(contacts.len() + 1);
    lines.push("Name,Category,Organization,Tier,Emails,Notes".to_string());
    for c in contacts {
        let fields = [
            c.name.clone(),
            c.category.as_str().to_string(),
            c.organization.clone().unwrap_or_default(),
            c.tier.clone().unwrap_or_default(),
            c.emails.join("; "),
            c.notes.clone(),
        ];
        lines.push(csv_row(&fields));
    }
    lines.join("\r\n") + "\r\n"
}

// ── Workbook Parsing & Normalization ────────────────────────────────────────

struct RawSheet {
    name: String,
    rows: Vec<Vec<String>>,
}

fn is_spatial_matrix_sheet(sheet_name: &str) -> bool {
    let lower = sheet_name.trim().to_ascii_lowercase();
    lower.contains("floor plan")
        || lower.contains("floorplan")
        || lower.contains("mappayment")
        || lower.contains("map payment")
        || lower.contains("room supplies")
        || lower.contains("class schedule")
        || lower.contains("fulfillment")
}

fn contact_category_from_sheet(sheet_name: &str) -> Option<ContactCategory> {
    let lower = sheet_name.trim().to_ascii_lowercase();
    if lower.contains("sponsor") {
        Some(ContactCategory::Sponsor)
    } else if lower.contains("vendor") {
        Some(ContactCategory::Vendor)
    } else if lower.contains("instructor") || lower.contains("teacher") {
        Some(ContactCategory::Instructor)
    } else if lower.contains("vip") || lower.contains("judge") || lower.contains("guest") {
        Some(ContactCategory::Vip)
    } else if lower.contains("volunteer") {
        Some(ContactCategory::Volunteer)
    } else if lower.contains("venue") || lower.contains("hotel") {
        Some(ContactCategory::Venue)
    } else if lower.contains("contact") {
        Some(ContactCategory::Other)
    } else {
        None
    }
}

fn parse_contact_category(raw: &str, fallback: ContactCategory) -> ContactCategory {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return fallback;
    }
    if lower.contains("sponsor") {
        ContactCategory::Sponsor
    } else if lower.contains("vendor") || lower.contains("exhibitor") {
        ContactCategory::Vendor
    } else if lower.contains("instructor") || lower.contains("teacher") {
        ContactCategory::Instructor
    } else if lower.contains("vip") || lower.contains("judge") {
        ContactCategory::Vip
    } else if lower.contains("volunteer") {
        ContactCategory::Volunteer
    } else if lower.contains("venue") || lower.contains("hotel") {
        ContactCategory::Venue
    } else {
        fallback
    }
}

pub fn normalize_department(raw: &str) -> Option<String> {
    let cleaned = raw
        .trim()
        .trim_start_matches('#')
        .to_ascii_lowercase()
        .replace(['_', ' '], "-");
    if cleaned.is_empty() {
        return None;
    }
    let mapped = match cleaned.as_str() {
        "marketing" | "mktg" | "promo" | "social" => "marketing",
        "sponsor" | "sponsors" | "sponsorship" | "sponsorships" => "sponsors",
        "exhibition" | "exhibitors" | "display" | "competition" | "judging" => "exhibition",
        "volunteer" | "volunteers" | "crew" => "volunteers",
        "ops" | "operation" | "operations" | "logistics" | "venue" => "operations",
        "workshop" | "workshops" | "classes" | "education" => "workshops",
        "shopping" | "shopping-list" | "supplies" | "purchasing" => "shopping-list",
        "code" | "code-monkeys" | "tech" | "web" | "it" => "code-monkeys",
        "general" | "general-business" | "admin" | "finance" | "legal" => "general-business",
        _ => return Some(cleaned),
    };
    Some(mapped.to_string())
}

fn parse_bool_done(raw: &str) -> ActionItemStatus {
    let lower = raw.trim().to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "true" | "yes" | "y" | "1" | "x" | "done" | "complete" | "completed" | "✓" | "✔"
    ) {
        ActionItemStatus::Done
    } else {
        ActionItemStatus::Open
    }
}

/// Parse an ISO date (`2027-04-12`), US date (`04/12/2027`, `4/12/27`),
/// or Excel serial day number (`46489`) into UTC midnight `UnixMs`.
fn parse_date_cell(raw: &str) -> Option<UnixMs> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Excel serial date (days since 1899-12-30)
    if let Ok(serial) = trimmed.parse::<f64>() {
        if (20_000.0..=80_000.0).contains(&serial) {
            let epoch = NaiveDate::from_ymd_opt(1899, 12, 30)?;
            let date = epoch + Duration::days(serial.trunc() as i64);
            let dt = date.and_hms_opt(0, 0, 0)?;
            return Some(UnixMs(Utc.from_utc_datetime(&dt).timestamp_millis()));
        }
    }
    for fmt in ["%Y-%m-%d", "%m/%d/%Y", "%m/%d/%y", "%Y/%m/%d", "%b %d, %Y", "%b %d %Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, fmt) {
            let dt = date.and_hms_opt(0, 0, 0)?;
            return Some(UnixMs(Utc.from_utc_datetime(&dt).timestamp_millis()));
        }
    }
    None
}

/// Normalize a time cell (`09:00`, `2:30 PM`, or Excel fractional day `0.375`) to `HH:MM`.
fn parse_time_cell(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(frac) = trimmed.parse::<f64>() {
        if (0.0..1.0).contains(&frac) {
            let total_mins = (frac * 1440.0).round() as u32;
            let h = (total_mins / 60).min(23);
            let m = total_mins % 60;
            return Some(format!("{h:02}:{m:02}"));
        }
    }
    parse_hhmm(trimmed).map(|(h, m)| format!("{h:02}:{m:02}"))
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            if (f.fract()).abs() < 1e-9 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Data::DateTime(dt) => {
            if let Some(naive) = dt.as_datetime() {
                if naive.time() == chrono::NaiveTime::from_hms_opt(0, 0, 0).expect("midnight") {
                    naive.format("%Y-%m-%d").to_string()
                } else {
                    naive.format("%H:%M").to_string()
                }
            } else {
                format!("{}", dt.as_f64())
            }
        }
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_string(),
        Data::Error(_) => String::new(),
    }
}

fn extract_sheets(filename: &str, bytes: &[u8]) -> Result<Vec<RawSheet>, CoreError> {
    let lower = filename.trim().to_ascii_lowercase();
    let is_tsv = lower.ends_with(".tsv");
    let is_csv = lower.ends_with(".csv") || is_tsv || lower.ends_with(".txt");

    if is_csv {
        let text = std::str::from_utf8(bytes).map_err(|_| CoreError::InvalidInput {
            field: "dataBase64".to_string(),
            reason: "CSV file must be valid UTF-8 text".to_string(),
        })?;
        let base_name = filename.rsplit(['/', '\\']).next().unwrap_or("CSV");
        let sheet_name = base_name
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(base_name)
            .to_string();
        let delimiter = if is_tsv { '\t' } else { ',' };
        return Ok(vec![RawSheet {
            name: sheet_name,
            rows: parse_delimited(text, delimiter),
        }]);
    }

    let cursor = Cursor::new(bytes);
    let mut wb = open_workbook_auto_from_rs(cursor).map_err(|e| CoreError::InvalidInput {
        field: "dataBase64".to_string(),
        reason: format!("Could not read spreadsheet workbook ({e})"),
    })?;

    let mut sheets = Vec::new();
    for name in wb.sheet_names() {
        if let Ok(range) = wb.worksheet_range(&name) {
            let rows: Vec<Vec<String>> = range
                .rows()
                .map(|r| r.iter().map(cell_to_string).collect::<Vec<_>>())
                .filter(|r| r.iter().any(|c| !c.trim().is_empty()))
                .collect();
            sheets.push(RawSheet { name, rows });
        }
    }
    Ok(sheets)
}

fn normalize_header(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '$')
        .collect()
}

fn col_idx(headers: &[String], candidates: &[&str]) -> Option<usize> {
    for candidate in candidates {
        if let Some(pos) = headers.iter().position(|h| h == candidate) {
            return Some(pos);
        }
    }
    for candidate in candidates {
        if let Some(pos) = headers.iter().position(|h| h.contains(candidate)) {
            return Some(pos);
        }
    }
    None
}

fn cell_at(row: &[String], idx: Option<usize>) -> String {
    idx.and_then(|i| row.get(i))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn action_dedup_key(title: &str, milestone: Option<&str>, dept: Option<&str>) -> String {
    format!(
        "{}|{}|{}",
        title.trim().to_ascii_lowercase(),
        milestone.unwrap_or("").trim().to_ascii_lowercase(),
        dept.unwrap_or("").trim().to_ascii_lowercase()
    )
}

fn classify_sheet(sheet: &RawSheet, target: WorkbookImportTarget) -> Option<PreviewRowKind> {
    match target {
        WorkbookImportTarget::ActionItems => Some(PreviewRowKind::ActionItem),
        WorkbookImportTarget::Contacts => Some(PreviewRowKind::Contact),
        WorkbookImportTarget::Auto => {
            if is_spatial_matrix_sheet(&sheet.name) {
                return None;
            }
            let headers: Vec<String> = sheet
                .rows
                .first()
                .map(|r| r.iter().map(|c| normalize_header(c)).collect())
                .unwrap_or_default();
            let has_email_col = col_idx(&headers, &["email", "emails", "emailaddress"]).is_some();
            let has_task_col = col_idx(
                &headers,
                &["titletask", "task", "actionitem", "title", "item", "dept", "department"],
            )
            .is_some();

            if contact_category_from_sheet(&sheet.name).is_some() && has_email_col {
                return Some(PreviewRowKind::Contact);
            }
            if has_email_col && !has_task_col {
                return Some(PreviewRowKind::Contact);
            }
            if has_task_col {
                return Some(PreviewRowKind::ActionItem);
            }
            if has_email_col {
                return Some(PreviewRowKind::Contact);
            }
            None
        }
    }
}

// ── MailCore Facade Methods ─────────────────────────────────────────────────

impl MailCore {
    pub async fn import_operations_workbook(
        &self,
        input: ImportWorkbookInput,
    ) -> Result<WorkbookImportReport, CoreError> {
        const MAX_WORKBOOK_BYTES: usize = 20 * 1024 * 1024;
        const MAX_BASE64_CHARS: usize = (MAX_WORKBOOK_BYTES / 3 + 1) * 4 + 64;
        let trimmed_b64 = input.data_base64.trim();
        if trimmed_b64.len() > MAX_BASE64_CHARS {
            return Err(CoreError::InvalidInput {
                field: "dataBase64".to_string(),
                reason: "workbook exceeds 20 MB size limit".to_string(),
            });
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(trimmed_b64)
            .map_err(|_| CoreError::InvalidInput {
                field: "dataBase64".to_string(),
                reason: "invalid base64 payload".to_string(),
            })?;
        if bytes.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "dataBase64".to_string(),
                reason: "uploaded file is empty".to_string(),
            });
        }
        if bytes.len() > MAX_WORKBOOK_BYTES {
            return Err(CoreError::InvalidInput {
                field: "dataBase64".to_string(),
                reason: "workbook exceeds 20 MB size limit".to_string(),
            });
        }

        let sheets = extract_sheets(&input.filename, &bytes)?;
        if sheets.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "filename".to_string(),
                reason: "no readable sheets or rows found in file".to_string(),
            });
        }

        let ws = self.get_staff_workspace().await?;
        let actor = if ws.connected && !ws.staff_handle.is_empty() {
            ws.staff_handle.clone()
        } else {
            "staff".to_string()
        };
        let now = self.now();

        let (report, synced_items, synced_events, synced_contacts) =
            self.inner.store.with_conn(|conn| {
                let mut report = WorkbookImportReport {
                    dry_run: input.dry_run,
                    ..WorkbookImportReport::default()
                };
                let mut synced_items: Vec<ActionItemView> = Vec::new();
                let mut synced_events: Vec<CalendarEventView> = Vec::new();
                let mut synced_contacts: Vec<ShowContactView> = Vec::new();

                let existing_items = store::list_action_items(conn)?;
                let mut item_by_key: HashMap<String, ActionItemView> = existing_items
                    .into_iter()
                    .map(|it| {
                        (
                            action_dedup_key(
                                &it.title,
                                it.milestone.as_deref(),
                                it.department.as_deref(),
                            ),
                            it,
                        )
                    })
                    .collect();

                let existing_contacts = store::list_show_contacts(conn)?;
                let mut contact_by_email: HashMap<String, ShowContactView> = HashMap::new();
                let mut contact_by_name: HashMap<String, ShowContactView> = HashMap::new();
                for c in existing_contacts {
                    for email in &c.emails {
                        contact_by_email.insert(email.to_ascii_lowercase(), c.clone());
                    }
                    contact_by_name.insert(c.name.trim().to_ascii_lowercase(), c);
                }

                for sheet in &sheets {
                    if input.target == WorkbookImportTarget::Auto
                        && is_spatial_matrix_sheet(&sheet.name)
                    {
                        report.skipped_sheets.push(sheet.name.clone());
                        continue;
                    }
                    let Some(kind) = classify_sheet(sheet, input.target) else {
                        report.skipped_sheets.push(sheet.name.clone());
                        continue;
                    };
                    if sheet.rows.len() < 2 {
                        continue;
                    }

                    let headers: Vec<String> = sheet.rows[0]
                        .iter()
                        .map(|h| normalize_header(h))
                        .collect();

                    match kind {
                        PreviewRowKind::ActionItem => {
                            let c_title = col_idx(
                                &headers,
                                &["titletask", "task", "title", "actionitem", "item", "description"],
                            );
                            let c_dept = col_idx(&headers, &["dept", "department", "team"]);
                            let c_what = col_idx(&headers, &["what", "event", "milestone", "show"]);
                            let c_who = col_idx(&headers, &["assignee", "owner", "who", "assigned"]);
                            let c_done = col_idx(&headers, &["done", "status", "complete", "completed"]);
                            let c_date = col_idx(&headers, &["date", "duedate", "due", "when"]);
                            let c_start = col_idx(&headers, &["starttime", "start"]);
                            let c_end = col_idx(&headers, &["endtime", "end"]);
                            let c_where = col_idx(&headers, &["where", "location", "room"]);
                            let c_cost = col_idx(&headers, &["$$$", "$$", "$", "cost", "budget"]);

                            if c_title.is_none() {
                                report.warnings.push(format!(
                                    "Sheet \"{}\" has no Title/Task column — skipped.",
                                    sheet.name
                                ));
                                report.skipped_sheets.push(sheet.name.clone());
                                continue;
                            }

                            for row in sheet.rows.iter().skip(1) {
                                let title = cell_at(row, c_title);
                                if title.is_empty() {
                                    report.action_items_skipped += 1;
                                    continue;
                                }
                                let dept = normalize_department(&cell_at(row, c_dept));
                                let milestone = {
                                    let m = cell_at(row, c_what);
                                    if m.is_empty() { None } else { Some(m) }
                                };
                                let assignee = {
                                    let a = normalize_handle(&cell_at(row, c_who));
                                    if a.is_empty() { None } else { Some(a) }
                                };
                                let status = parse_bool_done(&cell_at(row, c_done));
                                let due_at = parse_date_cell(&cell_at(row, c_date));
                                let start_time = parse_time_cell(&cell_at(row, c_start));
                                let end_time = parse_time_cell(&cell_at(row, c_end));
                                let location = {
                                    let l = cell_at(row, c_where);
                                    if l.is_empty() { None } else { Some(l) }
                                };
                                let cost_note = {
                                    let c = cell_at(row, c_cost);
                                    if c.is_empty() { None } else { Some(c) }
                                };

                                let key = action_dedup_key(
                                    &title,
                                    milestone.as_deref(),
                                    dept.as_deref(),
                                );
                                let existing = item_by_key.get(&key).cloned();
                                let row_status = if existing.is_some() {
                                    report.action_items_updated += 1;
                                    PreviewRowStatus::Update
                                } else {
                                    report.action_items_created += 1;
                                    PreviewRowStatus::Create
                                };

                                let mut item = ActionItemView {
                                    id: existing
                                        .as_ref()
                                        .map(|e| e.id)
                                        .unwrap_or_else(ActionItemId::fresh),
                                    title: title.clone(),
                                    department: dept.clone(),
                                    milestone: milestone.clone(),
                                    assignee_handle: assignee.clone(),
                                    status,
                                    due_at,
                                    start_time,
                                    end_time,
                                    location: location.clone(),
                                    cost_note: cost_note.clone(),
                                    calendar_event_id: existing
                                        .as_ref()
                                        .and_then(|e| e.calendar_event_id),
                                    source_event_id: existing
                                        .as_ref()
                                        .and_then(|e| e.source_event_id),
                                    source_series_id: existing
                                        .as_ref()
                                        .and_then(|e| e.source_series_id.clone()),
                                    source_thread_id: existing
                                        .as_ref()
                                        .and_then(|e| e.source_thread_id),
                                    created_by: existing
                                        .as_ref()
                                        .map(|e| e.created_by.clone())
                                        .unwrap_or_else(|| actor.clone()),
                                    created_at: existing
                                        .as_ref()
                                        .map(|e| e.created_at)
                                        .unwrap_or(now),
                                    updated_at: now,
                                };

                                if input.sync_to_calendar {
                                    if let Some(due) = due_at {
                                        if item.calendar_event_id.is_none() {
                                            report.calendar_events_created += 1;
                                        }
                                        if !input.dry_run {
                                            let cal_input = calendar_input_from_action(&item, due);
                                            let ev = store::save_calendar_event(
                                                conn, &cal_input, &actor, now,
                                            )?;
                                            item.calendar_event_id = Some(ev.id);
                                            synced_events.push(ev);
                                        }
                                    }
                                }

                                if !input.dry_run {
                                    store::upsert_action_item_row(conn, &item)?;
                                    synced_items.push(item.clone());
                                }
                                item_by_key.insert(key, item);

                                if report.preview_rows.len() < 50 {
                                    let mut parts = Vec::new();
                                    if let Some(d) = &dept {
                                        parts.push(format!("#{d}"));
                                    }
                                    if let Some(m) = &milestone {
                                        parts.push(m.clone());
                                    }
                                    if let Some(a) = &assignee {
                                        parts.push(format!("@{a}"));
                                    }
                                    if let Some(l) = &location {
                                        parts.push(l.clone());
                                    }
                                    report.preview_rows.push(WorkbookPreviewRow {
                                        kind: PreviewRowKind::ActionItem,
                                        sheet: sheet.name.clone(),
                                        title,
                                        detail: parts.join(" · "),
                                        status: row_status,
                                    });
                                }
                            }
                        }

                        PreviewRowKind::Contact => {
                            let c_name = col_idx(&headers, &["name", "contact", "fullname", "person"]);
                            let c_email = col_idx(&headers, &["emails", "email", "emailaddress"]);
                            let c_cat = col_idx(&headers, &["category", "type", "role"]);
                            let c_org = col_idx(
                                &headers,
                                &["organization", "company", "studio", "business", "org"],
                            );
                            let c_tier = col_idx(&headers, &["tier", "level", "booth", "package"]);
                            let c_notes = col_idx(&headers, &["notes", "note", "comments"]);

                            let fallback_cat = contact_category_from_sheet(&sheet.name)
                                .unwrap_or(ContactCategory::Other);

                            for row in sheet.rows.iter().skip(1) {
                                let raw_name = cell_at(row, c_name);
                                let raw_emails = cell_at(row, c_email);
                                let mut emails: Vec<String> = raw_emails
                                    .split([',', ';', ' '])
                                    .map(|e| e.trim().to_ascii_lowercase())
                                    .filter(|e| e.contains('@') && e.len() >= 3)
                                    .collect();
                                emails.sort();
                                emails.dedup();

                                let org = {
                                    let o = cell_at(row, c_org);
                                    if o.is_empty() { None } else { Some(o) }
                                };
                                let name = if !raw_name.is_empty() {
                                    raw_name
                                } else if let Some(o) = &org {
                                    o.clone()
                                } else {
                                    String::new()
                                };

                                if name.is_empty() || emails.is_empty() {
                                    report.contacts_skipped += 1;
                                    continue;
                                }

                                let category =
                                    parse_contact_category(&cell_at(row, c_cat), fallback_cat);
                                let tier = {
                                    let t = cell_at(row, c_tier);
                                    if t.is_empty() { None } else { Some(t) }
                                };
                                let notes = cell_at(row, c_notes);

                                let existing = emails
                                    .iter()
                                    .find_map(|e| contact_by_email.get(e).cloned())
                                    .or_else(|| {
                                        contact_by_name
                                            .get(&name.trim().to_ascii_lowercase())
                                            .cloned()
                                    });

                                let row_status = if existing.is_some() {
                                    report.contacts_updated += 1;
                                    PreviewRowStatus::Update
                                } else {
                                    report.contacts_created += 1;
                                    PreviewRowStatus::Create
                                };

                                let mut merged_emails = existing
                                    .as_ref()
                                    .map(|e| e.emails.clone())
                                    .unwrap_or_default();
                                merged_emails.extend(emails.iter().cloned());
                                merged_emails.sort();
                                merged_emails.dedup();

                                let contact = ShowContactView {
                                    id: existing
                                        .as_ref()
                                        .map(|e| e.id)
                                        .unwrap_or_else(ContactId::fresh),
                                    name: name.clone(),
                                    emails: merged_emails.clone(),
                                    category,
                                    organization: org.or_else(|| {
                                        existing.as_ref().and_then(|e| e.organization.clone())
                                    }),
                                    tier: tier
                                        .or_else(|| existing.as_ref().and_then(|e| e.tier.clone())),
                                    notes: if notes.is_empty() {
                                        existing
                                            .as_ref()
                                            .map(|e| e.notes.clone())
                                            .unwrap_or_default()
                                    } else {
                                        notes
                                    },
                                    updated_at: now,
                                };

                                if !input.dry_run {
                                    store::upsert_show_contact_row(conn, &contact)?;
                                    synced_contacts.push(contact.clone());
                                }
                                for email in &merged_emails {
                                    contact_by_email.insert(email.clone(), contact.clone());
                                }
                                contact_by_name
                                    .insert(contact.name.trim().to_ascii_lowercase(), contact);

                                if report.preview_rows.len() < 50 {
                                    report.preview_rows.push(WorkbookPreviewRow {
                                        kind: PreviewRowKind::Contact,
                                        sheet: sheet.name.clone(),
                                        title: name,
                                        detail: format!(
                                            "{} · {}",
                                            category.as_str(),
                                            emails.join(", ")
                                        ),
                                        status: row_status,
                                    });
                                }
                            }
                        }
                    }
                }

                Ok((report, synced_items, synced_events, synced_contacts))
            })?;

        if !input.dry_run {
            if ws.connected {
                if let Some(client) = self.remote_staff_client(&ws)? {
                    for item in &synced_items {
                        let _ = client.push_action_item(item).await;
                    }
                    for ev in &synced_events {
                        let _ = client.push_calendar_event(ev).await;
                    }
                    for contact in &synced_contacts {
                        let _ = client.push_contact(contact).await;
                    }
                }
            }
            if !synced_items.is_empty() || !synced_events.is_empty() {
                self.emit(CoreEvent::CalendarChanged);
            }
            if !synced_contacts.is_empty() {
                self.emit(CoreEvent::StaffChanged);
            }
        }

        Ok(report)
    }

    pub async fn export_action_items_csv(&self) -> Result<String, CoreError> {
        let items = self.inner.store.with_conn(store::list_action_items)?;
        Ok(format_action_items_csv(&items))
    }

    pub async fn export_show_contacts_csv(&self) -> Result<String, CoreError> {
        let contacts = self.inner.store.with_conn(store::list_show_contacts)?;
        Ok(format_show_contacts_csv(&contacts))
    }
}
