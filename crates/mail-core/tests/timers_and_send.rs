//! Scheduler behavior with a ManualClock plus the full outbox pipeline against a
//! local mock SMTP server: Undo Send, Send Later, TooLate, and crash-safe retries.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use mail_core::auth::{Endpoint, MemorySecretStore, ProviderAuth, SecretKey};
use mail_core::events::{Alert, CoreEvent};
use mail_core::host::ManualClock;
use mail_core::ids::UnixMs;
use mail_core::types::*;
use mail_core::{CoreConfig, Host, MailCore};

const START: i64 = 1_760_000_000_000;

// ---------------------------------------------------------------- mock SMTP

/// Minimal SMTP server: one session at a time, acknowledges every command.
struct MockSmtp {
    port: u16,
}

impl MockSmtp {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock smtp");
        let port = listener.local_addr().expect("local addr").port();
        // Detached thread: the listener lives until the test process exits.
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = serve(stream);
            }
        });
        Self { port }
    }
}

fn read_line(input: &mut BufReader<TcpStream>) -> Option<String> {
    let mut buf = String::new();
    match input.read_line(&mut buf) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(buf.trim_end().to_string()),
    }
}

fn reply(out: &mut TcpStream, line: &str) -> std::io::Result<()> {
    // SMTP lines must be CRLF-terminated; lettre's parser waits for it.
    write!(out, "{}\r\n", line)
}

fn serve(stream: TcpStream) -> std::io::Result<()> {
    let mut out = stream.try_clone()?;
    let mut input = BufReader::new(stream);
    reply(&mut out, "220 mock.localhost ESMTP ready")?;
    while let Some(line) = read_line(&mut input) {
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("EHLO") {
            reply(&mut out, "250-mock.localhost")?;
            reply(&mut out, "250 AUTH PLAIN")?;
        } else if upper.starts_with("HELO") {
            reply(&mut out, "250 mock.localhost")?;
        } else if upper.starts_with("AUTH PLAIN") {
            if line.split_whitespace().count() >= 3 {
                reply(&mut out, "235 2.7.23 ok")?;
            } else {
                reply(&mut out, "334")?;
                read_line(&mut input);
                reply(&mut out, "235 2.7.23 ok")?;
            }
        } else if upper.starts_with("MAIL FROM") || upper.starts_with("RCPT TO") {
            reply(&mut out, "250 ok")?;
        } else if upper.starts_with("DATA") {
            reply(&mut out, "354 go ahead")?;
            while let Some(body_line) = read_line(&mut input) {
                if body_line == "." {
                    break;
                }
            }
            reply(&mut out, "250 2.0.0 queued")?;
        } else if upper.starts_with("QUIT") {
            reply(&mut out, "221 bye")?;
            return Ok(());
        } else {
            reply(&mut out, "250 ok")?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- helpers

async fn seeded_core() -> (MailCore, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock::new(UnixMs(START)));
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock: clock.clone(),
        },
    )
    .expect("open core");
    assert!(core.seed_demo_if_empty().await.expect("seed"));
    (core, clock)
}

async fn thread_id_by_subject(core: &MailCore, needle: &str) -> mail_core::ids::ThreadId {
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
    .find(|t| t.subject.contains(needle))
    .map(|t| t.id)
    .unwrap_or_else(|| panic!("thread containing {needle:?}"))
}

async fn inbox_has(core: &MailCore, needle: &str) -> bool {
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
    .iter()
    .any(|t| t.subject.contains(needle))
}

fn take_alerts(receiver: &mut tokio::sync::broadcast::Receiver<CoreEvent>) -> Vec<Alert> {
    let mut alerts = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        if let CoreEvent::Attention { alert } = event {
            alerts.push(alert);
        }
    }
    alerts
}

async fn mock_account(core: &MailCore, smtp_port: u16, label: &str) -> AccountView {
    core.add_account(NewAccountInput {
        display_name: label.to_string(),
        color: SpaceColor::Emerald,
        auth: ProviderAuth::ImapPassword {
            address: format!("{label}@example.test"),
            username: format!("{label}@example.test"),
            imap: Endpoint {
                host: "127.0.0.1".into(),
                port: 1,
                tls: false,
            },
            smtp: Endpoint {
                host: "127.0.0.1".into(),
                port: smtp_port,
                tls: false,
            },
        },
        secret: Some(format!("{label}-password")),
    })
    .await
    .expect("add account")
}

async fn simple_draft(
    core: &MailCore,
    account_id: mail_core::ids::AccountId,
    subject: &str,
) -> DraftView {
    core.create_draft(CreateDraftInput {
        account_id,
        reply_to_thread: None,
        to: vec!["friend@example.test".into()],
        cc: vec![],
        bcc: vec![],
        subject: subject.to_string(),
        body_text: format!("Body for {subject}."),
        body_html: None,
        attachments: vec![],
    })
    .await
    .expect("draft")
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn snooze_and_follow_up_fire_on_the_clock() {
    let (core, clock) = seeded_core().await;
    let mut events = core.subscribe();

    let retainer = thread_id_by_subject(&core, "Q2 illustration retainer").await;

    // Snooze the retainer thread for one hour.
    let until = UnixMs(START + 3_600_000);
    let job = core.snooze_thread(retainer, until).await.expect("snooze");
    assert_eq!(job.kind, "snooze");
    assert!(!inbox_has(&core, "Q2 illustration retainer").await);
    assert_eq!(
        core.mailbox_counts(Scope::Unified)
            .await
            .expect("counts")
            .snoozed,
        2
    );

    // First tick: the seeded (already past-due) follow-up fires, the snooze does not.
    let fired = core.tick_scheduler().await.expect("tick");
    assert!(
        fired.iter().all(|j| j.kind != "snooze"),
        "snooze must not fire before its deadline"
    );
    let alerts = take_alerts(&mut events);
    assert!(
        alerts
            .iter()
            .any(|a| matches!(a, Alert::FollowUpDue { .. })),
        "seeded due follow-up must fire: {alerts:?}"
    );
    assert!(
        !alerts.iter().any(|a| matches!(a, Alert::SnoozeWoke { .. })),
        "snooze must not wake early"
    );

    // Advance past the snooze deadline; next tick wakes it exactly once.
    clock.set(until.add_secs(60));
    let fired = core.tick_scheduler().await.expect("tick2");
    assert!(fired
        .iter()
        .any(|j| j.kind == "snooze" && j.state == "woken"));
    let alerts = take_alerts(&mut events);
    assert!(
        alerts
            .iter()
            .any(|a| matches!(a, Alert::SnoozeWoke { thread_id } if *thread_id == retainer)),
        "expected SnoozeWoke: {alerts:?}"
    );
    assert!(inbox_has(&core, "Q2 illustration retainer").await);
    assert_eq!(
        core.mailbox_counts(Scope::Unified)
            .await
            .expect("counts")
            .snoozed,
        1
    );

    // Firing is idempotent: the woken snooze is gone for good (the seeded booking
    // snooze is still armed in the future).
    let again = core.tick_scheduler().await.expect("tick3");
    assert!(!again.iter().any(|j| j.kind == "snooze"));
    let live_snoozes = core
        .list_jobs()
        .await
        .expect("jobs")
        .into_iter()
        .filter(|j| j.kind == "snooze")
        .collect::<Vec<_>>();
    assert_eq!(
        live_snoozes.len(),
        1,
        "only the future booking snooze remains"
    );
    assert_eq!(live_snoozes[0].state, "armed");

    // The seeded booking thread is still snoozed into the future.
    assert!(!inbox_has(&core, "Studio booking").await);

    // The fired follow-up stays live (and visible) until dismissed.
    let follow = core
        .list_jobs()
        .await
        .expect("jobs")
        .into_iter()
        .find(|j| j.kind == "follow_up")
        .expect("follow-up still listed after firing");
    assert_eq!(follow.state, "fired");
    assert_eq!(follow.thread_id, Some(retainer));
    let follow_ups = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::FollowUps,
            limit: 50,
            offset: 0,
        })
        .await
        .expect("follow-ups");
    assert_eq!(follow_ups.total, 1);

    core.dismiss_follow_up(follow.id).await.expect("dismiss");
    assert_eq!(
        core.mailbox_counts(Scope::Unified)
            .await
            .expect("counts")
            .follow_ups,
        0
    );
    // Dismissing twice is a no-op.
    core.dismiss_follow_up(follow.id)
        .await
        .expect("dismiss again");
}

#[tokio::test]
async fn timers_reject_the_past_and_rearm_in_place() {
    let (core, _clock) = seeded_core().await;
    let retainer = thread_id_by_subject(&core, "Q2 illustration retainer").await;

    let past = UnixMs(START - 1_000);
    assert!(matches!(
        core.snooze_thread(retainer, past).await,
        Err(mail_core::error::CoreError::InvalidInput { .. })
    ));
    assert!(matches!(
        core.set_follow_up(retainer, past).await,
        Err(mail_core::error::CoreError::InvalidInput { .. })
    ));
    assert!(matches!(
        core.set_follow_up(mail_core::ids::ThreadId::fresh(), UnixMs(START + 60_000))
            .await,
        Err(mail_core::error::CoreError::NotFound { .. })
    ));
    assert!(matches!(
        core.unsnooze_thread(mail_core::ids::ThreadId::fresh())
            .await,
        Err(mail_core::error::CoreError::NotFound { .. })
    ));

    // Re-arm the seeded follow-up to a future date: one live job, new deadline.
    let follow = core
        .list_jobs()
        .await
        .expect("jobs")
        .into_iter()
        .find(|j| j.kind == "follow_up")
        .expect("follow-up");
    let rearm_due = UnixMs(START + 7 * 86_400_000);
    let rearmed = core
        .set_follow_up(retainer, rearm_due)
        .await
        .expect("rearm");
    assert_eq!(rearmed.id, follow.id);
    assert_eq!(rearmed.due_at, rearm_due);
    assert_eq!(
        core.list_jobs()
            .await
            .expect("jobs")
            .into_iter()
            .filter(|j| j.kind == "follow_up")
            .count(),
        1
    );

    // Un-snoozing a thread that is not snoozed is a NotFound, not a panic.
    assert!(matches!(
        core.unsnooze_thread(retainer).await,
        Err(mail_core::error::CoreError::NotFound { .. })
    ));
}

#[tokio::test]
async fn undo_send_cancels_before_start_and_reports_too_late_after_delivery() {
    let mock = MockSmtp::start();
    let clock = Arc::new(ManualClock::new(UnixMs(START)));
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock,
        },
    )
    .expect("open core");

    let account = mock_account(&core, mock.port, "mock").await;
    let draft = simple_draft(&core, account.id, "Hello from the outbox").await;

    // --- Undoable send cancelled before its deadline ---
    let receipt = core
        .send_draft(draft.id, SendMode::Undoable { undo_secs: 60 })
        .await
        .expect("send");
    assert!(!receipt.scheduled);
    assert_eq!(receipt.due_at, UnixMs(START + 60_000));

    let outcome = core.cancel_send(receipt.job_id).await.expect("cancel");
    assert_eq!(outcome, UndoOutcome::Cancelled);
    let after_cancel = core.get_draft(draft.id).await.expect("draft");
    assert!(matches!(after_cancel.state, DraftState::Editing));

    // Cancelling twice is idempotent.
    assert_eq!(
        core.cancel_send(receipt.job_id)
            .await
            .expect("cancel again"),
        UndoOutcome::Cancelled
    );

    // Nothing was delivered: the tick leaves cancelled jobs alone.
    assert!(core.tick_scheduler().await.expect("tick").is_empty());

    // The draft is editable again and can be re-sent with no undo window.
    let resend = core
        .send_draft(draft.id, SendMode::Undoable { undo_secs: 0 })
        .await
        .expect("resend");
    assert_eq!(resend.due_at, UnixMs(START));

    // Due now: the tick delivers through the mock SMTP server.
    let fired = core.tick_scheduler().await.expect("tick send");
    assert!(fired
        .iter()
        .any(|j| j.id == resend.job_id && j.state == "sent"));
    assert!(
        core.list_jobs().await.expect("jobs").is_empty(),
        "sent jobs go inactive"
    );

    let sent_draft = core.get_draft(draft.id).await.expect("draft");
    assert!(matches!(sent_draft.state, DraftState::Sent { .. }));

    // Too late now: the outbox refuses to cancel delivered mail.
    assert!(matches!(
        core.cancel_send(resend.job_id).await,
        Err(mail_core::error::CoreError::TooLate)
    ));

    // The sent message shows up in Sent as its own new thread.
    let sent_view = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Sent,
            limit: 50,
            offset: 0,
        })
        .await
        .expect("sent");
    assert_eq!(sent_view.total, 1);
    assert!(sent_view.items[0].subject.contains("Hello from the outbox"));

    // A follow-up on that thread is satisfied by the next reply send.
    let sent_thread = sent_view.items[0].id;
    let follow = core
        .set_follow_up(sent_thread, UnixMs(START + 86_400_000))
        .await
        .expect("follow-up");
    assert_eq!(follow.thread_id, Some(sent_thread));

    let reply = core
        .create_draft(CreateDraftInput {
            account_id: account.id,
            reply_to_thread: Some(sent_thread),
            to: vec!["friend@example.test".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Hello from the outbox".into(),
            body_text: "Second delivery attempt.".into(),
            body_html: None,
            attachments: vec![],
        })
        .await
        .expect("reply draft");
    assert!(reply.from_locked, "replies lock the From identity");

    core.send_draft(reply.id, SendMode::Undoable { undo_secs: 0 })
        .await
        .expect("send reply");
    core.tick_scheduler().await.expect("tick reply");

    let page = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Sent,
            limit: 50,
            offset: 0,
        })
        .await
        .expect("sent again");
    assert_eq!(page.total, 1, "reply joins the same thread");
    assert_eq!(page.items[0].message_count, 2);
    assert_eq!(
        core.mailbox_counts(Scope::Unified)
            .await
            .expect("counts")
            .follow_ups,
        0,
        "sending the reply satisfies the follow-up"
    );
}

#[tokio::test]
async fn unreachable_smtp_backs_off_and_stays_cancellable() {
    let clock = Arc::new(ManualClock::new(UnixMs(START)));
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock,
        },
    )
    .expect("open core");

    // Port 1 on loopback refuses instantly: no listener, no external network.
    let account = mock_account(&core, 1, "offline").await;
    let draft = simple_draft(&core, account.id, "Will retry").await;
    let receipt = core
        .send_draft(draft.id, SendMode::Undoable { undo_secs: 0 })
        .await
        .expect("send");

    let fired = core.tick_scheduler().await.expect("tick");
    assert!(fired
        .iter()
        .any(|j| j.id == receipt.job_id && j.state == "retryWait"));

    // The draft stays queued, and the job is still cancellable.
    let draft = core.get_draft(draft.id).await.expect("draft");
    assert!(matches!(draft.state, DraftState::Queued { .. }));
    assert_eq!(
        core.cancel_send(receipt.job_id).await.expect("cancel"),
        UndoOutcome::Cancelled
    );
    let draft = core.get_draft(draft.id).await.expect("draft");
    assert!(matches!(draft.state, DraftState::Editing));
}

#[tokio::test]
async fn sync_account_surfaces_auth_and_network_errors_cleanly() {
    let clock = Arc::new(ManualClock::new(UnixMs(START)));
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock,
        },
    )
    .expect("open core");
    assert!(core.seed_demo_if_empty().await.expect("seed"));

    // Seeded accounts have no stored credential → AuthRequired + Blocked state.
    let accounts = core.list_accounts().await.expect("accounts");
    let studio = accounts
        .iter()
        .find(|a| a.display_name == "Studio Gmail")
        .expect("studio");
    let err = core.sync_account(studio.id).await.expect_err("no secret");
    assert!(matches!(
        err,
        mail_core::error::CoreError::AuthRequired { .. }
    ));
    let after = core
        .list_accounts()
        .await
        .expect("accounts")
        .into_iter()
        .find(|a| a.id == studio.id)
        .expect("studio");
    assert!(matches!(after.sync, SyncState::Blocked { .. }));

    // An unreachable IMAP endpoint surfaces Network without crashing.
    let offline = mock_account(&core, 1, "offline2").await;
    let err = core.sync_account(offline.id).await.expect_err("refused");
    assert!(matches!(
        err,
        mail_core::error::CoreError::Network {
            retryable: true,
            ..
        }
    ));
    let after = core
        .list_accounts()
        .await
        .expect("accounts")
        .into_iter()
        .find(|a| a.id == offline.id)
        .expect("offline");
    assert!(matches!(after.sync, SyncState::Backoff { .. }));

    // Deleting the credential flips the next sync to AuthRequired.
    core.secrets()
        .delete(&SecretKey {
            account_id: offline.id,
            kind: mail_core::auth::CredentialKind::Password,
        })
        .expect("delete");
    let err = core.sync_account(offline.id).await.expect_err("gone");
    assert!(matches!(
        err,
        mail_core::error::CoreError::AuthRequired { .. }
    ));
}
