//! End-to-end integration tests for Phase E:
//! - Companion Staff Sync Server bearer auth, SQLite persistence, and draft rev monotonicity
//! - Two `MailCore` clients syncing live over SSE (threads, handoff, draft locks, comments,
//!   calendar events, meeting docs, and action items)
//! - Durable SQLite offline outbox queuing edits while offline and flushing automatically on reconnect

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mail_core::auth::MemorySecretStore;
use mail_core::calendar::{
    ActionItemStatus, AgendaItem, CalendarEventKind, CalendarRangeQuery, MeetingDocAction,
    RecurrenceRule, SaveActionItemInput, SaveCalendarEventInput, UpdateMeetingDocInput,
};
use mail_core::events::CoreEvent;
use mail_core::host::ManualClock;
use mail_core::ids::UnixMs;
use mail_core::staff::{
    AddSharedCommentInput, ConnectStaffWorkspaceInput, DraftLockAction, DraftLockState,
    HandoffStatus, ShareThreadInput, ShareVisibility, StaffSyncStatus, UpdateHandoffInput,
    UpdateSharedDraftInput,
};
use mail_core::types::{InboxCategory, MailboxView, Scope, SpaceKind, ThreadQuery, UpdateAccountInput};
use mail_core::{CoreConfig, Host, MailCore};
use staff_sync_server::{spawn_server, ServerConfig};
use uuid::Uuid;

const BASE_MS: i64 = 1_780_000_000_000;
const HOUR: i64 = 3_600_000;
const DAY: i64 = 86_400_000;

fn open_test_core(db_path: Option<PathBuf>, clock_ms: i64) -> MailCore {
    let secrets = Arc::new(MemorySecretStore::default());
    MailCore::open(
        CoreConfig {
            db_path,
            ..CoreConfig::default()
        },
        Host {
            secrets,
            clock: Arc::new(ManualClock::new(UnixMs(clock_ms))),
        },
    )
    .expect("open test MailCore")
}

async fn wait_for_state<F, Fut>(
    rx: &mut tokio::sync::broadcast::Receiver<CoreEvent>,
    mut check: F,
) where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    if check().await {
        return;
    }
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for peer sync state");
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {
                if check().await {
                    return;
                }
            }
            other => panic!("event channel ended unexpectedly: {other:?}"),
        }
    }
}

#[tokio::test]
async fn server_enforces_bearer_auth_and_draft_rev_monotonicity() {
    let server = spawn_server(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        ServerConfig::with_tokens(vec!["pigment-token-123".into()], None),
    )
    .await
    .expect("spawn server");

    let http = reqwest::Client::new();

    // /health is unauthenticated for Fly.io / VPS probes.
    let health = http
        .get(format!("{}/health", server.url()))
        .send()
        .await
        .expect("health");
    assert_eq!(health.status(), reqwest::StatusCode::OK);

    // Missing or wrong bearer token -> 401.
    let no_auth = http
        .get(format!("{}/v1/snapshot", server.url()))
        .send()
        .await
        .expect("no auth");
    assert_eq!(no_auth.status(), reqwest::StatusCode::UNAUTHORIZED);

    let wrong_auth = http
        .get(format!("{}/v1/snapshot", server.url()))
        .bearer_auth("wrong-token")
        .send()
        .await
        .expect("wrong auth");
    assert_eq!(wrong_auth.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Valid token -> 200.
    let ok_auth = http
        .get(format!("{}/v1/snapshot", server.url()))
        .bearer_auth("pigment-token-123")
        .send()
        .await
        .expect("ok auth");
    assert_eq!(ok_auth.status(), reqwest::StatusCode::OK);

    // Connect a client, share a thread, claim & save draft (rev=3), then push a stale rev=2 directly.
    let client = open_test_core(None, BASE_MS);
    client.seed_demo_if_empty().await.expect("seed");
    client
        .connect_staff_workspace(ConnectStaffWorkspaceInput {
            server_url: server.url(),
            token: "pigment-token-123".into(),
            staff_name: "Reyth".into(),
            staff_handle: "reyth".into(),
            staff_email: "info@pigmentminiatures.com".into(),
            staff_role: "Director".into(),
        })
        .await
        .expect("connect");

    let accts = client.list_accounts().await.expect("accts");
    client
        .update_account(UpdateAccountInput {
            account_id: accts[0].id,
            display_name: accts[0].display_name.clone(),
            color: accts[0].color,
            space_kind: Some(SpaceKind::PigmentStaff),
        })
        .await
        .expect("staff space");
    let threads = client
        .list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: accts[0].id,
            },
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 5,
            offset: 0,
        })
        .await
        .expect("threads");

    let shared = client
        .share_thread(ShareThreadInput {
            thread_id: threads.items[0].id,
            visibility: ShareVisibility::AllStaff,
            invited_handles: vec![],
            assignee_handle: None,
            initial_note: None,
        })
        .await
        .expect("share");

    let claimed = client
        .update_shared_draft(UpdateSharedDraftInput {
            shared_thread_id: shared.id,
            action: DraftLockAction::Claim,
        })
        .await
        .expect("claim");
    let saved = client
        .update_shared_draft(UpdateSharedDraftInput {
            shared_thread_id: shared.id,
            action: DraftLockAction::Save {
                subject: "Re: Booth".into(),
                to: vec!["dana@ferngate.co".into()],
                cc: vec![],
                body_text: "Confirmed booth A1.".into(),
            },
        })
        .await
        .expect("save");
    assert!(saved.rev > claimed.rev);

    // Attempt to overwrite server draft with stale rev -> 409 Conflict.
    let stale_resp = http
        .put(format!("{}/v1/shared-threads/{}/draft", server.url(), shared.id))
        .bearer_auth("pigment-token-123")
        .json(&claimed)
        .send()
        .await
        .expect("put stale draft");
    assert_eq!(stale_resp.status(), reqwest::StatusCode::CONFLICT);

    client.stop_staff_sync_listener();
    server.shutdown().await;
}

#[tokio::test]
async fn two_clients_sync_live_via_sse_for_threads_drafts_calendar_and_actions() {
    let server = spawn_server(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        ServerConfig::with_tokens(vec!["live-sse-token".into()], None),
    )
    .await
    .expect("spawn server");

    let alice = open_test_core(None, BASE_MS);
    let bob = open_test_core(None, BASE_MS + 1_000);
    alice.seed_demo_if_empty().await.expect("seed alice");

    alice
        .connect_staff_workspace(ConnectStaffWorkspaceInput {
            server_url: server.url(),
            token: "live-sse-token".into(),
            staff_name: "Alice Director".into(),
            staff_handle: "alice".into(),
            staff_email: "info@pigmentminiatures.com".into(),
            staff_role: "Director".into(),
        })
        .await
        .expect("connect alice");

    bob.connect_staff_workspace(ConnectStaffWorkspaceInput {
        server_url: server.url(),
        token: "live-sse-token".into(),
        staff_name: "Bob Sponsors".into(),
        staff_handle: "bob".into(),
        staff_email: "sponsors@pigmentminiatures.com".into(),
        staff_role: "Sponsors Lead".into(),
    })
    .await
    .expect("connect bob");

    // Allow SSE streams to establish before sending live mutations.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let mut bob_rx = bob.subscribe();

    // 1. Alice shares a thread and hands it off to Bob -> Bob receives live via SSE!
    let accts = alice.list_accounts().await.expect("accts");
    alice
        .update_account(UpdateAccountInput {
            account_id: accts[0].id,
            display_name: accts[0].display_name.clone(),
            color: accts[0].color,
            space_kind: Some(SpaceKind::PigmentStaff),
        })
        .await
        .expect("staff space");
    let threads = alice
        .list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: accts[0].id,
            },
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 5,
            offset: 0,
        })
        .await
        .expect("threads");

    let shared = alice
        .share_thread(ShareThreadInput {
            thread_id: threads.items[0].id,
            visibility: ShareVisibility::AllStaff,
            invited_handles: vec![],
            assignee_handle: Some("bob".into()),
            initial_note: Some("Hey @bob please check this sponsor note.".into()),
        })
        .await
        .expect("alice shares thread");

    wait_for_state(&mut bob_rx, || async {
        bob.open_shared_thread(shared.id)
            .await
            .map(|t| t.assignee_handle.as_deref() == Some("bob") && !t.comments.is_empty())
            .unwrap_or(false)
    })
    .await;
    let bob_thread = bob.open_shared_thread(shared.id).await.expect("bob sees thread");
    assert_eq!(bob_thread.assignee_handle.as_deref(), Some("bob"));

    // 2. Alice claims draft lock ("Editing: Alice Director") -> Bob sees lock live!
    alice
        .update_shared_draft(UpdateSharedDraftInput {
            shared_thread_id: shared.id,
            action: DraftLockAction::Claim,
        })
        .await
        .expect("alice claims draft");

    wait_for_state(&mut bob_rx, || async {
        bob.open_shared_thread(shared.id)
            .await
            .ok()
            .and_then(|t| t.draft)
            .is_some_and(|d| d.lock_state == DraftLockState::Editing)
    })
    .await;
    let bob_thread = bob.open_shared_thread(shared.id).await.expect("bob sees draft lock");
    let draft = bob_thread.draft.expect("draft present");
    assert_eq!(draft.lock_state, DraftLockState::Editing);
    assert_eq!(draft.editor_name.as_deref(), Some("Alice Director"));

    // 3. Alice adds a calendar event, meeting doc agenda, and action item -> Bob sees live!
    let event = alice
        .save_calendar_event(SaveCalendarEventInput {
            id: None,
            series_id: None,
            title: "Sponsor Sync Live".into(),
            description: "Review gold tier booths".into(),
            start_at: UnixMs(BASE_MS + DAY),
            end_at: UnixMs(BASE_MS + DAY + HOUR),
            all_day: false,
            kind: CalendarEventKind::Meeting,
            department: Some("sponsors".into()),
            location: Some("Discord #sponsors".into()),
            recurrence: RecurrenceRule::None,
            discord_url: None,
            attendees: vec!["alice".into(), "bob".into()],
            source_ics_uid: None,
        })
        .await
        .expect("alice saves event");

    wait_for_state(&mut bob_rx, || async {
        bob.list_calendar_events(CalendarRangeQuery {
            range_start: UnixMs(BASE_MS),
            range_end: UnixMs(BASE_MS + 3 * DAY),
        })
        .await
        .map(|evs| evs.iter().any(|o| o.event.id == event.id))
        .unwrap_or(false)
    })
    .await;

    alice
        .update_meeting_doc(UpdateMeetingDocInput {
            event_id: event.id,
            action: MeetingDocAction::UpdateAgenda {
                agenda: vec![AgendaItem {
                    id: "ag-1".into(),
                    text: "Confirm Ferngate booth power".into(),
                    checked: false,
                    added_by: "alice".into(),
                    promoted_action_id: None,
                }],
            },
        })
        .await
        .expect("alice updates meeting doc");

    wait_for_state(&mut bob_rx, || async {
        bob.get_meeting_doc(event.id)
            .await
            .map(|doc| doc.agenda.len() == 1)
            .unwrap_or(false)
    })
    .await;
    let bob_doc = bob.get_meeting_doc(event.id).await.expect("bob meeting doc");
    assert_eq!(bob_doc.agenda[0].text, "Confirm Ferngate booth power");

    let action = alice
        .save_action_item(SaveActionItemInput {
            id: None,
            title: "Send booth invoice".into(),
            department: Some("sponsors".into()),
            milestone: Some("PMS 2027".into()),
            assignee_handle: Some("bob".into()),
            status: ActionItemStatus::Open,
            due_at: Some(UnixMs(BASE_MS + 2 * DAY)),
            start_time: None,
            end_time: None,
            location: None,
            cost_note: Some("$1,500".into()),
            source_event_id: Some(event.id),
            source_series_id: None,
            source_thread_id: None,
            sync_to_calendar: false,
        })
        .await
        .expect("alice saves action item");

    wait_for_state(&mut bob_rx, || async {
        bob.list_action_items()
            .await
            .map(|items| items.iter().any(|a| a.id == action.id))
            .unwrap_or(false)
    })
    .await;
    let bob_actions = bob.list_action_items().await.expect("bob actions");
    assert!(bob_actions.iter().any(|a| a.id == action.id && a.title == "Send booth invoice"));

    alice.stop_staff_sync_listener();
    bob.stop_staff_sync_listener();
    server.shutdown().await;
}

#[tokio::test]
async fn offline_outbox_queues_edits_and_flushes_on_reconnect() {
    let server_db = std::env::temp_dir().join(format!("ferrule-srv-offline-{}.db", Uuid::now_v7()));
    let server1 = spawn_server(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        ServerConfig::with_tokens(vec!["offline-token".into()], Some(server_db.clone())),
    )
    .await
    .expect("spawn server 1");
    let bound_addr = server1.addr;
    let server_url = server1.url();

    let alice = open_test_core(None, BASE_MS);
    alice.seed_demo_if_empty().await.expect("seed alice");
    alice
        .connect_staff_workspace(ConnectStaffWorkspaceInput {
            server_url: server_url.clone(),
            token: "offline-token".into(),
            staff_name: "Alice".into(),
            staff_handle: "alice".into(),
            staff_email: "info@pigmentminiatures.com".into(),
            staff_role: "Director".into(),
        })
        .await
        .expect("connect alice");

    // Share one thread while online so we can comment/handoff on it while offline.
    let accts = alice.list_accounts().await.expect("accts");
    alice
        .update_account(UpdateAccountInput {
            account_id: accts[0].id,
            display_name: accts[0].display_name.clone(),
            color: accts[0].color,
            space_kind: Some(SpaceKind::PigmentStaff),
        })
        .await
        .expect("staff space");
    let threads = alice
        .list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: accts[0].id,
            },
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 5,
            offset: 0,
        })
        .await
        .expect("threads");
    let shared = alice
        .share_thread(ShareThreadInput {
            thread_id: threads.items[0].id,
            visibility: ShareVisibility::AllStaff,
            invited_handles: vec![],
            assignee_handle: None,
            initial_note: None,
        })
        .await
        .expect("share while online");
    assert_eq!(alice.pending_staff_outbox_count().await.expect("outbox"), 0);

    // Take the server offline.
    alice.stop_staff_sync_listener();
    server1.shutdown().await;

    // Make multiple edits while offline: handoff, comment, calendar event, and action item.
    alice
        .update_shared_handoff(UpdateHandoffInput {
            shared_thread_id: shared.id,
            assignee_handle: Some("bob".into()),
            status: HandoffStatus::Waiting,
        })
        .await
        .expect("offline handoff");

    alice
        .add_shared_comment(AddSharedCommentInput {
            shared_thread_id: shared.id,
            body_text: "Offline note for @bob while on plane wifi.".into(),
        })
        .await
        .expect("offline comment");

    let offline_event = alice
        .save_calendar_event(SaveCalendarEventInput {
            id: None,
            series_id: None,
            title: "Offline Venue Walkthrough".into(),
            description: String::new(),
            start_at: UnixMs(BASE_MS + 2 * DAY),
            end_at: UnixMs(BASE_MS + 2 * DAY + HOUR),
            all_day: false,
            kind: CalendarEventKind::Milestone,
            department: Some("operations".into()),
            location: Some("Hall B".into()),
            recurrence: RecurrenceRule::None,
            discord_url: None,
            attendees: vec![],
            source_ics_uid: None,
        })
        .await
        .expect("offline event");

    let offline_action = alice
        .save_action_item(SaveActionItemInput {
            id: None,
            title: "Order lanyards offline".into(),
            department: Some("operations".into()),
            milestone: None,
            assignee_handle: Some("bob".into()),
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
        .expect("offline action");

    let ws_offline = alice.get_staff_workspace().await.expect("ws");
    assert_eq!(ws_offline.status, StaffSyncStatus::Offline);
    assert_eq!(alice.pending_staff_outbox_count().await.expect("outbox"), 4);

    // Restart the server on the same port and SQLite DB.
    let server2 = spawn_server(
        bound_addr,
        ServerConfig::with_tokens(vec!["offline-token".into()], Some(server_db.clone())),
    )
    .await
    .expect("restart server 2");

    // Syncing Alice flushes all 4 queued outbox operations and returns status to Connected.
    let ws_recovered = alice.sync_staff_workspace().await.expect("sync recover");
    assert_eq!(ws_recovered.status, StaffSyncStatus::Connected);
    assert_eq!(alice.pending_staff_outbox_count().await.expect("outbox"), 0);

    // Connect Bob fresh and verify Bob pulls every offline edit Alice made!
    let bob = open_test_core(None, BASE_MS + 5_000);
    bob.connect_staff_workspace(ConnectStaffWorkspaceInput {
        server_url,
        token: "offline-token".into(),
        staff_name: "Bob".into(),
        staff_handle: "bob".into(),
        staff_email: "sponsors@pigmentminiatures.com".into(),
        staff_role: "Sponsors Lead".into(),
    })
    .await
    .expect("connect bob");

    let bob_thread = bob.open_shared_thread(shared.id).await.expect("bob thread");
    assert_eq!(bob_thread.status, HandoffStatus::Waiting);
    assert_eq!(bob_thread.assignee_handle.as_deref(), Some("bob"));
    assert!(bob_thread
        .comments
        .iter()
        .any(|c| c.body_text.contains("plane wifi")));

    let bob_events = bob
        .list_calendar_events(CalendarRangeQuery {
            range_start: UnixMs(BASE_MS),
            range_end: UnixMs(BASE_MS + 5 * DAY),
        })
        .await
        .expect("bob events");
    assert!(bob_events.iter().any(|o| o.event.id == offline_event.id));

    let bob_actions = bob.list_action_items().await.expect("bob actions");
    assert!(bob_actions.iter().any(|a| a.id == offline_action.id));

    bob.stop_staff_sync_listener();
    server2.shutdown().await;
    let _ = std::fs::remove_file(&server_db);
    let _ = std::fs::remove_file(format!("{}-wal", server_db.display()));
    let _ = std::fs::remove_file(format!("{}-shm", server_db.display()));
}
