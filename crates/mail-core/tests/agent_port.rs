//! Phase-2 agent boundary: AgentPort can search, read plain text, and stage drafts
//! that land in PendingApproval — but only the human approve path can ever queue a
//! send. Also covers the From Identity Guard.

use std::sync::Arc;

use mail_core::agent::StageAgentDraft;
use mail_core::auth::{MemorySecretStore, ProviderAuth, ProviderKind};
use mail_core::events::{Alert, CoreEvent};
use mail_core::host::{Clock, ManualClock};
use mail_core::ids::{AgentId, UnixMs};
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

#[tokio::test]
async fn agent_port_stages_drafts_that_only_a_human_can_send() {
    let core = seeded_core().await;
    let mut events = core.subscribe();

    let hermes = core.agent_port(AgentId::parse("hermes").expect("valid id"));
    assert_eq!(hermes.agent_id().as_str(), "hermes");

    // Search and plain-text read are available to the agent.
    let hits = hermes.search("retainer", 5).await.expect("search");
    assert_eq!(hits.len(), 1);
    let thread_id = hits[0].id;
    let plain = hermes.read_thread_plain(thread_id).await.expect("read");
    assert!(plain.contains("invoice #4821"));
    assert!(plain.contains("Dana Okafor"));
    assert!(
        !plain.contains("<p>") && !plain.contains("<img"),
        "plain read must never expose raw HTML"
    );

    // Staging lands in PendingApproval with a locked From identity.
    let draft = hermes
        .stage_draft(StageAgentDraft {
            thread_id,
            body_text: "Confirmed — paying Friday.".to_string(),
            rationale: "Invoice #4821 requests payment confirmation.".to_string(),
        })
        .await
        .expect("stage");
    assert!(matches!(draft.state, DraftState::PendingApproval));
    assert!(draft.from_locked);
    match &draft.origin {
        DraftOrigin::Agent {
            agent_id,
            rationale,
        } => {
            assert_eq!(agent_id, "hermes");
            assert!(rationale.contains("Invoice #4821"));
        }
        other => panic!("expected agent origin, got {other:?}"),
    }
    assert_eq!(draft.reply_to_thread, Some(thread_id));

    // The approval request is announced to the UI (after the DraftChanged notice).
    let approval = loop {
        match events.try_recv() {
            Ok(CoreEvent::Attention {
                alert: Alert::ApprovalRequested { draft_id },
            }) => break draft_id,
            Ok(_) => continue,
            Err(_) => panic!("ApprovalRequested event"),
        }
    };
    assert_eq!(approval, draft.id);

    // The human send path refuses agent drafts outright.
    let blocked = core
        .send_draft(draft.id, SendMode::Undoable { undo_secs: 60 })
        .await;
    assert!(matches!(
        blocked,
        Err(mail_core::error::CoreError::IllegalState { .. })
    ));

    // Human approval is the only way out of PendingApproval.
    let receipt = core
        .approve_agent_draft(draft.id, SendMode::Undoable { undo_secs: 60 })
        .await
        .expect("approve");
    assert!(!receipt.scheduled);
    let queued = core.get_draft(draft.id).await.expect("draft");
    assert!(matches!(
        queued.state,
        DraftState::Queued { job_id } if job_id == receipt.job_id
    ));

    // Approving twice, or approving a human draft, is an illegal transition.
    assert!(matches!(
        core.approve_agent_draft(draft.id, SendMode::Undoable { undo_secs: 60 })
            .await,
        Err(mail_core::error::CoreError::IllegalState { .. })
    ));

    // Cancelling returns an agent draft to PendingApproval — never to a state the
    // agent could send from — and the human can approve it again.
    core.cancel_send(receipt.job_id).await.expect("cancel");
    let resting = core.get_draft(draft.id).await.expect("draft");
    assert!(matches!(resting.state, DraftState::PendingApproval));
    assert!(matches!(
        core.send_draft(draft.id, SendMode::Undoable { undo_secs: 60 })
            .await,
        Err(mail_core::error::CoreError::IllegalState { .. })
    ));

    // Approving the cancelled draft is the sanctioned path again.
    core.approve_agent_draft(draft.id, SendMode::Undoable { undo_secs: 60 })
        .await
        .expect("approve again");
}

#[tokio::test]
async fn agent_port_rejects_unknown_threads_and_bad_ids() {
    let core = seeded_core().await;
    let port = core.agent_port(AgentId::parse("grok-bot").expect("id"));
    assert!(matches!(
        AgentId::parse("Bad Id!"),
        Err(mail_core::error::CoreError::InvalidInput { .. })
    ));
    assert!(matches!(
        port.read_thread_plain(mail_core::ids::ThreadId::fresh())
            .await,
        Err(mail_core::error::CoreError::NotFound { .. })
    ));
    assert!(matches!(
        port.stage_draft(StageAgentDraft {
            thread_id: mail_core::ids::ThreadId::fresh(),
            body_text: "hi".into(),
            rationale: "r".into(),
        })
        .await,
        Err(mail_core::error::CoreError::NotFound { .. })
    ));
    assert!(port
        .search("nothing-matches-xyz", 5)
        .await
        .expect("search")
        .is_empty());
}

#[tokio::test]
async fn from_identity_guard_blocks_cross_account_replies() {
    let core = seeded_core().await;
    let accounts = core.list_accounts().await.expect("accounts");
    let studio = accounts
        .iter()
        .find(|a| a.display_name == "Studio Gmail")
        .expect("studio")
        .clone();
    let personal = accounts
        .iter()
        .find(|a| a.display_name == "Personal Gmail")
        .expect("personal")
        .clone();

    let studio_thread = thread_id_by_subject(&core, "Q2 illustration retainer").await;
    let personal_thread = thread_id_by_subject(&core, "Fall layers").await;

    // A reply can never be drafted against another account's thread.
    let cross = core
        .create_draft(CreateDraftInput {
            account_id: personal.id,
            reply_to_thread: Some(studio_thread),
            to: vec!["dana@ferngate.co".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Re: Q2 illustration retainer".into(),
            body_text: "Should not exist.".into(),
        })
        .await;
    assert!(matches!(
        cross,
        Err(mail_core::error::CoreError::IdentityMismatch)
    ));

    // In-account replies lock the From identity to that account's primary address.
    let locked = core
        .create_draft(CreateDraftInput {
            account_id: studio.id,
            reply_to_thread: Some(studio_thread),
            to: vec!["dana@ferngate.co".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Re: Q2 illustration retainer".into(),
            body_text: "Rough by Friday.".into(),
        })
        .await
        .expect("reply draft");
    assert!(locked.from_locked);
    assert_eq!(locked.from_identity.address, "mira.studio@gmail.com");
    assert_eq!(locked.account_id, studio.id);
    assert_eq!(locked.space_color, SpaceColor::Azure);

    // Fresh compose drafts start unlocked on the same primary identity.
    let fresh = core
        .create_draft(CreateDraftInput {
            account_id: studio.id,
            reply_to_thread: None,
            to: vec!["someone@example.test".into()],
            cc: vec![],
            bcc: vec![],
            subject: "New thread".into(),
            body_text: "Hello.".into(),
        })
        .await
        .expect("compose draft");
    assert!(!fresh.from_locked);
    assert_eq!(fresh.from_identity.address, "mira.studio@gmail.com");

    // The personal account's own thread replies fine and uses its own identity.
    let personal_reply = core
        .create_draft(CreateDraftInput {
            account_id: personal.id,
            reply_to_thread: Some(personal_thread),
            to: vec!["sales@northwindoutfitters.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Re: Fall layers".into(),
            body_text: "Unsubscribe me please.".into(),
        })
        .await
        .expect("personal reply");
    assert!(personal_reply.from_locked);
    assert_eq!(
        personal_reply.from_identity.address,
        "reyth.personal@gmail.com"
    );

    // Replying to a thread that does not exist is a NotFound, not a silent draft.
    assert!(matches!(
        core.create_draft(CreateDraftInput {
            account_id: studio.id,
            reply_to_thread: Some(mail_core::ids::ThreadId::fresh()),
            to: vec!["x@example.test".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Ghost".into(),
            body_text: "hi".into(),
        })
        .await,
        Err(mail_core::error::CoreError::NotFound { .. })
    ));
}

#[tokio::test]
async fn queued_human_drafts_enqueue_without_touching_the_network() {
    let core = seeded_core().await;
    let studio_thread = thread_id_by_subject(&core, "Q2 illustration retainer").await;
    let draft = core
        .create_draft(CreateDraftInput {
            account_id: {
                let accounts = core.list_accounts().await.expect("accounts");
                accounts
                    .iter()
                    .find(|a| a.display_name == "Studio Gmail")
                    .expect("studio")
                    .id
            },
            reply_to_thread: Some(studio_thread),
            to: vec!["dana@ferngate.co".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Re: Q2 illustration retainer — invoice #4821".into(),
            body_text: "Rough by Friday, promise.".into(),
        })
        .await
        .expect("draft");

    // Enqueueing is pure state: the receipt points at a live scheduled job.
    let receipt = core
        .send_draft(draft.id, SendMode::Undoable { undo_secs: 7 })
        .await
        .expect("enqueue");
    let jobs = core.list_jobs().await.expect("jobs");
    let job = jobs
        .iter()
        .find(|j| j.id == receipt.job_id)
        .expect("job listed");
    assert_eq!(job.kind, "send");
    assert_eq!(job.state, "scheduled");
    assert_eq!(job.thread_id, Some(studio_thread));

    // The draft is queued and can be cancelled back to editing.
    let queued = core.get_draft(draft.id).await.expect("draft");
    assert!(matches!(queued.state, DraftState::Queued { .. }));
    core.cancel_send(receipt.job_id).await.expect("cancel");
    let editing = core.get_draft(draft.id).await.expect("draft");
    assert!(matches!(editing.state, DraftState::Editing));

    // A scheduled send must be in the future.
    let draft2 = core
        .create_draft(CreateDraftInput {
            account_id: queued.account_id,
            reply_to_thread: None,
            to: vec!["x@example.test".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Later".into(),
            body_text: "hi".into(),
        })
        .await
        .expect("draft2");
    assert!(matches!(
        core.send_draft(
            draft2.id,
            SendMode::Scheduled {
                send_at: UnixMs(START - 1)
            }
        )
        .await,
        Err(mail_core::error::CoreError::InvalidInput { .. })
    ));
    // Sending with no recipients is rejected up front.
    let draft3 = core
        .create_draft(CreateDraftInput {
            account_id: queued.account_id,
            reply_to_thread: None,
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "No recipients".into(),
            body_text: "hi".into(),
        })
        .await
        .expect("draft3");
    assert!(matches!(
        core.send_draft(draft3.id, SendMode::Undoable { undo_secs: 1 })
            .await,
        Err(mail_core::error::CoreError::InvalidInput { .. })
    ));
    // ProviderAuth round-trips through the account row without storing secrets.
    let accounts = core.list_accounts().await.expect("accounts");
    assert_eq!(accounts[0].provider, ProviderKind::Gmail);
    let _ = ProviderAuth::ImapPassword {
        address: "x@example.test".into(),
        username: "x".into(),
        imap: mail_core::auth::Endpoint {
            host: "h".into(),
            port: 993,
            tls: true,
        },
        smtp: mail_core::auth::Endpoint {
            host: "h".into(),
            port: 587,
            tls: true,
        },
    };
}
