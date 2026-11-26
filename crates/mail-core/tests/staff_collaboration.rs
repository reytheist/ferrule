//! Phase B integration tests: Staff Sync secret isolation, personal-space guardrail,
//! selective thread sharing & sanitization, handoff + comments + @mentions, draft lock
//! state machine, shared templates, show contacts, pinned links, and SSE parsing.

use std::path::PathBuf;
use std::sync::Arc;

use mail_core::auth::{Endpoint, MemorySecretStore, ProviderAuth, SecretKey, SecretStore};
use mail_core::error::CoreError;
use mail_core::host::ManualClock;
use mail_core::ids::{SharedThreadId, UnixMs};
use mail_core::staff::{
    AddSharedCommentInput, ConnectStaffWorkspaceInput, ContactCategory, DraftLockAction,
    DraftLockState, HandoffStatus, SavePinnedLinkInput, SaveSharedTemplateInput,
    SaveShowContactInput, ShareThreadInput, ShareVisibility, StaffSyncStatus, UpdateHandoffInput,
    UpdateSharedDraftInput,
};
use mail_core::types::{InboxCategory, MailboxView, NewAccountInput, Scope, SpaceColor, SpaceKind, ThreadQuery, UpdateAccountInput};
use mail_core::{parse_sse_frame, CoreConfig, Host, MailCore, StaffSyncEvent};
use uuid::Uuid;

const START: i64 = 1_780_000_000_000;

fn open_test_core() -> (MailCore, Arc<MemorySecretStore>) {
    let secrets = Arc::new(MemorySecretStore::default());
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: secrets.clone(),
            clock: Arc::new(ManualClock::new(UnixMs(START))),
        },
    )
    .expect("open core");
    (core, secrets)
}

#[tokio::test]
async fn staff_sync_token_stays_strictly_in_secret_store_never_sqlite() {
    const TOKEN: &str = "pigment-staff-invite-token-998877";
    let db_path: PathBuf =
        std::env::temp_dir().join(format!("ferrule-staff-secret-{}.db", Uuid::now_v7()));
    let secrets = Arc::new(MemorySecretStore::default());
    let core = MailCore::open(
        CoreConfig {
            db_path: Some(db_path.clone()),
            ..CoreConfig::default()
        },
        Host {
            secrets: secrets.clone(),
            clock: Arc::new(ManualClock::new(UnixMs(START))),
        },
    )
    .expect("open core");

    let ws = core
        .connect_staff_workspace(ConnectStaffWorkspaceInput {
            server_url: "local://pigment-demo".into(),
            token: TOKEN.into(),
            staff_name: "Reyth".into(),
            staff_handle: "@Reyth".into(),
            staff_email: "info@pigmentminiatures.com".into(),
            staff_role: "Show Director".into(),
        })
        .await
        .expect("connect workspace");
    assert!(ws.connected);
    assert_eq!(ws.staff_handle, "reyth");
    assert_eq!(ws.status, StaffSyncStatus::Connected);

    // Token lives in SecretStore...
    let stored = secrets
        .get(&SecretKey::staff_sync_token())
        .expect("get")
        .expect("token present");
    assert_eq!(stored.expose(), TOKEN);

    // ...and never appears in SQLite file bytes.
    for suffix in ["", "-wal", "-shm"] {
        let mut path = db_path.clone().into_os_string();
        path.push(suffix);
        let bytes = std::fs::read(&path).unwrap_or_default();
        assert!(
            !bytes.windows(TOKEN.len()).any(|w| w == TOKEN.as_bytes()),
            "staff token leaked into {path:?}"
        );
    }

    // Local demo seeded directory, shared templates, contacts, and pinned links.
    assert!(core.list_staff_directory().await.expect("dir").len() >= 5);
    assert!(!core.list_shared_templates().await.expect("tpls").is_empty());
    assert!(!core.list_show_contacts().await.expect("contacts").is_empty());
    assert!(!core.list_pinned_links().await.expect("links").is_empty());

    // Disconnect clears token from SecretStore.
    core.disconnect_staff_workspace().await.expect("disconnect");
    assert!(secrets
        .get(&SecretKey::staff_sync_token())
        .expect("get")
        .is_none());
    assert!(!core.get_staff_workspace().await.expect("ws").connected);

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}

#[tokio::test]
async fn personal_space_guardrail_blocks_sharing_and_staff_space_allows_it() {
    let (core, _) = open_test_core();
    core.seed_demo_if_empty().await.expect("seed");
    core.connect_staff_workspace(ConnectStaffWorkspaceInput {
        server_url: "local://pigment-demo".into(),
        token: "demo-token".into(),
        staff_name: "Reyth".into(),
        staff_handle: "reyth".into(),
        staff_email: "info@pigmentminiatures.com".into(),
        staff_role: "Show Director".into(),
    })
    .await
    .expect("connect");

    let inbox = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 50,
            offset: 0,
        })
        .await
        .expect("inbox");
    let first = &inbox.items[0];

    // Demo accounts are Personal by default -> sharing must be rejected!
    let denied = core
        .share_thread(ShareThreadInput {
            thread_id: first.id,
            visibility: ShareVisibility::AllStaff,
            invited_handles: vec![],
            assignee_handle: Some("maya".into()),
            initial_note: None,
        })
        .await;
    assert!(matches!(denied, Err(CoreError::IllegalState { .. })));

    // Switch that account to PigmentStaff -> sharing succeeds!
    let acct = core
        .list_accounts()
        .await
        .expect("accounts")
        .into_iter()
        .find(|a| a.id == first.account_id)
        .expect("account");
    core.update_account(mail_core::types::UpdateAccountInput {
        account_id: acct.id,
        display_name: acct.display_name,
        color: acct.color,
        space_kind: Some(SpaceKind::PigmentStaff),
    })
    .await
    .expect("upgrade to staff space");

    let shared = core
        .share_thread(ShareThreadInput {
            thread_id: first.id,
            visibility: ShareVisibility::Specific,
            invited_handles: vec!["@maya".into(), "devon".into()],
            assignee_handle: Some("maya".into()),
            initial_note: Some("Hey @maya can you take point on this invoice?".into()),
        })
        .await
        .expect("share staff thread");

    assert_eq!(shared.visibility, ShareVisibility::Specific);
    assert_eq!(shared.invited_handles, vec!["devon", "maya"]);
    assert_eq!(shared.assignee_handle.as_deref(), Some("maya"));
    assert_eq!(shared.status, HandoffStatus::Open);
    assert_eq!(shared.comments.len(), 1);
    assert_eq!(shared.comments[0].mentions, vec!["maya"]);
    assert!(!shared.messages.is_empty());

    // Handoff status update
    let handed = core
        .update_shared_handoff(UpdateHandoffInput {
            shared_thread_id: shared.id,
            assignee_handle: Some("devon".into()),
            status: HandoffStatus::Waiting,
        })
        .await
        .expect("handoff");
    assert_eq!(handed.assignee_handle.as_deref(), Some("devon"));
    assert_eq!(handed.status, HandoffStatus::Waiting);

    // Add comment with multiple @mentions
    let comment = core
        .add_shared_comment(AddSharedCommentInput {
            shared_thread_id: shared.id,
            body_text: "Looping in @tessa and @sam for weekend setup.".into(),
        })
        .await
        .expect("comment");
    assert_eq!(comment.mentions, vec!["tessa", "sam"]);
}

#[tokio::test]
async fn draft_lock_claim_conflict_takeover_review_and_approve() {
    let (core, _) = open_test_core();
    core.seed_demo_if_empty().await.expect("seed");
    core.connect_staff_workspace(ConnectStaffWorkspaceInput {
        server_url: "local://pigment-demo".into(),
        token: "demo-token".into(),
        staff_name: "Reyth".into(),
        staff_handle: "reyth".into(),
        staff_email: "info@pigmentminiatures.com".into(),
        staff_role: "Show Director".into(),
    })
    .await
    .expect("connect");

    let inbox = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 10,
            offset: 0,
        })
        .await
        .expect("inbox");
    let first = &inbox.items[0];
    core.update_account(mail_core::types::UpdateAccountInput {
        account_id: first.account_id,
        display_name: "Pigment · Sponsors".into(),
        color: SpaceColor::Amber,
        space_kind: Some(SpaceKind::PigmentStaff),
    })
    .await
    .expect("staff space");

    let shared = core
        .share_thread(ShareThreadInput {
            thread_id: first.id,
            visibility: ShareVisibility::AllStaff,
            invited_handles: vec![],
            assignee_handle: None,
            initial_note: None,
        })
        .await
        .expect("share");

    // Reyth claims lock and saves draft.
    let claimed = core
        .update_shared_draft(UpdateSharedDraftInput {
            shared_thread_id: shared.id,
            action: DraftLockAction::Claim,
        })
        .await
        .expect("claim");
    assert_eq!(claimed.lock_state, DraftLockState::Editing);
    assert_eq!(claimed.editor_handle.as_deref(), Some("reyth"));

    let saved = core
        .update_shared_draft(UpdateSharedDraftInput {
            shared_thread_id: shared.id,
            action: DraftLockAction::Save {
                subject: "Re: Sponsor Booth".into(),
                to: vec!["dana@ferngate.co".into()],
                cc: vec![],
                body_text: "Confirmed — we reserved corner booth A1.".into(),
            },
        })
        .await
        .expect("save");
    assert_eq!(saved.body_text, "Confirmed — we reserved corner booth A1.");

    // Switch identity to Maya to test conflict & explicit TakeOver.
    core.connect_staff_workspace(ConnectStaffWorkspaceInput {
        server_url: "local://pigment-demo".into(),
        token: "demo-token".into(),
        staff_name: "Maya Chen".into(),
        staff_handle: "maya".into(),
        staff_email: "sponsors@pigmentminiatures.com".into(),
        staff_role: "Sponsors Lead".into(),
    })
    .await
    .expect("switch to maya");

    let conflict = core
        .update_shared_draft(UpdateSharedDraftInput {
            shared_thread_id: shared.id,
            action: DraftLockAction::Claim,
        })
        .await;
    assert!(matches!(conflict, Err(CoreError::Conflict { .. })));

    let taken = core
        .update_shared_draft(UpdateSharedDraftInput {
            shared_thread_id: shared.id,
            action: DraftLockAction::TakeOver,
        })
        .await
        .expect("take over");
    assert_eq!(taken.lock_state, DraftLockState::Editing);
    assert_eq!(taken.editor_handle.as_deref(), Some("maya"));

    // Request Review releases lock and posts a review comment mentioning @reyth.
    let review = core
        .update_shared_draft(UpdateSharedDraftInput {
            shared_thread_id: shared.id,
            action: DraftLockAction::RequestReview {
                reviewer_handles: vec!["reyth".into()],
                note: Some("Ready for final check!".into()),
            },
        })
        .await
        .expect("request review");
    assert_eq!(review.lock_state, DraftLockState::ReviewRequested);
    assert!(review.editor_handle.is_none());

    let reopened = core.open_shared_thread(shared.id).await.expect("reopen");
    assert!(reopened
        .comments
        .iter()
        .any(|c| c.mentions.contains(&"reyth".to_string())));

    // Approve marks draft ready for mailbox owner to send.
    let approved = core
        .update_shared_draft(UpdateSharedDraftInput {
            shared_thread_id: shared.id,
            action: DraftLockAction::Approve,
        })
        .await
        .expect("approve");
    assert_eq!(approved.lock_state, DraftLockState::Approved);
    assert_eq!(approved.approved_by.as_deref(), Some("maya"));
    assert!(approved.approved_at.is_some());
}

#[tokio::test]
async fn shared_templates_contacts_and_pinned_links_crud() {
    let (core, _) = open_test_core();

    // Shared templates
    let tpl = core
        .save_shared_template(SaveSharedTemplateInput {
            id: None,
            name: "Booth Confirmation".into(),
            shortcut: Some("/booth".into()),
            category: "sponsors".into(),
            subject: "Booth Confirmed".into(),
            body_text: "Your booth is locked in.".into(),
        })
        .await
        .expect("save shared template");
    assert_eq!(core.list_shared_templates().await.expect("list").len(), 1);
    core.delete_shared_template(tpl.id).await.expect("delete");
    assert!(core.list_shared_templates().await.expect("list").is_empty());

    // Show contacts + email lookup
    let contact = core
        .save_show_contact(SaveShowContactInput {
            id: None,
            name: "Elena Vance".into(),
            emails: vec!["Elena@BrushCraft.io".into()],
            category: ContactCategory::Vendor,
            organization: Some("BrushCraft".into()),
            tier: Some("Double Booth".into()),
            notes: "Needs 2 chairs + power.".into(),
        })
        .await
        .expect("save contact");
    let found = core
        .find_contact_by_email("elena@brushcraft.io".into())
        .await
        .expect("lookup")
        .expect("contact matched");
    assert_eq!(found.id, contact.id);
    assert_eq!(found.category, ContactCategory::Vendor);
    core.delete_show_contact(contact.id).await.expect("delete");

    // Pinned links
    let bad_link = core
        .save_pinned_link(SavePinnedLinkInput {
            id: None,
            title: "Bad".into(),
            url: "javascript:alert(1)".into(),
            category: "ops".into(),
            description: None,
        })
        .await;
    assert!(matches!(bad_link, Err(CoreError::InvalidInput { .. })));

    let link = core
        .save_pinned_link(SavePinnedLinkInput {
            id: None,
            title: "Floor Plan".into(),
            url: "https://docs.google.com/spreadsheets/d/floorplan".into(),
            category: "operations".into(),
            description: Some("2027 hall map".into()),
        })
        .await
        .expect("save link");
    assert_eq!(core.list_pinned_links().await.expect("links").len(), 1);
    core.delete_pinned_link(link.id).await.expect("delete");
    assert!(core.list_pinned_links().await.expect("links").is_empty());

    // Ensure adding an IONOS Pigment Staff account still works cleanly alongside Phase B.
    let acct = core
        .add_account(NewAccountInput {
            display_name: "Pigment · Info".into(),
            color: SpaceColor::Emerald,
            space_kind: SpaceKind::PigmentStaff,
            auth: ProviderAuth::ImapPassword {
                address: "info@pigmentminiatures.com".into(),
                username: "info@pigmentminiatures.com".into(),
                imap: Endpoint {
                    host: "imap.ionos.com".into(),
                    port: 993,
                    tls: true,
                },
                smtp: Endpoint {
                    host: "smtp.ionos.com".into(),
                    port: 587,
                    tls: true,
                },
            },
            secret: Some("ionos-pass".into()),
        })
        .await
        .expect("add ionos staff");
    assert_eq!(acct.space_kind, SpaceKind::PigmentStaff);
}

#[test]
fn sse_frame_parser_handles_events_and_comments() {
    assert_eq!(parse_sse_frame(": keep-alive\n\n"), None);
    let id = SharedThreadId::fresh();
    let frame = format!("event: update\ndata: {{\"type\":\"threadUpdated\",\"sharedThreadId\":\"{id}\"}}\n\n");
    assert_eq!(
        parse_sse_frame(&frame),
        Some(StaffSyncEvent::ThreadUpdated {
            shared_thread_id: id
        })
    );
}

#[tokio::test]
async fn reshare_preserves_handoff_and_rejects_cleartext_non_loopback_http() {
    let (core, _secrets) = open_test_core();
    core.seed_demo_if_empty().await.expect("seed");

    // Non-loopback plaintext http:// must be rejected to protect the Staff Sync bearer token.
    let insecure = core
        .connect_staff_workspace(ConnectStaffWorkspaceInput {
            server_url: "http://sync.example.com".into(),
            token: "secret-token".into(),
            staff_name: "Alex".into(),
            staff_handle: "alex".into(),
            staff_email: "alex@pigmentminiatures.com".into(),
            staff_role: "Director".into(),
        })
        .await;
    assert!(matches!(insecure, Err(CoreError::InvalidInput { .. })));

    core.connect_staff_workspace(ConnectStaffWorkspaceInput {
        server_url: "local://pigment-2027".into(),
        token: "demo-token".into(),
        staff_name: "Alex".into(),
        staff_handle: "alex".into(),
        staff_email: "alex@pigmentminiatures.com".into(),
        staff_role: "Director".into(),
    })
    .await
    .expect("connect local");

    let accounts = core.list_accounts().await.expect("accounts");
    let staff_acct = core
        .update_account(UpdateAccountInput {
            account_id: accounts[0].id,
            display_name: accounts[0].display_name.clone(),
            color: accounts[0].color,
            space_kind: Some(SpaceKind::PigmentStaff),
        })
        .await
        .expect("mark account as pigment staff");
    let threads = core
        .list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: staff_acct.id,
            },
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 5,
            offset: 0,
        })
        .await
        .expect("threads");
    let thread_id = threads.items[0].id;

    let first = core
        .share_thread(ShareThreadInput {
            thread_id,
            visibility: ShareVisibility::AllStaff,
            invited_handles: vec![],
            assignee_handle: Some("jordan".into()),
            initial_note: None,
        })
        .await
        .expect("initial share");

    core.update_shared_handoff(UpdateHandoffInput {
        shared_thread_id: first.id,
        assignee_handle: Some("jordan".into()),
        status: HandoffStatus::Waiting,
    })
    .await
    .expect("handoff waiting");

    // Re-sharing the thread to refresh its message snapshot must preserve Waiting status & assignee.
    let refreshed = core
        .share_thread(ShareThreadInput {
            thread_id,
            visibility: ShareVisibility::AllStaff,
            invited_handles: vec![],
            assignee_handle: None,
            initial_note: None,
        })
        .await
        .expect("re-share");
    assert_eq!(refreshed.id, first.id);
    assert_eq!(refreshed.status, HandoffStatus::Waiting);
    assert_eq!(refreshed.assignee_handle.as_deref(), Some("jordan"));
}

#[tokio::test]
async fn external_and_pinned_link_urls_enforce_parsed_scheme_rules() {
    let (core, _secrets) = open_test_core();

    for bad in [
        "javascript:alert(1)",
        "file:///C:/Windows/System32/calc.exe",
        "data:text/html,<script>alert(1)</script>",
        "http://example.com/non-loopback",
        "https://evil.example/a\nb",
        "discord://",
    ] {
        assert!(
            matches!(
                core.save_pinned_link(SavePinnedLinkInput {
                    id: None,
                    title: "Bad".into(),
                    url: bad.into(),
                    category: "ops".into(),
                    description: None,
                })
                .await,
                Err(CoreError::InvalidInput { .. })
            ),
            "expected save_pinned_link to reject '{bad}'"
        );
        assert!(
            matches!(
                core.open_external_url(bad),
                Err(CoreError::InvalidInput { .. })
            ),
            "expected open_external_url to reject '{bad}'"
        );
    }

    for good in [
        "https://docs.google.com/spreadsheets/d/123",
        "http://localhost:8787/health",
        "http://127.0.0.1:8787/health",
        "discord://-/channels/12345/67890",
    ] {
        core.save_pinned_link(SavePinnedLinkInput {
            id: None,
            title: "Good".into(),
            url: good.into(),
            category: "ops".into(),
            description: None,
        })
        .await
        .unwrap_or_else(|e| panic!("expected '{good}' to be accepted, got {e:?}"));
    }
}
