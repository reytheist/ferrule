//! Thread reading and search: FTS5 + LIKE fallback, mailbox views, idempotent
//! triage actions, read-on-open, and per-message privacy reports.

use std::sync::Arc;

use mail_core::auth::{MemorySecretStore, SecretString};
use mail_core::host::{Clock, ManualClock};
use mail_core::ids::UnixMs;
use mail_core::sanitize::sanitize_html;
use mail_core::types::*;
use mail_core::{CoreConfig, Host, MailCore};

const START: i64 = 1_760_000_000_000;

async fn seeded_core() -> MailCore {
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(UnixMs(START)));
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock,
        },
    )
    .expect("open core");
    assert!(core.seed_demo_if_empty().await.expect("seed"));
    core
}

async fn search(core: &MailCore, text: &str) -> Vec<ThreadSummary> {
    core.list_threads(ThreadQuery {
        scope: Scope::Unified,
        view: MailboxView::Search {
            text: text.to_string(),
        },
        limit: 50,
        offset: 0,
    })
    .await
    .expect("search")
    .items
}

async fn inbox_subjects(core: &MailCore) -> Vec<String> {
    core.list_threads(ThreadQuery {
        scope: Scope::Unified,
        view: MailboxView::Inbox {
            category: InboxCategory::All,
        },
        limit: 50,
        offset: 0,
    })
    .await
    .expect("inbox")
    .items
    .into_iter()
    .map(|t| t.subject)
    .collect()
}

#[tokio::test]
async fn fts5_search_finds_full_and_partial_tokens() {
    let core = seeded_core().await;

    // Full token via FTS5 MATCH.
    let hits = search(&core, "retainer").await;
    assert_eq!(hits.len(), 1);
    assert!(hits[0].subject.contains("Q2 illustration retainer"));

    // Prefix search over a partial token.
    let hits = search(&core, "invoi").await;
    assert_eq!(hits.len(), 1);
    assert!(hits[0].subject.contains("invoice #4821"));

    // Multi-token AND across subject and body.
    let hits = search(&core, "contract redlines").await;
    assert_eq!(hits.len(), 1);
    assert!(hits[0].subject.contains("Contract redlines"));

    // Newsletter hit including a body-only word.
    let hits = search(&core, "specimen").await;
    assert_eq!(hits.len(), 1);
    assert!(hits[0].subject.contains("Design Dispatch"));

    assert!(search(&core, "no-such-token-xyzzy").await.is_empty());
    assert!(search(&core, "   ").await.is_empty());
}

#[tokio::test]
async fn fts5_search_uses_diacritic_folding() {
    let core = seeded_core().await;
    // Two seeded bodies contain "café" and nothing contains plain "cafe" — so
    // LIKE '%cafe%' cannot match them; these hits prove the FTS5 unicode61
    // remove_diacritics path answered.
    let hits = search(&core, "cafe").await;
    assert_eq!(hits.len(), 2, "diacritic-folded FTS matches expected");
    assert!(hits.iter().any(|t| t.subject.contains("Fall layers")));
    assert!(hits.iter().any(|t| t.subject.contains("Design Dispatch")));
}

#[tokio::test]
async fn archive_unarchive_star_and_read_are_idempotent() {
    let core = seeded_core().await;
    let thread_id = {
        let hits = search(&core, "retainer").await;
        hits[0].id
    };

    // Archive twice: same id set both times (idempotent), exact inverse undo.
    let first = core
        .act_on_threads(MailAction::Archive, vec![thread_id])
        .await
        .expect("archive");
    assert_eq!(first.updated_ids, vec![thread_id]);
    assert_eq!(first.undo_action, Some(MailAction::Unarchive));

    let second = core
        .act_on_threads(MailAction::Archive, vec![thread_id])
        .await
        .expect("archive again");
    assert_eq!(second.updated_ids, vec![thread_id]);

    assert!(!inbox_subjects(&core)
        .await
        .iter()
        .any(|s| s.contains("Q2 illustration retainer")));

    let archived = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Archive,
            limit: 50,
            offset: 0,
        })
        .await
        .expect("archive view");
    assert_eq!(archived.total, 1);

    // Undo restores exactly.
    let undo = core
        .act_on_threads(MailAction::Unarchive, vec![thread_id])
        .await
        .expect("unarchive");
    assert_eq!(undo.undo_action, Some(MailAction::Archive));
    assert!(inbox_subjects(&core)
        .await
        .iter()
        .any(|s| s.contains("Q2 illustration retainer")));

    // Star on/off with exact inverses, idempotent under repeat.
    let starred = core
        .act_on_threads(MailAction::Star { on: true }, vec![thread_id])
        .await
        .expect("star");
    assert_eq!(starred.undo_action, Some(MailAction::Star { on: false }));
    let starred_again = core
        .act_on_threads(MailAction::Star { on: true }, vec![thread_id])
        .await
        .expect("star again");
    assert_eq!(starred_again.updated_ids, vec![thread_id]);
    let unstarred = core
        .act_on_threads(MailAction::Star { on: false }, vec![thread_id])
        .await
        .expect("unstar");
    assert_eq!(unstarred.undo_action, Some(MailAction::Star { on: true }));

    // MarkRead is idempotent and inverts exactly.
    let read = core
        .act_on_threads(MailAction::MarkRead { read: true }, vec![thread_id])
        .await
        .expect("mark read");
    assert_eq!(read.undo_action, Some(MailAction::MarkRead { read: false }));
    let page = core
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
    let summary = page
        .items
        .iter()
        .find(|t| t.id == thread_id)
        .expect("thread in inbox");
    assert_eq!(summary.unread_count, 0);

    // Trash + undo restore.
    let trashed = core
        .act_on_threads(MailAction::Trash, vec![thread_id])
        .await
        .expect("trash");
    assert_eq!(trashed.undo_action, Some(MailAction::Unarchive));
    assert!(!inbox_subjects(&core)
        .await
        .iter()
        .any(|s| s.contains("Q2 illustration retainer")));
    core.act_on_threads(MailAction::Unarchive, vec![thread_id])
        .await
        .expect("restore");
    assert!(inbox_subjects(&core)
        .await
        .iter()
        .any(|s| s.contains("Q2 illustration retainer")));

    // Missing ids are skipped, not fatal.
    let ghost = mail_core::ids::ThreadId::fresh();
    let partial = core
        .act_on_threads(MailAction::Archive, vec![thread_id, ghost])
        .await
        .expect("archive with ghost");
    assert_eq!(partial.updated_ids, vec![thread_id]);
    core.act_on_threads(MailAction::Unarchive, vec![thread_id])
        .await
        .expect("undo");
}

#[tokio::test]
async fn open_thread_marks_read_and_reports_privacy() {
    let core = seeded_core().await;
    let hits = search(&core, "Design Dispatch").await;
    let thread_id = hits[0].id;
    assert_eq!(hits[0].unread_count, 1);

    // BlockAll: strict CSP, every remote image gone, accurate counts.
    let blocked = core
        .open_thread(thread_id, RemoteContent::BlockAll)
        .await
        .expect("open block-all");
    assert!(blocked.messages[0].body_srcdoc.contains(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline';\">"
    ));
    assert!(!blocked.messages[0].body_srcdoc.contains("mailchimp"));
    assert!(!blocked.messages[0]
        .body_srcdoc
        .contains("cdn.dispatch.design"));
    match &blocked.messages[0].privacy.remote_images {
        RemoteImagesStatus::Blocked { count } => assert_eq!(*count, 5),
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert_eq!(blocked.messages[0].privacy.trackers.len(), 4);
    let unsub = blocked.messages[0].unsubscribe.as_ref().expect("unsub");
    assert!(unsub.one_click);
    assert!(unsub.target.starts_with("https://"));

    // AllowImagesStripTrackers: hero kept, trackers still stripped.
    let allowed = core
        .open_thread(thread_id, RemoteContent::AllowImagesStripTrackers)
        .await
        .expect("open allow");
    assert!(allowed.messages[0]
        .body_srcdoc
        .contains("https://cdn.dispatch.design/hero-142.jpg"));
    assert!(!allowed.messages[0].body_srcdoc.contains("mailchimp"));
    assert!(!allowed.messages[0]
        .body_srcdoc
        .contains("track.dispatch.design"));
    assert!(matches!(
        allowed.messages[0].privacy.remote_images,
        RemoteImagesStatus::Allowed
    ));
    assert_eq!(allowed.messages[0].privacy.trackers.len(), 4);

    // Opening marks the thread read exactly once.
    let page = core
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
    let summary = page
        .items
        .iter()
        .find(|t| t.id == thread_id)
        .expect("thread");
    assert_eq!(summary.unread_count, 0);
    assert_eq!(
        core.mailbox_counts(Scope::Unified)
            .await
            .expect("counts")
            .inbox,
        5
    );

    // Snoozed thread advertises its timer in the view.
    let snoozed_hits = search(&core, "Studio booking").await;
    let view = core
        .open_thread(snoozed_hits[0].id, RemoteContent::BlockAll)
        .await
        .expect("open snoozed");
    assert!(view.snoozed_until.is_some());
}

#[tokio::test]
async fn open_thread_handles_plain_text_messages() {
    let core = seeded_core().await;
    let hits = search(&core, "kill fee at 25").await;
    assert_eq!(hits.len(), 1);
    let view = core
        .open_thread(hits[0].id, RemoteContent::BlockAll)
        .await
        .expect("open");
    let plain = view
        .messages
        .iter()
        .find(|m| m.from_email == "reyth@outlook.com")
        .expect("our sent message");
    assert!(plain.body_srcdoc.contains("kill fee at 25%"));
    assert!(plain.body_srcdoc.contains("Content-Security-Policy"));
    assert!(plain.body_text.contains("kill fee at 25%"));
    assert!(matches!(
        plain.privacy.remote_images,
        RemoteImagesStatus::None
    ));
}

#[tokio::test]
async fn sanitize_is_also_safe_as_a_standalone_call() {
    // Belt-and-braces: the exported sanitizer never lets script through, regardless
    // of what a caller hands it.
    let raw = r#"<p onclick="x()">hi</p><script>alert(1)</script><img src="https://t.example/pixel.gif" width="1" height="1">"#;
    for policy in [
        RemoteContent::BlockAll,
        RemoteContent::AllowImagesStripTrackers,
    ] {
        let out = sanitize_html(raw, policy);
        let html = out.srcdoc.as_str();
        assert!(!html.contains("alert(1)"));
        assert!(!html.contains("onclick"));
        assert!(!html.contains("t.example"));
        assert!(!out.privacy.trackers.is_empty());
    }
    // The secret store type is importable and round-trips through the core handle.
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(UnixMs(START)));
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock,
        },
    )
    .expect("open");
    let stored = core.secrets().get(&mail_core::auth::SecretKey {
        account_id: mail_core::ids::AccountId::fresh(),
        kind: mail_core::auth::CredentialKind::Password,
    });
    assert!(matches!(stored, Ok(None)));
    let secret = SecretString::new("round-trip");
    assert_eq!(secret.expose(), "round-trip");
}
