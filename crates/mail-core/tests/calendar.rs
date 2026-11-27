//! Integration tests for personal Calendar, .ics invites,
//! read-only personal .ics overlays, and availability slots.

use std::sync::Arc;

use mail_core::auth::MemorySecretStore;
use mail_core::calendar::{
    parse_ics, parse_quick_add, AvailabilityQuery, CalendarEventKind, CalendarRangeQuery,
    RecurrenceRule, SaveCalendarEventInput, SaveIcsOverlayInput,
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
fn parses_ics_au_nz_dst_transition_boundaries() {
    // 2025 AU (Sydney): AEDT (+11) ends first Sun of Apr = Apr 6, starts first Sun of Oct = Oct 5.
    // 2025 NZ (Auckland): NZDT (+13) ends first Sun of Apr = Apr 6, starts last Sun of Sep = Sep 28.
    let raw = concat!(
        "BEGIN:VCALENDAR\r\n",
        "VERSION:2.0\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:syd-apr3@pigment\r\n",
        "SUMMARY:Sydney Before Apr 6 End\r\n",
        "DTSTART;TZID=Australia/Sydney:20250403T110000\r\n",
        "DTEND;TZID=Australia/Sydney:20250403T120000\r\n",
        "END:VEVENT\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:syd-oct3@pigment\r\n",
        "SUMMARY:Sydney Before Oct 5 Start\r\n",
        "DTSTART;TZID=Australia/Sydney:20251003T100000\r\n",
        "DTEND;TZID=Australia/Sydney:20251003T110000\r\n",
        "END:VEVENT\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:akl-sep26@pigment\r\n",
        "SUMMARY:Auckland Before Sep 28 Start\r\n",
        "DTSTART;TZID=Pacific/Auckland:20250926T120000\r\n",
        "DTEND;TZID=Pacific/Auckland:20250926T130000\r\n",
        "END:VEVENT\r\n",
        "BEGIN:VEVENT\r\n",
        "UID:akl-sep29@pigment\r\n",
        "SUMMARY:Auckland After Sep 28 Start\r\n",
        "DTSTART;TZID=Pacific/Auckland:20250929T130000\r\n",
        "DTEND;TZID=Pacific/Auckland:20250929T140000\r\n",
        "END:VEVENT\r\n",
        "END:VCALENDAR\r\n",
    );

    let events = parse_ics(raw);
    assert_eq!(events.len(), 4);

    // 2025-04-03 11:00 Australia/Sydney is still AEDT (+11) -> 00:00 UTC
    let syd_apr3 = chrono::DateTime::parse_from_rfc3339("2025-04-03T00:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(events[0].start_at, UnixMs(syd_apr3));

    // 2025-10-03 10:00 Australia/Sydney is still AEST (+10) -> 00:00 UTC
    let syd_oct3 = chrono::DateTime::parse_from_rfc3339("2025-10-03T00:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(events[1].start_at, UnixMs(syd_oct3));

    // 2025-09-26 12:00 Pacific/Auckland is still NZST (+12) -> 00:00 UTC
    let akl_sep26 = chrono::DateTime::parse_from_rfc3339("2025-09-26T00:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(events[2].start_at, UnixMs(akl_sep26));

    // 2025-09-29 13:00 Pacific/Auckland is NZDT (+13) -> 00:00 UTC
    let akl_sep29 = chrono::DateTime::parse_from_rfc3339("2025-09-29T00:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(events[3].start_at, UnixMs(akl_sep29));
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
async fn availability_respects_calendar_events_and_local_ics_overlays() {
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
async fn calendar_event_rejects_discord_webhook_urls_and_ssrf_ics_overlays() {
    let (core, _secrets) = test_core();

    for webhook in [
        "https://discord.com/api/webhooks/123456789/secret-token",
        "https://canary.discord.com/API/WEBHOOKS/123456789/secret-token",
    ] {
        let err = core
            .save_calendar_event(SaveCalendarEventInput {
                id: None,
                series_id: None,
                title: "Webhook Leak Attempt".to_string(),
                description: String::new(),
                start_at: UnixMs(BASE_MS + DAY),
                end_at: UnixMs(BASE_MS + DAY + HOUR),
                all_day: false,
                kind: CalendarEventKind::Meeting,
                department: None,
                location: None,
                recurrence: RecurrenceRule::None,
                discord_url: Some(webhook.to_string()),
                attendees: Vec::new(),
                source_ics_uid: None,
            })
            .await
            .expect_err("webhook URL must be rejected on calendar_event");
        assert!(matches!(err, mail_core::error::CoreError::InvalidInput { .. }));
    }

    for bad_ics in [
        "http://example.com/feed.ics",
        "https://169.254.169.254/latest/meta-data/",
        "https://127.0.0.1/feed.ics",
        "file:///etc/passwd",
    ] {
        let err = core
            .save_ics_overlay(SaveIcsOverlayInput {
                id: None,
                name: "Bad Feed".to_string(),
                url: bad_ics.to_string(),
                color: "violet".to_string(),
                enabled: true,
            })
            .await
            .expect_err("disallowed ICS URL must be rejected");
        assert!(matches!(err, mail_core::error::CoreError::InvalidInput { .. }));
    }
}

#[tokio::test]
async fn delete_account_atomically_removes_messages_and_fts_entries() {
    let (core, _secrets) = test_core();
    core.seed_demo_if_empty().await.expect("seed demo");
    let accounts = core.list_accounts().await.expect("list accounts");
    assert!(!accounts.is_empty());
    let target = accounts[0].id;

    let before_threads = core
        .list_threads(mail_core::types::ThreadQuery {
            scope: mail_core::types::Scope::Account { account_id: target },
            view: mail_core::types::MailboxView::Inbox {
                category: mail_core::types::InboxCategory::All,
            },
            limit: 20,
            offset: 0,
        })
        .await
        .expect("list threads before delete");
    assert!(!before_threads.items.is_empty());

    core.remove_account(target).await.expect("remove account");

    let after_accounts = core.list_accounts().await.expect("list accounts after");
    assert!(!after_accounts.iter().any(|a| a.id == target));
    let after_search = core
        .agent_port(mail_core::ids::AgentId::parse("test-agent").expect("agent id"))
        .search("the", 50)
        .await
        .expect("search after delete");
    assert!(
        !after_search.iter().any(|t| t.account_id == target),
        "deleted account messages must not appear in FTS search"
    );
}
