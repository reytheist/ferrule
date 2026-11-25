//! Integration tests for Phase D: Action Items CRUD, Calendar Sync, Email/Meeting
//! Source Links, and Operations Workbook CSV import/export + deduplication.

use std::sync::Arc;

use base64::Engine;
use mail_core::auth::MemorySecretStore;
use mail_core::calendar::{
    ActionItemStatus, CalendarRangeQuery, SaveActionItemInput,
};
use mail_core::host::ManualClock;
use mail_core::ids::UnixMs;
use mail_core::workbook::{
    format_action_items_csv, parse_csv, ImportWorkbookInput, WorkbookImportTarget,
};
use mail_core::{CoreConfig, Host, MailCore};

const BASE_MS: i64 = 1_764_028_800_000; // 2025-11-25 00:00:00 UTC
const HOUR: i64 = 3_600_000;
const DAY: i64 = 86_400_000;

fn test_core() -> MailCore {
    let secrets = Arc::new(MemorySecretStore::default());
    let host = Host {
        secrets,
        clock: Arc::new(ManualClock::new(UnixMs(BASE_MS + 9 * HOUR))),
    };
    MailCore::open(CoreConfig::default(), host).expect("open in-memory core")
}

#[test]
fn parses_rfc4180_csv_with_quotes_commas_and_bom() {
    let raw = "\u{feff}Dept.,What,Title / Task,Where,$$$\r\n\"operations\",\"PMS 2027\",\"Check \"\"Ballroom A\"\", power drops\",\"Ballroom A\",\"$450\"\r\n";
    let rows = parse_csv(raw);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], "Dept.");
    assert_eq!(rows[1][2], "Check \"Ballroom A\", power drops");
    assert_eq!(rows[1][4], "$450");
}

#[tokio::test]
async fn action_item_crud_and_optional_calendar_sync() {
    let core = test_core();

    let created = core
        .save_action_item(SaveActionItemInput {
            id: None,
            title: "Set up Ballroom A lighting rigs".to_string(),
            department: Some("#Exhibition".to_string()),
            milestone: Some("PMS 2027".to_string()),
            assignee_handle: Some("@Devon".to_string()),
            status: ActionItemStatus::Open,
            due_at: Some(UnixMs(BASE_MS + 2 * DAY)),
            start_time: Some("09:00".to_string()),
            end_time: Some("11:30".to_string()),
            location: Some("Ballroom A".to_string()),
            cost_note: Some("$320".to_string()),
            source_event_id: None,
            source_series_id: None,
            source_thread_id: None,
            sync_to_calendar: true,
        })
        .await
        .expect("create action item");

    assert_eq!(created.department.as_deref(), Some("exhibition"));
    assert_eq!(created.assignee_handle.as_deref(), Some("devon"));
    assert_eq!(created.start_time.as_deref(), Some("09:00"));
    assert_eq!(created.end_time.as_deref(), Some("11:30"));
    assert!(created.calendar_event_id.is_some());

    // Synced event should appear in the Pigment Shared Calendar range.
    let occs = core
        .list_calendar_events(CalendarRangeQuery {
            range_start: UnixMs(BASE_MS),
            range_end: UnixMs(BASE_MS + 5 * DAY),
        })
        .await
        .expect("list calendar events");
    assert_eq!(occs.len(), 1);
    assert_eq!(occs[0].event.title, "Set up Ballroom A lighting rigs");
    assert_eq!(occs[0].event.location.as_deref(), Some("Ballroom A"));

    // Toggle done and verify CSV export includes it.
    let done = core
        .toggle_action_item(created.id, ActionItemStatus::Done)
        .await
        .expect("toggle done");
    assert_eq!(done.status, ActionItemStatus::Done);

    let csv = core.export_action_items_csv().await.expect("export csv");
    assert!(csv.contains("Set up Ballroom A lighting rigs"));
    assert!(csv.contains("@devon"));
    assert!(csv.contains("TRUE"));

    // Delete removes it cleanly.
    core.delete_action_item(created.id)
        .await
        .expect("delete action item");
    assert!(core.list_action_items().await.expect("list").is_empty());
}

#[tokio::test]
async fn workbook_csv_import_dry_run_dedup_and_contacts_roundtrip() {
    let core = test_core();

    let tasks_csv = concat!(
        "Dept.,What,Title / Task,Assignee,Done,Date,Start Time,End Time,Where,$$$\r\n",
        "Sponsors,PMS 2027,Send Gold Sponsor invoices,@maya,FALSE,2027-03-15,09:00,10:00,Office,$1200\r\n",
        "Shopping List,PMS 2027,Buy lanyards and badge clips,,FALSE,2027-04-01,,,Registration,$180\r\n",
        "Marketing,ReaperCon 2027,Print flyer stack,tessa,TRUE,2027-02-10,,,Expo Hall,$95\r\n",
    );
    let b64 = base64::engine::general_purpose::STANDARD.encode(tasks_csv.as_bytes());

    // Dry run previews 3 creates without mutating SQLite.
    let preview = core
        .import_operations_workbook(ImportWorkbookInput {
            filename: "2027 Operations Workbook.csv".to_string(),
            data_base64: b64.clone(),
            target: WorkbookImportTarget::Auto,
            sync_to_calendar: true,
            dry_run: true,
        })
        .await
        .expect("dry run");
    assert!(preview.dry_run);
    assert_eq!(preview.action_items_created, 3);
    assert_eq!(preview.calendar_events_created, 3);
    assert!(core.list_action_items().await.expect("list").is_empty());

    // Actual import creates 3 items + 3 calendar events.
    let imported = core
        .import_operations_workbook(ImportWorkbookInput {
            filename: "2027 Operations Workbook.csv".to_string(),
            data_base64: b64.clone(),
            target: WorkbookImportTarget::Auto,
            sync_to_calendar: true,
            dry_run: false,
        })
        .await
        .expect("commit import");
    assert_eq!(imported.action_items_created, 3);
    assert_eq!(imported.action_items_updated, 0);
    assert_eq!(imported.calendar_events_created, 3);

    // Re-importing the same file updates in place instead of duplicating!
    let reimported = core
        .import_operations_workbook(ImportWorkbookInput {
            filename: "2027 Operations Workbook.csv".to_string(),
            data_base64: b64,
            target: WorkbookImportTarget::Auto,
            sync_to_calendar: true,
            dry_run: false,
        })
        .await
        .expect("re-import");
    assert_eq!(reimported.action_items_created, 0);
    assert_eq!(reimported.action_items_updated, 3);
    assert_eq!(core.list_action_items().await.expect("list").len(), 3);

    // Contacts CSV import + dedup by email + CSV export roundtrip.
    let contacts_csv = concat!(
        "Name,Category,Organization,Tier,Emails,Notes\r\n",
        "Dana Okafor,Sponsor,Ferngate Miniatures,Gold,dana@ferngate.co,Corner booth\r\n",
        "Marco Bellini,Instructor,Bellini Studio,Workshop Lead,marco@bellini.example,Needs projector\r\n",
        "Dana Okafor,Sponsor,Ferngate Miniatures,Platinum,dana@ferngate.co; ops@ferngate.co,Upgraded\r\n",
    );
    let contacts_b64 = base64::engine::general_purpose::STANDARD.encode(contacts_csv.as_bytes());
    let contact_report = core
        .import_operations_workbook(ImportWorkbookInput {
            filename: "Sponsors_Contacts.csv".to_string(),
            data_base64: contacts_b64,
            target: WorkbookImportTarget::Contacts,
            sync_to_calendar: false,
            dry_run: false,
        })
        .await
        .expect("import contacts");
    assert_eq!(contact_report.contacts_created, 2);
    assert_eq!(contact_report.contacts_updated, 1);

    let contacts = core.list_show_contacts().await.expect("list contacts");
    assert_eq!(contacts.len(), 2);
    let dana = contacts.iter().find(|c| c.name == "Dana Okafor").expect("dana");
    assert_eq!(dana.tier.as_deref(), Some("Platinum"));
    assert_eq!(dana.emails.len(), 2);

    let exported_contacts = core
        .export_show_contacts_csv()
        .await
        .expect("export contacts csv");
    assert!(exported_contacts.contains("Dana Okafor"));
    assert!(exported_contacts.contains("ops@ferngate.co"));

    // Verify format_action_items_csv roundtrip parses back cleanly.
    let items = core.list_action_items().await.expect("list items");
    let exported_items = format_action_items_csv(&items);
    let parsed_back = parse_csv(&exported_items);
    assert_eq!(parsed_back.len(), 4); // header + 3 rows
}

#[tokio::test]
async fn csv_export_neutralizes_formula_injection_and_tsv_import_works() {
    let core = test_core();

    // Formula-leading values must be neutralized with a leading apostrophe on CSV export.
    core.save_action_item(SaveActionItemInput {
        id: None,
        title: "=HYPERLINK(\"http://evil.test\",\"Click\")".to_string(),
        department: Some("-Ops".to_string()),
        milestone: Some("+cmd|' /C calc'!A0".to_string()),
        assignee_handle: Some("@jordan".to_string()),
        status: ActionItemStatus::Open,
        due_at: None,
        start_time: None,
        end_time: None,
        location: None,
        cost_note: None,
        source_event_id: None,
        source_series_id: None,
        source_thread_id: None,
        sync_to_calendar: false,
    })
    .await
    .expect("save formula item");

    let items = core.list_action_items().await.expect("list items");
    let csv = format_action_items_csv(&items);
    let parsed = parse_csv(&csv);
    assert_eq!(parsed.len(), 2);
    assert!(parsed[1][0].starts_with("'-ops"), "got {}", parsed[1][0]);
    assert!(parsed[1][2].starts_with("'="), "got {}", parsed[1][2]);

    // Tab-separated (.tsv) workbooks must split on tabs, not commas.
    let tsv = "Dept.\tWhat\tTitle / Task\tAssignee\tDone\nVenue\tLoad-in\tConfirm dock pass\tdevon\t\n";
    let tsv_b64 = base64::engine::general_purpose::STANDARD.encode(tsv.as_bytes());
    let report = core
        .import_operations_workbook(ImportWorkbookInput {
            filename: "schedule.tsv".to_string(),
            data_base64: tsv_b64,
            target: WorkbookImportTarget::ActionItems,
            sync_to_calendar: false,
            dry_run: false,
        })
        .await
        .expect("import tsv");
    assert_eq!(report.action_items_created, 1);

    // Oversized base64 payloads are rejected before decoding.
    let huge = "A".repeat(30 * 1024 * 1024);
    let err = core
        .import_operations_workbook(ImportWorkbookInput {
            filename: "huge.xlsx".to_string(),
            data_base64: huge,
            target: WorkbookImportTarget::Auto,
            sync_to_calendar: false,
            dry_run: true,
        })
        .await;
    assert!(err.is_err());
}
