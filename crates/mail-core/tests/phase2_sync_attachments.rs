//! Phase 2 integration tests: offline action queue, named signatures,
//! rich compose + draft attachments, attachment download/save, and async IMAP sync.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use base64::Engine as _;
use mail_core::auth::{Endpoint, MemorySecretStore, ProviderAuth};
use mail_core::host::ManualClock;
use mail_core::ids::UnixMs;
use mail_core::types::*;
use mail_core::{CoreConfig, Host, MailCore};

const START: i64 = 1_760_000_000_000;

async fn test_core() -> (MailCore, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock::new(UnixMs(START)));
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock: clock.clone(),
        },
    )
    .expect("open core");
    (core, clock)
}

#[tokio::test]
async fn signatures_crud_and_single_default_per_account() {
    let (core, _) = test_core().await;
    core.seed_demo_if_empty().await.expect("seed");
    let accounts = core.list_accounts().await.expect("accounts");
    let acct_a = accounts[0].id;
    let acct_b = accounts[1].id;

    let sig1 = core
        .save_signature(SaveSignatureInput {
            id: None,
            account_id: acct_a,
            name: "Formal".into(),
            body_html: "<p>Best,<br><strong>Reyth</strong></p>".into(),
            body_text: "Best,\nReyth".into(),
            is_default: true,
        })
        .await
        .expect("save sig1");
    assert!(sig1.is_default);

    let sig2 = core
        .save_signature(SaveSignatureInput {
            id: None,
            account_id: acct_a,
            name: "Casual".into(),
            body_html: "<p>Cheers!</p>".into(),
            body_text: "Cheers!".into(),
            is_default: true,
        })
        .await
        .expect("save sig2");
    assert!(sig2.is_default);

    // Saving a second default for acct_a must clear is_default on sig1.
    let list_a = core.list_signatures(Some(acct_a)).await.expect("list a");
    assert_eq!(list_a.len(), 2);
    assert_eq!(list_a.iter().filter(|s| s.is_default).count(), 1);
    assert_eq!(list_a[0].id, sig2.id);

    // Other account stays isolated.
    let list_b = core.list_signatures(Some(acct_b)).await.expect("list b");
    assert!(list_b.is_empty());

    core.delete_signature(sig1.id).await.expect("delete sig1");
    let after = core.list_signatures(Some(acct_a)).await.expect("list after");
    assert_eq!(after.len(), 1);
}

#[tokio::test]
async fn offline_action_queue_enqueues_on_local_mutations() {
    let (core, _) = test_core().await;
    core.seed_demo_if_empty().await.expect("seed");
    let page = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 10,
            offset: 0,
        })
        .await
        .expect("list threads");
    let thread = &page.items[0];

    let before = core
        .pending_offline_actions(thread.account_id)
        .await
        .expect("pending before");
    assert_eq!(before, 0);

    core.act_on_threads(MailAction::Star { on: true }, vec![thread.id])
        .await
        .expect("star");
    core.act_on_threads(MailAction::Archive, vec![thread.id])
        .await
        .expect("archive");

    let after = core
        .pending_offline_actions(thread.account_id)
        .await
        .expect("pending after");
    assert_eq!(after, 2);
}

fn start_mock_smtp() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind smtp");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = serve_smtp(stream);
        }
    });
    port
}

fn serve_smtp(stream: TcpStream) -> std::io::Result<()> {
    let mut out = stream.try_clone()?;
    let mut input = BufReader::new(stream);
    write!(out, "220 mock.localhost ESMTP ready\r\n")?;
    let mut buf = String::new();
    while input.read_line(&mut buf)? > 0 {
        let line = buf.trim_end().to_string();
        buf.clear();
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("EHLO") {
            write!(out, "250-mock.localhost\r\n250 AUTH PLAIN\r\n")?;
        } else if upper.starts_with("AUTH PLAIN") {
            if line.split_whitespace().count() >= 3 {
                write!(out, "235 ok\r\n")?;
            } else {
                write!(out, "334\r\n")?;
                input.read_line(&mut buf)?;
                buf.clear();
                write!(out, "235 ok\r\n")?;
            }
        } else if upper.starts_with("DATA") {
            write!(out, "354 go ahead\r\n")?;
            loop {
                buf.clear();
                if input.read_line(&mut buf)? == 0 || buf.trim_end() == "." {
                    break;
                }
            }
            buf.clear();
            write!(out, "250 ok\r\n")?;
        } else if upper.starts_with("QUIT") {
            write!(out, "221 bye\r\n")?;
            return Ok(());
        } else {
            write!(out, "250 ok\r\n")?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn rich_compose_with_attachments_sends_and_persists_downloadable_attachments() {
    let smtp_port = start_mock_smtp();
    let (core, clock) = test_core().await;

    let acct = core
        .add_account(NewAccountInput {
            display_name: "Local Test".into(),
            color: SpaceColor::Teal,
            auth: ProviderAuth::ImapPassword {
                address: "me@local.test".into(),
                username: "me@local.test".into(),
                imap: Endpoint {
                    host: "127.0.0.1".into(),
                    port: 1993,
                    tls: false,
                },
                smtp: Endpoint {
                    host: "127.0.0.1".into(),
                    port: smtp_port,
                    tls: false,
                },
            },
            secret: Some("pass123".into()),
        })
        .await
        .expect("add account");

    let payload_bytes = b"Quarterly report contents";
    let draft = core
        .create_draft(CreateDraftInput {
            account_id: acct.id,
            reply_to_thread: None,
            to: vec!["friend@example.test".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Rich report".into(),
            body_text: "See attached.".into(),
            body_html: Some("<p>See <strong>attached</strong>.</p>".into()),
            attachments: vec![DraftAttachmentInput {
                filename: "report.txt".into(),
                mime: "text/plain".into(),
                data_base64: base64::engine::general_purpose::STANDARD.encode(payload_bytes),
            }],
        })
        .await
        .expect("create draft");
    assert_eq!(draft.attachments.len(), 1);
    assert_eq!(draft.attachments[0].filename, "report.txt");

    core.send_draft(draft.id, SendMode::Undoable { undo_secs: 0 })
        .await
        .expect("send draft");
    clock.advance_secs(1);
    core.tick_scheduler().await.expect("tick");

    let sent = core
        .list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: acct.id,
            },
            view: MailboxView::Sent,
            limit: 10,
            offset: 0,
        })
        .await
        .expect("sent list");
    assert_eq!(sent.items.len(), 1);
    assert!(sent.items[0].has_attachments);

    let view = core
        .open_thread(sent.items[0].id, RemoteContent::BlockAll)
        .await
        .expect("open thread");
    assert_eq!(view.messages.len(), 1);
    assert!(view.messages[0].body_srcdoc.contains("<strong>attached</strong>"));
    assert_eq!(view.messages[0].attachments.len(), 1);

    let att_id = view.messages[0].attachments[0].id;
    let downloaded = core
        .download_attachment(att_id)
        .await
        .expect("download attachment");
    assert_eq!(downloaded.filename, "report.txt");
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&downloaded.data_base64)
        .expect("decode");
    assert_eq!(decoded, payload_bytes);
}
