use std::sync::Arc;

use mail_core::agent::{UNTRUSTED_BEGIN, UNTRUSTED_END};
use mail_core::auth::{MemorySecretStore, SecretKey, SecretStore};
use mail_core::error::CoreError;
use mail_core::events::HermesPushEvent;
use mail_core::host::ManualClock;
use mail_core::ids::{AgentId, UnixMs};
use mail_core::mcp::{
    format_sse_frame, handle_mcp_request_line, send_ipc_request_once, sign_webhook_payload,
    spawn_ipc_server, validate_localhost_webhook_url,
};
use mail_core::types::{
    DraftOrigin, DraftState, InboxCategory, MailboxView, SaveAgentAccountPermissionInput,
    SaveAgentBridgeConfigInput, SaveAiConfigInput, SaveLabelInput, Scope, SendMode, SpaceColor,
    ThreadQuery,
};
use mail_core::{CoreConfig, Host, MailCore};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn test_core() -> (MailCore, Arc<MemorySecretStore>) {
    let secrets = Arc::new(MemorySecretStore::default());
    let host = Host {
        secrets: secrets.clone(),
        clock: Arc::new(ManualClock::new(UnixMs(1_700_000_000_000))),
    };
    let core = MailCore::open(CoreConfig::default(), host).expect("open core");
    (core, secrets)
}

#[tokio::test]
async fn mcp_protocol_all_15_tools_and_single_writer_ipc_roundtrip() {
    let (core, _secrets) = test_core();
    core.seed_demo_if_empty().await.expect("seed demo");
    let accounts = core.list_accounts().await.expect("accounts");
    let first_acct = accounts[0].id;
    let threads = core
        .list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: first_acct,
            },
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 10,
            offset: 0,
        })
        .await
        .expect("threads")
        .items;
    assert!(!threads.is_empty());
    let thread_id = threads[0].id;
    let opened = core
        .open_thread(thread_id, mail_core::types::RemoteContent::BlockAll)
        .await
        .expect("open");
    let msg_id = opened.messages[0].id;

    let label = core
        .save_label(SaveLabelInput {
            id: None,
            name: "Hermes Triaged".to_string(),
            color: SpaceColor::Teal,
        })
        .await
        .expect("label");

    let port = core.agent_port(AgentId::parse("hermes").unwrap());

    // 1. initialize & tools/list
    let init_resp: Value = serde_json::from_str(
        &handle_mcp_request_line(
            &port,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}).to_string(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "ferrule");

    let list_resp: Value = serde_json::from_str(
        &handle_mcp_request_line(
            &port,
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let tool_names: Vec<&str> = list_resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for expected in [
        "list_accounts",
        "list_folders",
        "list_labels",
        "list_messages",
        "search_messages",
        "read_message",
        "read_conversation",
        "list_events",
        "create_draft",
        "update_draft",
        "mark_messages",
        "move_messages",
        "label_messages",
        "snooze_thread",
        "archive_thread",
    ] {
        assert!(
            tool_names.contains(&expected),
            "missing MCP tool {expected}"
        );
    }
    assert_eq!(tool_names.len(), 15);

    // 2. read_message & read_conversation have prompt-injection framing & no raw HTML
    let read_msg_resp: Value = serde_json::from_str(
        &handle_mcp_request_line(
            &port,
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": "read_message", "arguments": { "message_id": msg_id.to_string() } }
            })
            .to_string(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(read_msg_resp["result"]["isError"], false);
    let msg_text = read_msg_resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(msg_text.contains(UNTRUSTED_BEGIN));
    assert!(msg_text.contains(UNTRUSTED_END));
    assert!(!msg_text.contains("<html"));

    // 3. create_draft + update_draft stay strictly in PendingApproval
    let create_resp: Value = serde_json::from_str(
        &handle_mcp_request_line(
            &port,
            &json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "create_draft",
                    "arguments": {
                        "reply_to_thread": thread_id.to_string(),
                        "body_text": "Initial Hermes draft",
                        "rationale": "Polite acknowledgement"
                    }
                }
            })
            .to_string(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(create_resp["result"]["isError"], false);
    let draft_json: Value =
        serde_json::from_str(create_resp["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap();
    let draft_id_str = draft_json["id"].as_str().unwrap().to_string();
    assert_eq!(draft_json["state"]["kind"], "pendingApproval");
    assert_eq!(draft_json["origin"]["kind"], "agent");

    let update_resp: Value = serde_json::from_str(
        &handle_mcp_request_line(
            &port,
            &json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "update_draft",
                    "arguments": {
                        "draft_id": draft_id_str,
                        "body_text": "Refined Hermes draft",
                        "rationale": "Updated with concrete timing"
                    }
                }
            })
            .to_string(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(update_resp["result"]["isError"], false);
    let updated_draft: Value =
        serde_json::from_str(update_resp["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap();
    assert_eq!(updated_draft["bodyText"], "Refined Hermes draft");
    assert_eq!(updated_draft["state"]["kind"], "pendingApproval");

    // Pending draft appears in human approval queue
    let pending = core.list_pending_agent_drafts().await.expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].body_text, "Refined Hermes draft");

    // 4. Triage mutations: label_messages, mark_messages, snooze_thread, archive_thread
    for (id, name, args) in [
        (
            6,
            "label_messages",
            json!({ "label_id": label.id.to_string(), "thread_ids": [thread_id.to_string()], "add": true }),
        ),
        (
            7,
            "mark_messages",
            json!({ "thread_ids": [thread_id.to_string()], "starred": true, "read": false }),
        ),
        (
            8,
            "snooze_thread",
            json!({ "thread_id": thread_id.to_string(), "until": 1_700_000_900_000i64 }),
        ),
        (
            9,
            "archive_thread",
            json!({ "thread_id": thread_id.to_string() }),
        ),
    ] {
        let resp: Value = serde_json::from_str(
            &handle_mcp_request_line(
                &port,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": "tools/call",
                    "params": { "name": name, "arguments": args }
                })
                .to_string(),
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(resp["result"]["isError"], false, "tool {name} failed: {resp}");
    }

    // 5. Undo every undoable triage mutation in reverse chronological order
    let activity = core.list_agent_activity(20).await.expect("activity");
    let undoable_ids: Vec<_> = activity.iter().filter(|a| a.can_undo).map(|a| a.id).collect();
    assert_eq!(undoable_ids.len(), 4);
    for act_id in undoable_ids {
        let undone = core.undo_agent_activity(act_id).await.expect("undo");
        assert!(undone.undone);
        assert!(!undone.can_undo);
    }

    // Thread is back in Inbox and un-snoozed after undoing archive + snooze!
    let back_in_inbox = core
        .list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: first_acct,
            },
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 10,
            offset: 0,
        })
        .await
        .expect("inbox")
        .items;
    assert!(back_in_inbox.iter().any(|t| t.id == thread_id));

    // 6. Single-writer local IPC server roundtrip (named pipe on Windows / Unix socket on Unix)
    let unique = uuid::Uuid::now_v7();
    #[cfg(windows)]
    let endpoint = format!(r"\\.\pipe\ferrule-mcp-test-{unique}");
    #[cfg(not(windows))]
    let endpoint = format!("/tmp/ferrule-mcp-test-{unique}.sock");

    let handle = spawn_ipc_server(core.clone(), endpoint.clone());
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;

    let ipc_resp_raw = send_ipc_request_once(
        &endpoint,
        &json!({"jsonrpc":"2.0","id":99,"method":"tools/call","params":{"name":"list_accounts","arguments":{}}}).to_string(),
    )
    .await
    .expect("ipc roundtrip");
    let ipc_resp: Value = serde_json::from_str(&ipc_resp_raw).unwrap();
    assert_eq!(ipc_resp["id"], 99);
    assert_eq!(ipc_resp["result"]["isError"], false);
    handle.abort();
}

#[tokio::test]
async fn agent_permissions_enforce_account_and_folder_toggles() {
    let (core, _secrets) = test_core();
    core.seed_demo_if_empty().await.expect("seed demo");
    let accounts = core.list_accounts().await.expect("accounts");
    let acct_id = accounts[0].id;
    let threads = core
        .list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: acct_id,
            },
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 10,
            offset: 0,
        })
        .await
        .expect("threads")
        .items;
    let tid = threads[0].id;

    // Disable triage & drafts, block INBOX folder
    core.save_agent_permission(SaveAgentAccountPermissionInput {
        account_id: acct_id,
        enabled: true,
        allow_read: true,
        allow_triage: false,
        allow_drafts: false,
        blocked_folders: vec!["INBOX".to_string()],
    })
    .await
    .expect("save perm");

    let port = core.agent_port(AgentId::parse("hermes").unwrap());
    let read_err = port.read_thread_plain(tid).await.unwrap_err();
    assert!(matches!(read_err, CoreError::PermissionDenied { .. }));

    let triage_err = port.archive_thread(tid).await.unwrap_err();
    assert!(matches!(triage_err, CoreError::PermissionDenied { .. }));

    let draft_err = port
        .stage_draft(mail_core::agent::StageAgentDraft {
            thread_id: tid,
            body_text: "hello".to_string(),
            rationale: "test".to_string(),
        })
        .await
        .unwrap_err();
    assert!(matches!(draft_err, CoreError::PermissionDenied { .. }));

    // Disable account completely -> omitted from list_accounts
    core.save_agent_permission(SaveAgentAccountPermissionInput {
        account_id: acct_id,
        enabled: false,
        allow_read: false,
        allow_triage: false,
        allow_drafts: false,
        blocked_folders: vec![],
    })
    .await
    .expect("disable");
    let permitted = port.list_accounts().await.expect("list");
    assert!(!permitted.iter().any(|a| a.id == acct_id));
}

#[tokio::test]
async fn secrets_stay_strictly_in_secret_store_and_webhook_validates_localhost() {
    let (core, secrets) = test_core();

    // Reject non-localhost webhook URL
    let err = core
        .save_agent_bridge_config(SaveAgentBridgeConfigInput {
            mcp_enabled: true,
            sse_port: 48173,
            webhook_enabled: true,
            webhook_url: "https://evil.example.com/hook".to_string(),
            webhook_secret: Some("super-secret".to_string()),
            clear_webhook_secret: false,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::InvalidInput { .. }));

    // Accept localhost webhook URL & store secret strictly in SecretStore
    let saved = core
        .save_agent_bridge_config(SaveAgentBridgeConfigInput {
            mcp_enabled: true,
            sse_port: 48173,
            webhook_enabled: true,
            webhook_url: "http://127.0.0.1:9999/hooks/ferrule".to_string(),
            webhook_secret: Some("whsec_local_123".to_string()),
            clear_webhook_secret: false,
        })
        .await
        .expect("save bridge");
    assert!(saved.has_webhook_secret);
    assert_eq!(
        secrets
            .get(&SecretKey::webhook_secret())
            .unwrap()
            .unwrap()
            .expose(),
        "whsec_local_123"
    );

    // Verify HMAC signature & SSE frame formatting
    let sig = sign_webhook_payload("whsec_local_123", br#"{"event":"new_message"}"#);
    assert!(sig.starts_with("sha256="));
    assert_eq!(sig.len(), 7 + 64);
    assert!(validate_localhost_webhook_url("http://localhost:8080/hooks/ferrule").is_ok());
    assert!(validate_localhost_webhook_url("http://192.168.1.5:8080/hook").is_err());

    let ev = HermesPushEvent::NewMessage {
        account_id: mail_core::ids::AccountId::fresh(),
        thread_id: mail_core::ids::ThreadId::fresh(),
        message_id: mail_core::ids::MessageId::fresh(),
        from_email: "alice@example.com".to_string(),
        from_name: Some("Alice".to_string()),
        subject: "Ping".to_string(),
        date: UnixMs(1_700_000_000_000),
        folder: "INBOX".to_string(),
    };
    let frame = format_sse_frame(&ev).unwrap();
    assert!(frame.starts_with("event: new_message\ndata: "));
    assert!(frame.ends_with("\n\n"));

    // AI API key goes strictly to SecretStore
    let ai_cfg = core
        .save_ai_config(SaveAiConfigInput {
            enabled: true,
            base_url: "http://127.0.0.1:11434/v1".to_string(),
            model: "llama3.2".to_string(),
            api_key: Some("sk-local-test-key".to_string()),
            clear_api_key: false,
        })
        .await
        .expect("save ai");
    assert!(ai_cfg.has_api_key);
    assert_eq!(
        secrets
            .get(&SecretKey::ai_api_key())
            .unwrap()
            .unwrap()
            .expose(),
        "sk-local-test-key"
    );
}

#[tokio::test]
async fn in_pane_ai_summarize_ask_and_stage_draft_against_mock_openai_server() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock ai");
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        for i in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 8192];
            let _ = socket.read(&mut buf).await;
            let content = match i {
                0 => r#"{"summary":"Budget approved for Q3.","keyPoints":["$5k allocated"],"actionItems":["Send invoice"]}"#,
                1 => "The invoice is due next Friday.",
                _ => "Thanks! I will send the invoice over today.",
            };
            let body = json!({
                "choices": [{ "message": { "content": content } }]
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(resp.as_bytes()).await;
        }
    });

    let (core, _secrets) = test_core();
    core.seed_demo_if_empty().await.expect("seed");
    let threads = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 5,
            offset: 0,
        })
        .await
        .expect("threads")
        .items;
    let tid = threads[0].id;

    core.save_ai_config(SaveAiConfigInput {
        enabled: true,
        base_url: format!("http://{addr}/v1"),
        model: "test-model".to_string(),
        api_key: Some("sk-test".to_string()),
        clear_api_key: false,
    })
    .await
    .expect("save ai");

    let summary = core.ai_summarize_thread(tid).await.expect("summary");
    assert_eq!(summary.summary, "Budget approved for Q3.");
    assert_eq!(summary.key_points, vec!["$5k allocated"]);
    assert_eq!(summary.action_items, vec!["Send invoice"]);

    let answer = core
        .ai_ask_thread(
            tid,
            "When is the invoice due?".to_string(),
            Some("Invoice details".to_string()),
        )
        .await
        .expect("ask");
    assert_eq!(answer.answer, "The invoice is due next Friday.");

    let drafted = core
        .ai_draft_reply(tid, "Confirm we will send it today".to_string(), true)
        .await
        .expect("draft");
    assert_eq!(
        drafted.body_text,
        "Thanks! I will send the invoice over today."
    );
    let staged = drafted.staged_draft.expect("staged");
    assert!(matches!(staged.origin, DraftOrigin::Agent { .. }));
    assert!(matches!(staged.state, DraftState::PendingApproval));

    // Only human approval can queue it for send
    let receipt = core
        .approve_agent_draft(staged.id, SendMode::Undoable { undo_secs: 7 })
        .await
        .expect("approve");
    assert_eq!(receipt.draft_id, staged.id);

    let _ = server.await;
}
