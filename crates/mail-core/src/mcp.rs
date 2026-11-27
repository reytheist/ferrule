//! Single-writer Hermes MCP bridge, local IPC server/client, SSE event stream,
//! and optional localhost webhook push dispatcher.
//!
//! Architecture:
//! - The running Ferrule desktop app owns SQLite + IMAP/SMTP sync actors and starts
//!   a user-only local IPC server (Windows Named Pipe or Unix Domain Socket) plus
//!   a localhost-only SSE push server (`127.0.0.1:<port>/events`).
//! - `ferrule mcp` (and `ferrule-mcp`) is a thin stdio bridge speaking MCP JSON-RPC 2.0
//!   over stdin/stdout and forwarding JSON-line requests to the running desktop app's
//!   local pipe/socket.

use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::agent::{
    AgentCreateDraftInput, AgentMoveDestination, AgentPort, AgentUpdateDraftInput,
};
use crate::auth::SecretKey;
use crate::error::CoreError;
use crate::events::HermesPushEvent;
use crate::ids::{AccountId, AgentId, DraftId, LabelId, MessageId, ThreadId, UnixMs};
use crate::MailCore;

pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Return the default user-scoped local IPC endpoint for the running Ferrule app.
/// Can be overridden via `FERRULE_MCP_ENDPOINT` (useful for tests and custom setups).
pub fn default_ipc_endpoint() -> String {
    if let Ok(custom) = std::env::var("FERRULE_MCP_ENDPOINT") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".to_string());
    let safe_user: String = user
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let suffix = if safe_user.is_empty() {
        "user".to_string()
    } else {
        safe_user
    };

    #[cfg(windows)]
    {
        format!(r"\\.\pipe\ferrule-mcp-{suffix}")
    }
    #[cfg(not(windows))]
    {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let trimmed = runtime_dir.trim();
            if !trimmed.is_empty() {
                return format!("{trimmed}/ferrule-mcp.sock");
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let trimmed = home.trim();
            if !trimmed.is_empty() {
                return format!("{trimmed}/.local/share/ferrule/ferrule-mcp.sock");
            }
        }
        format!("/run/user/ferrule-mcp-{suffix}.sock")
    }
}

/// Validate that a webhook URL strictly targets localhost so mail metadata can
/// never be exfiltrated to remote hosts.
pub fn validate_localhost_webhook_url(raw: &str) -> Result<String, CoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let parsed = url::Url::parse(trimmed).map_err(|e| CoreError::InvalidInput {
        field: "webhookUrl".to_string(),
        reason: format!("invalid webhook URL: {e}"),
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(CoreError::InvalidInput {
            field: "webhookUrl".to_string(),
            reason: "webhook URL must use http:// or https://".to_string(),
        });
    }
    let host = parsed.host_str().unwrap_or("");
    let is_local = host == "127.0.0.1" || host == "localhost" || host == "::1" || host == "[::1]";
    if !is_local {
        return Err(CoreError::InvalidInput {
            field: "webhookUrl".to_string(),
            reason: "webhook URL must target localhost (127.0.0.1, localhost, or ::1)".to_string(),
        });
    }
    Ok(trimmed.to_string())
}

/// Compute `sha256=<hex>` HMAC-SHA256 signature for webhook payloads.
pub fn sign_webhook_payload(secret: &str, body: &[u8]) -> String {
    const BLOCK_SIZE: usize = 64;
    let key_bytes = secret.as_bytes();
    let mut key_block = [0u8; BLOCK_SIZE];
    if key_bytes.len() > BLOCK_SIZE {
        let hashed = Sha256::digest(key_bytes);
        key_block[..32].copy_from_slice(&hashed);
    } else {
        key_block[..key_bytes.len()].copy_from_slice(key_bytes);
    }

    let mut o_key_pad = [0x5cu8; BLOCK_SIZE];
    let mut i_key_pad = [0x36u8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        o_key_pad[i] ^= key_block[i];
        i_key_pad[i] ^= key_block[i];
    }

    let mut inner = Sha256::new();
    inner.update(i_key_pad);
    inner.update(body);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(o_key_pad);
    outer.update(inner_hash);
    let mac = outer.finalize();

    let mut hex = String::with_capacity(7 + 64);
    hex.push_str("sha256=");
    for b in mac {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{b:02x}");
    }
    hex
}

// ── MCP JSON-RPC 2.0 Dispatcher ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

fn rpc_ok(id: Value, result: Value) -> String {
    serde_json::to_string(&JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    })
    .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"serialize error"}}"#.to_string())
}

fn rpc_err(id: Value, code: i32, message: impl Into<String>) -> String {
    serde_json::to_string(&JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
        }),
    })
    .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"serialize error"}}"#.to_string())
}

fn tool_result_json(value: &impl Serialize) -> Value {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    })
}

fn tool_result_text(text: String) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    })
}

fn tool_error(err: CoreError) -> Value {
    json!({
        "content": [{ "type": "text", "text": err.to_string() }],
        "isError": true
    })
}

pub fn mcp_tool_definitions() -> Value {
    json!({
        "tools": [
            {
                "name": "list_accounts",
                "description": "List email accounts enabled for agent access.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
            },
            {
                "name": "list_folders",
                "description": "List mailbox folders for a permitted account (or all permitted accounts).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "account_id": { "type": "string", "description": "Optional account UUID" }
                    }
                }
            },
            {
                "name": "list_labels",
                "description": "List user-defined mail labels.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
            },
            {
                "name": "list_messages",
                "description": "List recent threads/messages in a permitted account and folder/view.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "account_id": { "type": "string" },
                        "folder": { "type": "string", "description": "inbox, archive, sent, drafts, starred, snoozed, trash, spam, primary, receipts, newsletters" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 50 },
                        "offset": { "type": "integer", "minimum": 0 }
                    }
                }
            },
            {
                "name": "search_messages",
                "description": "Search messages using Gmail-syntax operators (from:, to:, in:, is:, has:, before:, after:, OR, -negation) and FTS5.",
                "inputSchema": {
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": { "type": "string" },
                        "account_id": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                    }
                }
            },
            {
                "name": "read_message",
                "description": "Read a single message as plain text only (never raw HTML), wrapped in prompt-injection framing.",
                "inputSchema": {
                    "type": "object",
                    "required": ["message_id"],
                    "properties": {
                        "message_id": { "type": "string" }
                    }
                }
            },
            {
                "name": "read_conversation",
                "description": "Read an entire thread as plain text only (never raw HTML), wrapped in prompt-injection framing.",
                "inputSchema": {
                    "type": "object",
                    "required": ["thread_id"],
                    "properties": {
                        "thread_id": { "type": "string" }
                    }
                }
            },
            {
                "name": "list_events",
                "description": "List personal calendar events within a Unix millisecond range.",
                "inputSchema": {
                    "type": "object",
                    "required": ["range_start", "range_end"],
                    "properties": {
                        "range_start": { "type": "integer", "description": "Start timestamp in Unix milliseconds" },
                        "range_end": { "type": "integer", "description": "End timestamp in Unix milliseconds" }
                    }
                }
            },
            {
                "name": "create_draft",
                "description": "Stage a new email or reply draft in PendingApproval with rationale. Agents can never send mail without human approval.",
                "inputSchema": {
                    "type": "object",
                    "required": ["body_text", "rationale"],
                    "properties": {
                        "account_id": { "type": "string" },
                        "reply_to_thread": { "type": "string" },
                        "to": { "type": "array", "items": { "type": "string" } },
                        "cc": { "type": "array", "items": { "type": "string" } },
                        "subject": { "type": "string" },
                        "body_text": { "type": "string" },
                        "rationale": { "type": "string" }
                    }
                }
            },
            {
                "name": "update_draft",
                "description": "Update an existing agent-staged PendingApproval draft.",
                "inputSchema": {
                    "type": "object",
                    "required": ["draft_id"],
                    "properties": {
                        "draft_id": { "type": "string" },
                        "to": { "type": "array", "items": { "type": "string" } },
                        "cc": { "type": "array", "items": { "type": "string" } },
                        "subject": { "type": "string" },
                        "body_text": { "type": "string" },
                        "rationale": { "type": "string" }
                    }
                }
            },
            {
                "name": "mark_messages",
                "description": "Mark threads or messages as read/unread and/or starred/unstarred (logged with 1-click undo).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "thread_ids": { "type": "array", "items": { "type": "string" } },
                        "message_ids": { "type": "array", "items": { "type": "string" } },
                        "read": { "type": "boolean" },
                        "starred": { "type": "boolean" }
                    }
                }
            },
            {
                "name": "move_messages",
                "description": "Move threads or messages to inbox, archive, trash, or spam (logged with 1-click undo).",
                "inputSchema": {
                    "type": "object",
                    "required": ["destination"],
                    "properties": {
                        "thread_ids": { "type": "array", "items": { "type": "string" } },
                        "message_ids": { "type": "array", "items": { "type": "string" } },
                        "destination": { "type": "string", "enum": ["inbox", "archive", "trash", "spam"] }
                    }
                }
            },
            {
                "name": "label_messages",
                "description": "Add or remove a label on threads or messages (logged with 1-click undo).",
                "inputSchema": {
                    "type": "object",
                    "required": ["label_id"],
                    "properties": {
                        "label_id": { "type": "string" },
                        "thread_ids": { "type": "array", "items": { "type": "string" } },
                        "message_ids": { "type": "array", "items": { "type": "string" } },
                        "add": { "type": "boolean", "default": true }
                    }
                }
            },
            {
                "name": "snooze_thread",
                "description": "Snooze a thread until a future Unix millisecond timestamp (logged with 1-click undo).",
                "inputSchema": {
                    "type": "object",
                    "required": ["thread_id", "until"],
                    "properties": {
                        "thread_id": { "type": "string" },
                        "until": { "type": "integer", "description": "Wake timestamp in Unix milliseconds" }
                    }
                }
            },
            {
                "name": "archive_thread",
                "description": "Archive a single thread (logged with 1-click undo).",
                "inputSchema": {
                    "type": "object",
                    "required": ["thread_id"],
                    "properties": {
                        "thread_id": { "type": "string" }
                    }
                }
            }
        ]
    })
}

fn parse_opt_id<T: FromStr<Err = CoreError>>(args: &Value, key: &str) -> Result<Option<T>, CoreError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(T::from_str(s.trim())?)),
        Some(_) => Err(CoreError::InvalidInput {
            field: key.to_string(),
            reason: "expected UUID string".to_string(),
        }),
    }
}

fn parse_req_id<T: FromStr<Err = CoreError>>(args: &Value, key: &str) -> Result<T, CoreError> {
    parse_opt_id(args, key)?.ok_or_else(|| CoreError::InvalidInput {
        field: key.to_string(),
        reason: "required UUID string".to_string(),
    })
}

fn parse_id_list<T: FromStr<Err = CoreError>>(args: &Value, key: &str) -> Result<Vec<T>, CoreError> {
    let Some(val) = args.get(key) else {
        return Ok(Vec::new());
    };
    let Some(arr) = val.as_array() else {
        return Err(CoreError::InvalidInput {
            field: key.to_string(),
            reason: "expected array of UUID strings".to_string(),
        });
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let s = item.as_str().ok_or_else(|| CoreError::InvalidInput {
            field: key.to_string(),
            reason: "expected UUID string element".to_string(),
        })?;
        out.push(T::from_str(s.trim())?);
    }
    Ok(out)
}

fn parse_str_list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Handle a single MCP JSON-RPC 2.0 request line against an [`AgentPort`].
/// Returns `None` for notifications (requests with no `id`).
pub async fn handle_mcp_request_line(port: &AgentPort, line: &str) -> Option<String> {
    let req: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return Some(rpc_err(Value::Null, -32700, format!("parse error: {e}")));
        }
    };

    // Notifications have no `id` and expect no response.
    let id = req.id?;

    match req.method.as_str() {
        "initialize" => Some(rpc_ok(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "ferrule",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )),
        "ping" => Some(rpc_ok(id, json!({}))),
        "tools/list" => Some(rpc_ok(id, mcp_tool_definitions())),
        "tools/call" => {
            let name = req
                .params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = dispatch_tool(port, name, &args).await;
            Some(rpc_ok(id, result))
        }
        other => Some(rpc_err(id, -32601, format!("method not found: {other}"))),
    }
}

async fn dispatch_tool(port: &AgentPort, name: &str, args: &Value) -> Value {
    let outcome: Result<Value, CoreError> = match name {
        "list_accounts" => port.list_accounts().await.map(|v| tool_result_json(&v)),
        "list_folders" => match parse_opt_id::<AccountId>(args, "account_id") {
            Ok(account_id) => port
                .list_folders(account_id)
                .await
                .map(|v| tool_result_json(&v)),
            Err(e) => Err(e),
        },
        "list_labels" => port.list_labels().await.map(|v| tool_result_json(&v)),
        "list_messages" => {
            let account_id = match parse_opt_id::<AccountId>(args, "account_id") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let folder = args.get("folder").and_then(Value::as_str);
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as u32;
            let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as u32;
            port.list_messages(account_id, folder, limit, offset)
                .await
                .map(|v| tool_result_json(&v))
        }
        "search_messages" => {
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let account_id = match parse_opt_id::<AccountId>(args, "account_id") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as u32;
            port.search_messages(query, account_id, limit)
                .await
                .map(|v| tool_result_json(&v))
        }
        "read_message" => match parse_req_id::<MessageId>(args, "message_id") {
            Ok(mid) => port.read_message(mid).await.map(tool_result_text),
            Err(e) => Err(e),
        },
        "read_conversation" => match parse_req_id::<ThreadId>(args, "thread_id") {
            Ok(tid) => port.read_thread_plain(tid).await.map(tool_result_text),
            Err(e) => Err(e),
        },
        "list_events" => {
            let start = args.get("range_start").and_then(Value::as_i64);
            let end = args.get("range_end").and_then(Value::as_i64);
            match (start, end) {
                (Some(s), Some(e)) => port
                    .list_events(UnixMs(s), UnixMs(e))
                    .await
                    .map(|v| tool_result_json(&v)),
                _ => Err(CoreError::InvalidInput {
                    field: "range_start/range_end".to_string(),
                    reason: "range_start and range_end Unix milliseconds are required".to_string(),
                }),
            }
        }
        "create_draft" => {
            let account_id = match parse_opt_id::<AccountId>(args, "account_id") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let reply_to_thread = match parse_opt_id::<ThreadId>(args, "reply_to_thread") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let body_text = args
                .get("body_text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let rationale = args
                .get("rationale")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let subject = args
                .get("subject")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let input = AgentCreateDraftInput {
                account_id,
                reply_to_thread,
                to: parse_str_list(args, "to"),
                cc: parse_str_list(args, "cc"),
                subject,
                body_text,
                rationale,
            };
            port.create_draft(input).await.map(|v| tool_result_json(&v))
        }
        "update_draft" => match parse_req_id::<DraftId>(args, "draft_id") {
            Ok(draft_id) => {
                let input = AgentUpdateDraftInput {
                    draft_id,
                    to: args.get("to").map(|_| parse_str_list(args, "to")),
                    cc: args.get("cc").map(|_| parse_str_list(args, "cc")),
                    subject: args
                        .get("subject")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    body_text: args
                        .get("body_text")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    rationale: args
                        .get("rationale")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                };
                port.update_draft(input).await.map(|v| tool_result_json(&v))
            }
            Err(e) => Err(e),
        },
        "mark_messages" => {
            let thread_ids = match parse_id_list::<ThreadId>(args, "thread_ids") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let message_ids = match parse_id_list::<MessageId>(args, "message_ids") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let read = args.get("read").and_then(Value::as_bool);
            let starred = args.get("starred").and_then(Value::as_bool);
            port.mark_messages(thread_ids, message_ids, read, starred)
                .await
                .map(|ids| tool_result_json(&json!({ "updatedThreadIds": ids })))
        }
        "move_messages" => {
            let thread_ids = match parse_id_list::<ThreadId>(args, "thread_ids") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let message_ids = match parse_id_list::<MessageId>(args, "message_ids") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let dest = match args
                .get("destination")
                .and_then(Value::as_str)
                .map(|s| s.to_ascii_lowercase())
                .as_deref()
            {
                Some("inbox") => AgentMoveDestination::Inbox,
                Some("archive") => AgentMoveDestination::Archive,
                Some("trash") => AgentMoveDestination::Trash,
                Some("spam") => AgentMoveDestination::Spam,
                _ => {
                    return tool_error(CoreError::InvalidInput {
                        field: "destination".to_string(),
                        reason: "destination must be one of inbox, archive, trash, spam"
                            .to_string(),
                    })
                }
            };
            port.move_messages(thread_ids, message_ids, dest)
                .await
                .map(|ids| tool_result_json(&json!({ "updatedThreadIds": ids })))
        }
        "label_messages" => {
            let label_id = match parse_req_id::<LabelId>(args, "label_id") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let thread_ids = match parse_id_list::<ThreadId>(args, "thread_ids") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let message_ids = match parse_id_list::<MessageId>(args, "message_ids") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let add = args.get("add").and_then(Value::as_bool).unwrap_or(true);
            port.label_messages(label_id, thread_ids, message_ids, add)
                .await
                .map(|ids| tool_result_json(&json!({ "updatedThreadIds": ids })))
        }
        "snooze_thread" => {
            let thread_id = match parse_req_id::<ThreadId>(args, "thread_id") {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let Some(until) = args.get("until").and_then(Value::as_i64) else {
                return tool_error(CoreError::InvalidInput {
                    field: "until".to_string(),
                    reason: "until Unix milliseconds is required".to_string(),
                });
            };
            port.snooze_thread(thread_id, UnixMs(until))
                .await
                .map(|id| tool_result_json(&json!({ "snoozedThreadId": id, "until": until })))
        }
        "archive_thread" => match parse_req_id::<ThreadId>(args, "thread_id") {
            Ok(tid) => port
                .archive_thread(tid)
                .await
                .map(|id| tool_result_json(&json!({ "archivedThreadId": id }))),
            Err(e) => Err(e),
        },
        unknown => Err(CoreError::InvalidInput {
            field: "name".to_string(),
            reason: format!("unknown MCP tool: {unknown}"),
        }),
    };

    match outcome {
        Ok(val) => val,
        Err(err) => tool_error(err),
    }
}

// ── Single-Writer Local IPC Server (Windows Named Pipe / Unix Socket) ───────

async fn serve_ipc_stream<S>(core: MailCore, stream: S)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut lines = BufReader::new(read_half).lines();
    let agent_id = AgentId::parse("hermes").expect("valid static agent id");
    let port = core.agent_port(agent_id);

    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let bridge_enabled = core.is_mcp_bridge_enabled();
        let reply = if !bridge_enabled {
            let id = serde_json::from_str::<Value>(trimmed)
                .ok()
                .and_then(|v| v.get("id").cloned())
                .unwrap_or(Value::Null);
            Some(rpc_err(
                id,
                -32001,
                "Ferrule MCP bridge is disabled in Settings",
            ))
        } else {
            handle_mcp_request_line(&port, trimmed).await
        };
        if let Some(resp) = reply {
            if write_half.write_all(resp.as_bytes()).await.is_err() {
                break;
            }
            if write_half.write_all(b"\n").await.is_err() {
                break;
            }
            let _ = write_half.flush().await;
        }
    }
}

/// Start the single-writer local IPC server inside the running Ferrule desktop app.
#[cfg(windows)]
pub fn spawn_ipc_server(core: MailCore, endpoint: String) -> tokio::task::JoinHandle<()> {
    use tokio::net::windows::named_pipe::ServerOptions;

    tokio::spawn(async move {
        let mut first = true;
        loop {
            let mut opts = ServerOptions::new();
            opts.first_pipe_instance(first);
            let server = match opts.create(&endpoint) {
                Ok(s) => {
                    first = false;
                    s
                }
                Err(err) => {
                    eprintln!("ferrule mcp ipc bind failed on {endpoint}: {err}");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }
            };
            if server.connect().await.is_ok() {
                let conn_core = core.clone();
                tokio::spawn(async move {
                    serve_ipc_stream(conn_core, server).await;
                });
            }
        }
    })
}

#[cfg(not(windows))]
pub fn spawn_ipc_server(core: MailCore, endpoint: String) -> tokio::task::JoinHandle<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;

    tokio::spawn(async move {
        let path = std::path::Path::new(&endpoint);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(err) = std::fs::create_dir_all(parent) {
                    eprintln!("ferrule mcp ipc mkdir failed for {}: {err}", parent.display());
                    return;
                }
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
        if path.exists() {
            if let Err(err) = std::fs::remove_file(path) {
                eprintln!("ferrule mcp ipc remove stale socket failed on {endpoint}: {err}");
                return;
            }
        }
        let listener = match UnixListener::bind(path) {
            Ok(l) => l,
            Err(err) => {
                eprintln!("ferrule mcp ipc bind failed on {endpoint}: {err}");
                return;
            }
        };
        if let Err(err) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            eprintln!("ferrule mcp ipc chmod 0600 failed on {endpoint}: {err}");
            return;
        }
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let conn_core = core.clone();
                    tokio::spawn(async move {
                        serve_ipc_stream(conn_core, stream).await;
                    });
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    })
}

// ── Thin Stdio Bridge (`ferrule mcp`) ───────────────────────────────────────

#[cfg(windows)]
async fn connect_ipc(endpoint: &str) -> Result<impl AsyncRead + AsyncWrite + Unpin, String> {
    use tokio::net::windows::named_pipe::ClientOptions;

    for _ in 0..5 {
        match ClientOptions::new().open(endpoint) {
            Ok(client) => return Ok(client),
            Err(e) if e.raw_os_error() == Some(231) => {
                // ERROR_PIPE_BUSY: wait briefly and retry
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => {
                return Err(format!(
                    "Could not connect to running Ferrule desktop app at {endpoint}: {e}"
                ));
            }
        }
    }
    Err(format!(
        "Timed out connecting to Ferrule desktop app at {endpoint}"
    ))
}

#[cfg(not(windows))]
async fn connect_ipc(endpoint: &str) -> Result<impl AsyncRead + AsyncWrite + Unpin, String> {
    tokio::net::UnixStream::connect(endpoint)
        .await
        .map_err(|e| format!("Could not connect to running Ferrule desktop app at {endpoint}: {e}"))
}

/// Forward MCP JSON-RPC 2.0 lines between stdin/stdout and the running Ferrule
/// desktop app's local pipe/socket.
pub async fn run_stdio_bridge(endpoint: Option<String>) -> Result<(), String> {
    let target = endpoint.unwrap_or_else(default_ipc_endpoint);
    let stream = connect_ipc(&target).await?;
    let (pipe_read, mut pipe_write) = tokio::io::split(stream);

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();
    let mut pipe_lines = BufReader::new(pipe_read).lines();
    let mut stdout = tokio::io::stdout();

    let to_pipe = async {
        while let Ok(Some(line)) = stdin_lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            if pipe_write.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if pipe_write.write_all(b"\n").await.is_err() {
                break;
            }
            if pipe_write.flush().await.is_err() {
                break;
            }
        }
    };

    let to_stdout = async {
        while let Ok(Some(line)) = pipe_lines.next_line().await {
            if stdout.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if stdout.write_all(b"\n").await.is_err() {
                break;
            }
            if stdout.flush().await.is_err() {
                break;
            }
        }
    };

    tokio::select! {
        _ = to_pipe => {}
        _ = to_stdout => {}
    }
    Ok(())
}

/// Send a single JSON-RPC request over the local IPC pipe/socket and return the
/// response line. Useful for integration tests and health probes.
pub async fn send_ipc_request_once(endpoint: &str, request_line: &str) -> Result<String, String> {
    let stream = connect_ipc(endpoint).await?;
    let (read_half, mut write_half) = tokio::io::split(stream);
    write_half
        .write_all(request_line.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    write_half
        .write_all(b"\n")
        .await
        .map_err(|e| e.to_string())?;
    write_half.flush().await.map_err(|e| e.to_string())?;

    let mut lines = BufReader::new(read_half).lines();
    match lines.next_line().await {
        Ok(Some(line)) => Ok(line),
        Ok(None) => Err("IPC server closed connection before responding".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

// ── Local SSE Event Stream + Localhost Webhook Push ─────────────────────────

/// Start the localhost-only SSE server (`127.0.0.1:<port>/events`) and the
/// optional localhost webhook POST dispatcher. Automatically rebinds when `sse_port`
/// changes in Settings.
pub fn spawn_push_services(core: MailCore) {
    // 1. Local SSE server on 127.0.0.1:<sse_port> with live rebind on config update
    let sse_core = core.clone();
    tokio::spawn(async move {
        let mut port_rx = sse_core.subscribe_sse_port();
        loop {
            let port = *port_rx.borrow_and_update();
            let addr = format!("127.0.0.1:{port}");
            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(err) => {
                    eprintln!("ferrule sse bind failed on {addr}: {err}");
                    if port_rx.changed().await.is_err() {
                        break;
                    }
                    continue;
                }
            };
            loop {
                tokio::select! {
                    changed = port_rx.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        break;
                    }
                    accepted = listener.accept() => {
                        if let Ok((socket, _)) = accepted {
                            let conn_core = sse_core.clone();
                            tokio::spawn(async move {
                                handle_sse_connection(conn_core, socket).await;
                            });
                        }
                    }
                }
            }
            drop(listener);
        }
    });

    // 2. Optional localhost webhook POST dispatcher
    let hook_core = core;
    tokio::spawn(async move {
        let mut rx = hook_core.subscribe_hermes_events();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok();
        while let Ok(event) = rx.recv().await {
            let Some(client) = client.as_ref() else {
                continue;
            };
            if !hook_core.is_mcp_bridge_enabled() {
                continue;
            }
            let Ok(cfg) = hook_core.get_agent_bridge_config().await else {
                continue;
            };
            if !cfg.mcp_enabled || !cfg.webhook_enabled || cfg.webhook_url.trim().is_empty() {
                continue;
            }
            if validate_localhost_webhook_url(&cfg.webhook_url).is_err() {
                continue;
            }
            let Ok(body) = serde_json::to_vec(&event) else {
                continue;
            };
            let mut req = client
                .post(&cfg.webhook_url)
                .header("Content-Type", "application/json")
                .header("X-Ferrule-Event", event.event_name());
            if let Ok(Some(secret)) = hook_core.secrets().get(&SecretKey::webhook_secret()) {
                let sig = sign_webhook_payload(secret.expose(), &body);
                req = req.header("X-Ferrule-Signature", sig);
            }
            let _ = req.body(body).send().await;
        }
    });
}

fn is_loopback_host(host_header: &str) -> bool {
    let trimmed = host_header.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return false;
    }
    let host_only = if let Some(rest) = trimmed.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        trimmed.split(':').next().unwrap_or("")
    };
    matches!(host_only, "127.0.0.1" | "localhost" | "::1")
}

fn extract_bearer_or_query_token<'a>(req_str: &'a str, target: &'a str) -> Option<&'a str> {
    for line in req_str.lines().skip(1) {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("authorization") {
                if let Some(tok) = v.trim().strip_prefix("Bearer ") {
                    let t = tok.trim();
                    if !t.is_empty() {
                        return Some(t);
                    }
                }
            }
        }
    }
    if let Some((_, query)) = target.split_once('?') {
        for pair in query.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                if k == "token" && !v.trim().is_empty() {
                    return Some(v.trim());
                }
            }
        }
    }
    None
}

async fn handle_sse_connection(core: MailCore, mut socket: tokio::net::TcpStream) {
    let mut buf = [0u8; 4096];
    let n = match tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await {
        Ok(n) if n > 0 => n,
        _ => return,
    };
    let req_str = String::from_utf8_lossy(&buf[..n]);
    let first_line = req_str.lines().next().unwrap_or("");
    let raw_target = first_line.split_whitespace().nth(1).unwrap_or("/");
    let path = raw_target.split('?').next().unwrap_or("/");

    // Reject non-loopback Host headers to prevent DNS rebinding.
    let host_ok = req_str
        .lines()
        .skip(1)
        .find_map(|line| {
            let (k, v) = line.split_once(':')?;
            k.trim().eq_ignore_ascii_case("host").then_some(v)
        })
        .map(is_loopback_host)
        .unwrap_or(false);
    if !host_ok {
        let _ = socket
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    }

    if path == "/health" {
        let body = r#"{"status":"ok"}"#;
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(resp.as_bytes()).await;
        return;
    }

    if path != "/events" {
        let _ = socket
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    }

    if !core.is_mcp_bridge_enabled() {
        let _ = socket
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    }

    let provided_token = extract_bearer_or_query_token(&req_str, raw_target);
    if provided_token != Some(core.sse_token()) {
        let _ = socket
            .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    }

    let headers = "HTTP/1.1 200 OK\r\n\
                   Content-Type: text/event-stream\r\n\
                   Cache-Control: no-cache\r\n\
                   Connection: keep-alive\r\n\r\n\
                   : connected to ferrule\n\n";
    if socket.write_all(headers.as_bytes()).await.is_err() {
        return;
    }

    let mut rx = core.subscribe_hermes_events();
    let mut keepalive = tokio::time::interval(Duration::from_secs(25));
    loop {
        tokio::select! {
            _ = keepalive.tick() => {
                if !core.is_mcp_bridge_enabled() {
                    break;
                }
                if socket.write_all(b": keep-alive\n\n").await.is_err() {
                    break;
                }
            }
            ev = rx.recv() => {
                if !core.is_mcp_bridge_enabled() {
                    break;
                }
                match ev {
                    Ok(event) => {
                        if let Some(frame) = format_sse_frame(&event) {
                            if socket.write_all(frame.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

pub fn format_sse_frame(event: &HermesPushEvent) -> Option<String> {
    let json = serde_json::to_string(event).ok()?;
    Some(format!("event: {}\ndata: {}\n\n", event.event_name(), json))
}
