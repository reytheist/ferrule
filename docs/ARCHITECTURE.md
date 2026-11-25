# Ferrule — Architecture

## Problem
Ferrule is a free, shareable, low-memory Windows and Linux desktop email client built on **Tauri v2** (Rust `mail-core` + React/TypeScript **Mosaic** UI). Four constraints shape the architecture:

1. **Low memory.** Keeping timers alive in the system tray must not hold a WebView or full body caches in RAM (`PRAGMA mmap_size=0`, close-to-tray destroys the WebView).
2. **Crash-safe timers.** Undo Send, Send Later, Snooze, and Follow-up timers survive restarts and crashes without double-sending mail.
3. **Compile-time Phase-2 agent safety.** Future MCP agents (Hermes / Grok Bot style) must be unable to send mail by construction, not convention.
4. **Unified protocol engine.** One IMAP/SMTP engine handles Gmail (with `X-GM-LABELS` / `X-GM-THRID`), Outlook (Microsoft Entra public client OAuth2), and generic IMAP/SMTP.

## Usage (caller's view)

### Tauri command adapter (`src-tauri/src/commands.rs`)
```rust
#[tauri::command]
pub async fn list_threads(
    core: tauri::State<'_, MailCore>,
    query: ThreadQuery,
) -> Result<ThreadPage, CoreError> {
    core.list_threads(query).await
}

#[tauri::command]
pub async fn send_draft(
    core: tauri::State<'_, MailCore>,
    draft_id: DraftId,
    mode: SendMode,
) -> Result<SendReceipt, CoreError> {
    core.send_draft(draft_id, mode).await
}
```

### React Mosaic UI (`src/ipc/core.ts` + feature hooks)
```ts
const page = await core.listThreads({ scope, view, cursor: null, limit: 50 });
const outcome = await core.actOnThreads({ kind: "archive" }, [thread.id]);
if (outcome.undoAction) {
  showUndoToast("Archived thread", () => core.actOnThreads(outcome.undoAction!, [thread.id]));
}
```

### Phase-2 MCP draft staging (`crates/mail-mcp`, additive)
```rust
let port = core.agent_port(AgentId::parse("hermes")?);
let hits = port.search("invoice", 5).await?;
let draft = port.stage_draft(StageAgentDraft {
    thread_id: hits[0].id,
    body_text: "Confirmed — paying Friday.".into(),
    rationale: "Invoice #42 requests payment confirmation.".into(),
}).await?;
// `AgentPort` has no `send` or `approve` method; `draft.state` is `PendingApproval`.
```

## Module map

```text
ferrule/
├── Cargo.toml                  # workspace: crates/mail-core, src-tauri (+ crates/mail-mcp in Phase 2)
├── crates/mail-core/           # ALL domain logic, zero Tauri/WebView dependencies
│   └── src/
│       ├── lib.rs              # MailCore facade + public domain types only
│       ├── ids.rs              # Branded UUID newtypes (AccountId, ThreadId, MessageId, DraftId, JobId, ...)
│       ├── error.rs            # Serializable user-safe CoreError
│       ├── auth.rs             # ProviderAuth enum, SecretString (zeroized), SecretStore trait
│       ├── host.rs             # Clock trait (SystemClock / ManualClock), OsKeychain adapter
│       ├── events.rs           # CoreEvent broadcast channel
│       ├── sanitize.rs         # Pure spy-pixel sanitizer + CSP srcdoc HtmlDocument newtype
│       ├── schedule.rs         # Pure state machines: OutboxState, SnoozeState, FollowUpState
│       ├── store/              # PRIVATE SQLite + FTS5 writer/reader + 0001_init.sql migration
│       ├── sync/               # PRIVATE IMAP/SMTP transport + Dialect (Gmail | Outlook | Generic)
│       └── agent.rs            # Phase-2 AgentPort capability boundary (read/search/stage_draft only)
├── src-tauri/                  # Thin desktop shell: Tauri v2 commands, event bridge, tray runner
│   └── src/
│       ├── lib.rs              # Boot MailCore with OsKeychain + SystemClock, emit CoreEvent
│       ├── commands.rs         # One-line #[tauri::command] adapters
│       └── main.rs             # Desktop entry point
└── src/                        # React + TypeScript + Vite Mosaic UI
    ├── ipc/core.ts             # Single transport boundary (Tauri invoke + typed browser preview fallback)
    ├── types/mail.ts           # Strict TypeScript contract mirrored from mail-core (branded IDs, unions)
    ├── styles/tokens.css       # Mosaic light/dark CSS tokens + account space colors
    ├── features/
    │   ├── spaces/             # Top color-coded Account Space switcher
    │   ├── threads/            # Category tabs + thread list + account pills + hover triage
    │   ├── reading/            # Split/Classic reading pane, sandboxed iframe srcdoc, Privacy Ribbon
    │   ├── compose/            # Reply Dock, From Identity Guard, Snippets, Send/Send Later, Compose modal
    │   ├── command-bar/        # Ctrl+K command palette + keyboard shortcut registry
    │   └── accounts/           # Add Account wizard (Gmail App Password / Gmail OAuth / Outlook / IMAP)
    └── App.tsx                 # Mosaic shell + Undo Toast
```

## Shape & Load-Bearing Decisions

- **Single deep `MailCore` facade** (`principle-boundary-discipline`, `principle-foundational-thinking`): Callers invoke domain verbs (`add_account`, `list_accounts`, `list_threads`, `open_thread`, `act_on_threads`, `search`, `stage_draft`, `send_draft`, `cancel_send`, `snooze_thread`, `set_follow_up`, `list_templates`, `save_template`). SQL, IMAP UIDs, OAuth tokens, and raw HTML never cross the crate boundary.
- **Branded IDs and discriminated unions** (`principle-type-system-discipline`, `principle-model-the-domain`): `AccountId`, `IdentityId`, `LabelId`, `ThreadId`, `MessageId`, `DraftId`, `JobId`, `TemplateId` are distinct newtypes in Rust and branded strings in TypeScript. `ProviderAuth` is an exhaustive enum (`GmailAppPassword`, `GmailOAuth`, `OutlookOAuth`, `ImapPassword`) storing zero secrets; secrets travel only through `SecretStore` using `Zeroizing<String>`.
- **One unified timer model**: Undo Send and Send Later share `OutboxState` (`Scheduled -> Sending -> Verifying -> RetryWait -> Sent | Failed | Cancelled`). MIME bytes and `Message-ID` are frozen at enqueue; crash recovery moves expired `Sending` leases to `Verifying` before any retry so mail is never double-sent.
- **Sanitize on read with CSP `srcdoc`**: `message_body` keeps raw HTML privately; `sanitize()` runs on read to produce `HtmlDocument` (private constructor) with an embedded `<meta http-equiv="Content-Security-Policy">` and a `PrivacyReport`. Tracking pixels (`1x1`, hidden, known tracker hosts, CSS `url()`) are stripped even when remote images are allowed.
- **Compile-time From Identity Guard & Phase-2 safety**: Drafts belong to one `AccountId` and lock `from_identity` on replies unless explicitly unlocked. `Sendable` is a private token constructible only inside human send/approve paths; `AgentPort` cannot obtain one.
- **SQLite + FTS5 low-memory store**: WAL mode, `PRAGMA mmap_size=0`, contentless FTS5 (`content=''`, `contentless_delete=1` via bundled SQLite), DB-level `CHECK` constraints mirroring state enums, and 90-day initial window cursors with on-demand body/attachment hydration.

## Synthesis Decision
Synthesized from parallel architectural sketches by **Claude Sonnet 5.5** (base), **MiMo-v2.6-Pro**, and **GLM-5.4**:
- **Base (Claude Sonnet 5.5)**: `AgentPort` + private `Sendable` token for compile-time Phase-2 safety, read-time sanitization with embedded CSP `srcdoc`, `OutboxState::Verifying` crash-safe deduplication, memory-only access tokens with refresh tokens in OS keychain, closed `Dialect` enum, and contentless FTS5.
- **Grafted from MiMo-v2.6-Pro**: Flat single `MailCore` facade (eliminating nested handle boilerplate), DB `CHECK` constraints mirroring state enums, and zeroized `SecretString`.
- **Grafted from GLM-5.4**: `ActionOutcome { undo_action }` on idempotent thread actions for clean, uniform Undo toasts.
- **Rejected**: Separate local JSON-RPC daemon in v1 (extra resident process + wire leakage), ingest-time-only sanitization (breaks per-sender image allowlists and policy updates), and `trait Provider` plugin hierarchies (unneeded with one IMAP/SMTP engine).

## Tradeoffs Accepted
- We accept local-only Snooze in v1 in exchange for never mutating remote IMAP folders for local timers.
- We accept sanitizing HTML on each thread read (a few ms) in exchange for instant policy updates and zero stale sanitized copies.
- We accept memory-only OAuth access tokens (one refresh after restart) in exchange for staying well under Windows Credential Manager's ~2.5 KB secret limit.
- We accept a 90-day initial local sync window in exchange for fast onboarding and low disk/RAM usage, with on-demand archive/attachment hydration.

## Implementation Reconciliation
- *(Updated during build whenever an accepted deviation changes a contract.)*
