//! Integration tests for Phase C: Pigment Shared Calendar, .ics invites,
//! read-only personal .ics overlays, availability slots, Meeting Docs with
//! recurring carryover, and Discord integration (SecretStore webhook isolation).

use std::sync::Arc;

use mail_core::auth::{MemorySecretStore, SecretKey, SecretStore};
use mail_core::calendar::{
    parse_ics, parse_quick_add, ActionItemStatus, AgendaItem, AvailabilityQuery,
    CalendarEventKind, CalendarRangeQuery, MeetingDocAction, PromoteActionItemInput,
    RecurrenceRule, SaveCalendarEventInput, SaveIcsOverlayInput, UpdateMeetingDocInput,
};
use mail_core::host::ManualClock;
use mail_core::ids::UnixMs;
use mail_core::{CoreConfig, Host, MailCore};

const BASE_MS: i64 = 1_764_028_800_000; // 2025-11-25 00:00:00 UTC (Tuesday)
const HOUR: i64 = 3_600_000;
const DAY: i64 = 86_400_000;

fn test_core() -> (MailCore, Arc<MemorySecretStore>) {
    let secrets = Arc::new(MemorySecretStore::default());
    let host = Host {
        secrets: secrets.clone(),
        clock: Arc::new(ManualClock::new(UnixMs(BASE_MS + 9 * HOUR))),
    };
    let core = MailCore::open(CoreConfig::default(), host).expect("open in-memory core");
    (core, secrets)
}

#[test]
fn parses_ics_vevent_with_folding_and_organizer() {
    let raw = concat!(
        "BEGIN:VCALENDAR\r\n",
        "VERSION:2.0\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:pms-2027-venue@hotel.example\r\n",
        "SUMMARY:Pigment Venue Walk\r\n",
        " through\r\n",
        "DESCRIPTION:Check Ballroom A\\, power drops\\nBring tape measure\r\n",
        "LOCATION:Ballroom A\r\n",
        "ORGANIZER;CN=\"Hotel Events\":mailto:events@hotel.example\r\n",
        "DTSTART:20251126T150000Z\r\n",
        "DTEND:20251126T163000Z\r\n",
        "END:VEVENT\r\n",
        "END:VCALENDAR",
    );

    let events = parse_ics(raw);
    assert_eq!(events.len(), 1);
    let ev = &events[0];
    assert_eq!(ev.uid, "pms-2027-venue@hotel.example");
    assert_eq!(ev.summary, "Pigment Venue Walkthrough");
    assert_eq!(ev.description, "Check Ballroom A, power drops\nBring tape measure");
    assert_eq!(ev.location.as_deref(), Some("Ballroom A"));
    assert_eq!(ev.organizer_name.as_deref(), Some("Hotel Events"));
    assert_eq!(ev.organizer_email, "events@hotel.example");
    assert!(!ev.all_day);
    assert_eq!(ev.end_at.0 - ev.start_at.0, 90 * 60_000);
}

#[test]
fn parses_ics_tzid_vtimezone_utc_and_all_day_value_date() {
    let raw = concat!(
        "BEGIN:VCALENDAR\r\n",
        "VERSION:2.0\r\n",
        "BEGIN:VTIMEZONE\r\n",
        "TZID:Custom/StudioPlus0530\r\n",
        "BEGIN:STANDARD\r\n",
        "DTSTART:19700101T000000\r\n",
        "TZOFFSETFROM:+0530\r\n",
        "TZOFFSETTO:+0530\r\n",
        "END:STANDARD\r\n",
        "END:VTIMEZONE\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:tzid-winter@pigment\r\n",
        "SUMMARY:Austin Winter Staff Sync\r\n",
        "DTSTART;TZID=America/Chicago:20251126T100000\r\n",
        "DTEND;TZID=America/Chicago:20251126T110000\r\n",
        "END:VEVENT\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:tzid-summer@pigment\r\n",
        "SUMMARY:Austin Summer Staff Sync\r\n",
        "DTSTART;TZID=\"Central Standard Time\":20250715T100000\r\n",
        "DTEND;TZID=\"Central Standard Time\":20250715T110000\r\n",
        "END:VEVENT\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:vtz-custom@pigment\r\n",
        "SUMMARY:Custom Offset Call\r\n",
        "DTSTART;TZID=Custom/StudioPlus0530:20251126T153000\r\n",
        "DTEND;TZID=Custom/StudioPlus0530:20251126T163000\r\n",
        "END:VEVENT\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:utc-z@pigment\r\n",
        "SUMMARY:UTC Kickoff\r\n",
        "DTSTART:20251126T160000Z\r\n",
        "DTEND:20251126T170000Z\r\n",
        "END:VEVENT\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:all-day@pigment\r\n",
        "SUMMARY:Pigment Show Setup Day\r\n",
        "DTSTART;VALUE=DATE:20251127\r\n",
        "DTEND;VALUE=DATE:20251128\r\n",
        "END:VEVENT\r\n",
        "END:VCALENDAR\r\n",
    );

    let events = parse_ics(raw);
    assert_eq!(events.len(), 5);

    // 2025-11-26 10:00 America/Chicago (CST, UTC-6) == 2025-11-26 16:00 UTC
    let winter = &events[0];
    let utc_z = &events[3];
    assert!(!winter.all_day);
    assert_eq!(winter.start_at, utc_z.start_at);
    assert_eq!(winter.end_at, utc_z.end_at);

    // 2025-07-15 10:00 Central Standard Time (CDT, UTC-5) == 2025-07-15 15:00 UTC
    let summer = &events[1];
    assert!(!summer.all_day);
    let expected_summer_start = chrono::DateTime::parse_from_rfc3339("2025-07-15T15:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(summer.start_at, UnixMs(expected_summer_start));

    // 2025-11-26 15:30 +0530 == 2025-11-26 10:00 UTC
    let custom = &events[2];
    assert!(!custom.all_day);
    let expected_custom_start = chrono::DateTime::parse_from_rfc3339("2025-11-26T10:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(custom.start_at, UnixMs(expected_custom_start));

    // All-day VALUE=DATE: 2025-11-27 -> 2025-11-28
    let all_day = &events[4];
    assert!(all_day.all_day);
    let expected_day_start = chrono::DateTime::parse_from_rfc3339("2025-11-27T00:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(all_day.start_at, UnixMs(expected_day_start));
    assert_eq!(all_day.end_at, UnixMs(expected_day_start + DAY));
}

#[test]
fn parses_ics_additional_iana_tzids_and_vtimezone_transition_day() {
    let raw = concat!(
        "BEGIN:VCALENDAR\r\n",
        "VERSION:2.0\r\n",
        "BEGIN:VTIMEZONE\r\n",
        "TZID:Custom/SouthernHalf\r\n",
        "BEGIN:DAYLIGHT\r\n",
        "DTSTART:19701015T020000\r\n",
        "TZOFFSETFROM:+1000\r\n",
        "TZOFFSETTO:+1100\r\n",
        "END:DAYLIGHT\r\n",
        "BEGIN:STANDARD\r\n",
        "DTSTART:19700405T030000\r\n",
        "TZOFFSETFROM:+1100\r\n",
        "TZOFFSETTO:+1000\r\n",
        "END:STANDARD\r\n",
        "END:VTIMEZONE\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:shanghai@pigment\r\n",
        "SUMMARY:Shanghai Sculptor Call\r\n",
        "DTSTART;TZID=Asia/Shanghai:20251126T180000\r\n",
        "DTEND;TZID=Asia/Shanghai:20251126T190000\r\n",
        "END:VEVENT\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:vtz-before-transition@pigment\r\n",
        "SUMMARY:Before Oct 15 Transition\r\n",
        "DTSTART;TZID=Custom/SouthernHalf:20251010T100000\r\n",
        "DTEND;TZID=Custom/SouthernHalf:20251010T110000\r\n",
        "END:VEVENT\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:vtz-after-transition@pigment\r\n",
        "SUMMARY:After Oct 15 Transition\r\n",
        "DTSTART;TZID=Custom/SouthernHalf:20251020T110000\r\n",
        "DTEND;TZID=Custom/SouthernHalf:20251020T120000\r\n",
        "END:VEVENT\r\n",
        "END:VCALENDAR\r\n",
    );

    let events = parse_ics(raw);
    assert_eq!(events.len(), 3);

    // 2025-11-26 18:00 Asia/Shanghai (UTC+8) == 2025-11-26 10:00 UTC
    let shanghai_expected = chrono::DateTime::parse_from_rfc3339("2025-11-26T10:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(events[0].start_at, UnixMs(shanghai_expected));

    // Oct 10 is before Oct 15 DST start -> +10:00 -> 00:00 UTC
    let before_expected = chrono::DateTime::parse_from_rfc3339("2025-10-10T00:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(events[1].start_at, UnixMs(before_expected));

    // Oct 20 is after Oct 15 DST start -> +11:00 -> 00:00 UTC
    let after_expected = chrono::DateTime::parse_from_rfc3339("2025-10-20T00:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(events[2].start_at, UnixMs(after_expected));
}

#[test]
fn parses_natural_language_quick_add() {
    let parsed = parse_quick_add(
        "Staff sync Thu 7pm 90m #operations @Ballroom A weekly",
        UnixMs(BASE_MS + 9 * HOUR),
    )
    .expect("quick add parses");
    assert_eq!(parsed.title, "Staff sync");
    assert_eq!(parsed.department.as_deref(), Some("operations"));
    assert_eq!(parsed.location.as_deref(), Some("Ballroom A"));
    assert_eq!(parsed.recurrence, RecurrenceRule::Weekly);
    assert_eq!(parsed.end_at.0 - parsed.start_at.0, 90 * 60_000);
}

#[tokio::test]
async fn recurring_meeting_carries_over_unfinished_action_items_and_formats_recap() {
    let (core, _) = test_core();

    let series = "series-ops-sync".to_string();
    let week1 = core
        .save_calendar_event(SaveCalendarEventInput {
            id: None,
            series_id: Some(series.clone()),
            title: "Ops Weekly".to_string(),
            description: String::new(),
            start_at: UnixMs(BASE_MS - 7 * DAY + 19 * HOUR),
            end_at: UnixMs(BASE_MS - 7 * DAY + 20 * HOUR),
            all_day: false,
            kind: CalendarEventKind::Meeting,
            department: Some("operations".to_string()),
            location: Some("Discord #ops".to_string()),
            recurrence: RecurrenceRule::None,
            discord_url: Some("discord://discord.com/channels/1/2".to_string()),
            attendees: vec!["reyth".to_string()],
            source_ics_uid: None,
        })
        .await
        .expect("save week 1");

    let week2 = core
        .save_calendar_event(SaveCalendarEventInput {
            id: None,
            series_id: Some(series.clone()),
            title: "Ops Weekly".to_string(),
            description: String::new(),
            start_at: UnixMs(BASE_MS + 19 * HOUR),
            end_at: UnixMs(BASE_MS + 20 * HOUR),
            all_day: false,
            kind: CalendarEventKind::Meeting,
            department: Some("operations".to_string()),
            location: Some("Discord #ops".to_string()),
            recurrence: RecurrenceRule::Weekly,
            discord_url: Some("discord://discord.com/channels/1/2".to_string()),
            attendees: vec!["reyth".to_string()],
            source_ics_uid: None,
        })
        .await
        .expect("save week 2");

    // Promote an action item on week 1 and leave it open.
    let item1 = core
        .promote_to_action_item(PromoteActionItemInput {
            event_id: week1.id,
            agenda_item_id: None,
            title: "Order display plinths".to_string(),
            department: Some("operations".to_string()),
            assignee_handle: Some("marcus".to_string()),
            due_at: None,
        })
        .await
        .expect("promote item");

    // Week 2's Meeting Doc automatically carries over week 1's unfinished item!
    let doc2 = core.get_meeting_doc(week2.id).await.expect("get doc2");
    assert_eq!(doc2.carryover_items.len(), 1);
    assert_eq!(doc2.carryover_items[0].id, item1.id);

    // Add agenda + notes to week 2 and verify Discord recap output.
    core.update_meeting_doc(UpdateMeetingDocInput {
        event_id: week2.id,
        action: MeetingDocAction::UpdateAgenda {
            agenda: vec![AgendaItem {
                id: "a1".to_string(),
                text: "Sign venue addendum".to_string(),
                checked: true,
                added_by: "reyth".to_string(),
                promoted_action_id: None,
            }],
        },
    })
    .await
    .expect("update agenda");

    core.update_meeting_doc(UpdateMeetingDocInput {
        event_id: week2.id,
        action: MeetingDocAction::SaveNotes {
            notes_markdown: "Confirmed hotel block rates.".to_string(),
            release_lock: true,
        },
    })
    .await
    .expect("save notes");

    let recap = core.format_meeting_recap(week2.id).await.expect("recap");
    assert!(recap.contains("**Meeting Recap: Ops Weekly**"));
    assert!(recap.contains("Confirmed hotel block rates."));
    assert!(recap.contains("Order display plinths (@marcus)"));

    // Completing item1 removes it from carryover on week 2.
    core.toggle_action_item(item1.id, ActionItemStatus::Done)
        .await
        .expect("mark done");
    let doc2_after = core.get_meeting_doc(week2.id).await.expect("get doc2 after");
    assert!(doc2_after.carryover_items.is_empty());
}

#[tokio::test]
async fn availability_respects_pigment_events_and_local_ics_overlays() {
    let (core, _) = test_core();

    // Pigment event: tomorrow 10:00–11:00 UTC
    core.save_calendar_event(SaveCalendarEventInput {
        id: None,
        series_id: None,
        title: "Sponsor Call".to_string(),
        description: String::new(),
        start_at: UnixMs(BASE_MS + DAY + 10 * HOUR),
        end_at: UnixMs(BASE_MS + DAY + 11 * HOUR),
        all_day: false,
        kind: CalendarEventKind::Meeting,
        department: Some("sponsors".to_string()),
        location: None,
        recurrence: RecurrenceRule::None,
        discord_url: None,
        attendees: Vec::new(),
        source_ics_uid: None,
    })
    .await
    .expect("save event");

    // Local personal overlay: demo feed puts a busy block tomorrow 14:00–15:00 UTC
    let overlay = core
        .save_ics_overlay(SaveIcsOverlayInput {
            id: None,
            name: "Personal".to_string(),
            url: "local://personal-demo".to_string(),
            color: "violet".to_string(),
            enabled: true,
        })
        .await
        .expect("save overlay");
    let refreshed = core
        .refresh_ics_overlay(overlay.id)
        .await
        .expect("refresh overlay");
    assert!(refreshed.event_count >= 1);

    let slots = core
        .find_availability_slots(AvailabilityQuery {
            range_start: UnixMs(BASE_MS + DAY + 9 * HOUR),
            range_end: UnixMs(BASE_MS + DAY + 17 * HOUR),
            slot_minutes: 60,
            workday_start_hour: 9,
            workday_end_hour: 17,
        })
        .await
        .expect("availability slots");

    // 10:00–11:00 (Pigment) and 14:00–15:00 (Personal overlay) must be excluded.
    assert!(!slots.iter().any(|s| s.start_at == UnixMs(BASE_MS + DAY + 10 * HOUR)));
    assert!(!slots.iter().any(|s| s.start_at == UnixMs(BASE_MS + DAY + 14 * HOUR)));
    assert!(slots.iter().any(|s| s.start_at == UnixMs(BASE_MS + DAY + 9 * HOUR)));

    // Verify list_calendar_events expands recurring events.
    let occs = core
        .list_calendar_events(CalendarRangeQuery {
            range_start: UnixMs(BASE_MS),
            range_end: UnixMs(BASE_MS + 3 * DAY),
        })
        .await
        .expect("list occurrences");
    assert_eq!(occs.len(), 1);
}

#[tokio::test]
async fn discord_webhook_lives_strictly_in_secret_store() {
    let (core, secrets) = test_core();

    let initial = core.get_discord_webhook_status().await.expect("status");
    assert!(!initial.configured);

    let bad = core
        .set_discord_webhook(Some("https://evil.example/hook".to_string()))
        .await;
    assert!(bad.is_err());

    let url = "https://discord.com/api/webhooks/12345/secret-token".to_string();
    let status = core
        .set_discord_webhook(Some(url.clone()))
        .await
        .expect("set webhook");
    assert!(status.configured);

    let stored = secrets
        .get(&SecretKey::discord_webhook_url())
        .expect("get secret")
        .expect("present");
    assert_eq!(stored.expose(), url);

    let cleared = core.set_discord_webhook(None).await.expect("clear");
    assert!(!cleared.configured);
    assert!(secrets
        .get(&SecretKey::discord_webhook_url())
        .expect("get secret")
        .is_none());
}
