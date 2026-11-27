# Ferrule — Personal Premium Mail & Hermes Roadmap

Ferrule is a local-first, low-memory personal desktop email and calendar client (Tauri v2 + Rust `mail-core` + SQLite/FTS5 + OS keychain + React/TS Mosaic UI), designed for keyboard-first power-user workflows and deep integration with a Hermes-like personal AI agent system.

## Architectural Principles

- **Keep Tauri v2 + React/TS UI**: Reliable real-world HTML email rendering (WebView2/WebKitGTK + `ammonia` sanitization + strict CSP sandbox) and fast UI iteration for custom power-user workflows, without the experimental HTML renderer and crate-patching burden of Slint + Blitz.
- **Local-first & single-writer**: SQLite + FTS5 owns all local state; secrets (IMAP/SMTP passwords, OAuth refresh tokens, AI/webhook keys) live strictly in `SecretStore` (OS keychain).
- **Per-account async sync actors**: Latency-sensitive work (commands, queued mutations, `LIST-STATUS`, interruptible IMAP IDLE) never waits on heavy backfill, bulk body downloads, or SMTP submission.
- **Safe agent boundary**: Hermes integrates through a single-writer local MCP bridge (`ferrule mcp`) with per-account/folder scoping, an undoable agent activity log for triage mutations, and mandatory human approval (`PendingApproval`) before any email is sent.
- **Local-only builds**: Built and installed locally via `pnpm tauri build` / `pnpm tauri dev` (no cloud release distribution or auto-updater).

---

## Phase 1 — Strip-Out & Re-Focus

Remove all multi-user collaboration, Pigment Staff branding, and cloud distribution/auto-updater machinery while keeping personal mail, personal Calendar, generic color-coded Account Spaces, and all IMAP/SMTP server presets (including IONOS).

### Removals
- **Backend / Crates**:
  - `crates/staff-sync-server/` (remove from workspace)
  - `crates/mail-core/src/{staff.rs, staff_core.rs, workbook.rs, sync/staff.rs, store/staff.rs}`
  - Remove `calamine` dependency from `Cargo.toml` / `crates/mail-core/Cargo.toml`
  - Strip Meeting Docs & Action Items from `calendar.rs`, `calendar_core.rs`, `store/calendar.rs` (keep personal `calendar_event`, `message_invite`, `ics_overlay`, `ics_overlay_event`)
  - Forward-compatible SQLite migration dropping removed staff tables (`DROP TABLE IF EXISTS`) while preserving existing accounts, synced mail, templates, and personal calendar events
  - Remove `src-tauri/src/updater.rs` and updater commands/background loop
- **Frontend**:
  - `src/features/staff/*`, `src/features/actions/*`, `src/features/updates/*`, `src/features/calendar/MeetingDocPanel.tsx`
  - Strip `pigment_staff` space kind, Pigment 1-click preset, and staff-sharing locks (keep generic color-coded Account Spaces, locked "From" identity guard, and all IMAP/SMTP presets including IONOS)
  - Strip staff/action/update state & IPC methods from `src/ipc/`, `src/types/`, `src/features/shell/useMailApp.ts`, `Sidebar`, `Topbar`, `SettingsModal`, `OnboardingModal`, `FeatureTour`, `ReadingPane`, `CalendarView`, `CommandBar`
- **Docs & Scripts**:
  - Remove `Dockerfile`, `.dockerignore`, `scripts/release.ps1`, `scripts/release.sh`, `docs/COLLABORATION_SPEC.md`, `docs/STAFF_ONBOARDING.md`, `docs/STAFF_SYNC_DEPLOY.md`
  - Remove `build:server` script from `package.json`
  - Update `AGENTS.md`, `README.md`, `GLOSSARY.md`, `src-tauri/tauri.conf.json`

**Exit criteria**: `pnpm verify` passes with zero warnings and all remaining unit tests green.

---

## Phase 2 — Sync Engine Overhaul & Attachments / Rich Compose

Rebuild `mail-core`'s sync layer around per-account async actors (inspired by Flectar's latency/bulk split):

1. **Async IMAP (`async-imap` + `tokio-rustls`)**:
   - Main per-account actor: drain commands → execute fair slice of queued actions → `LIST-STATUS` across folders (opening only folders whose counts changed; periodic full flag/label pass) → park in interruptible IMAP IDLE (`tokio::select!` on command channel).
   - Separate bounded connections for historical header backfill and priority/bulk body downloads (removing the 50-message cap).
   - Dedicated durable SMTP submission worker so sending never blocks on mailbox sync.
2. **Smart Pacing & Auth Circuit Breaker**:
   - Differentiate transient network drops (fast jittered backoff `1s → 5m`), server rate limits (`60s → 1h`, honoring `Retry-After` up to 24h), and auth failures (halt background retries immediately until user re-authenticates).
3. **Fair-Slice Offline Action Queue**:
   - Optimistic local mutations backed by a durable SQLite queue processed in bounded slices (e.g., max 20 actions or 2s per slice) so bulk operations never starve new mail or sends.
4. **Attachments, Rich Compose & Named Signatures**:
   - Full attachment metadata, on-demand download/save, inline preview, and compose attachment uploads.
   - Rich-text compose toolbar + per-account named signatures.

---

## Phase 3 — Premium Mail Workflow

1. **Gmail-Syntax Search Parser & Saved Searches**:
   - Parse `from:`, `to:`, `in:`, `is:`, `has:`, `before:`, `after:`, `older_than:`, `newer_than:`, `larger:`, `smaller:`, `filename:`, `OR`, parentheses, and `-negation` into a unified query AST.
   - Drive accent-folded SQLite FTS5 (with relaxed `OR` fallback) and optional IMAP server-side search.
   - Save frequent queries as sidebar views.
2. **Sender Screener & VIP Split Inbox**:
   - First-time sender screening queue (Approve to Inbox / Paper Trail / Feed / Block).
   - VIP sender list and split-inbox sections.
3. **Client-Side Rules & Auto-Labeling**:
   - Rule builder (match sender/domain/subject/headers → label, move, archive, mark read, star, or notify).

---

## Phase 4 — Hermes MCP Bridge & In-Pane AI

1. **Single-Writer `ferrule mcp` Stdio Bridge**:
   - Thin `ferrule mcp` subcommand/binary speaking MCP over stdio and forwarding JSON-line requests over a user-only local named pipe (Windows) / Unix socket (Linux/macOS) to the running Ferrule desktop app.
   - One sync engine, one DB writer, per-account and per-folder permission toggles in Settings.
   - Tool surface: `list_accounts`, `list_folders`, `list_labels`, `list_messages`, `search_messages`, `read_message`, `read_conversation`, `list_events`, `create_draft`, `update_draft`, `mark_messages`, `move_messages`, `label_messages`, `snooze_thread`, `archive_thread`.
2. **Safety & Undoable Agent Activity Log**:
   - Prompt-injection framing on all returned message content (plain text only, never raw HTML).
   - Every triage mutation logged in SQLite with an undo payload so any Hermes action can be inspected and reversed in 1 click.
   - `create_draft` / `update_draft` stage into `PendingApproval` with rationale; **no agent path can ever send mail without explicit human approval in Ferrule**.
3. **Proactive Hermes Event Push**:
   - Local SSE event stream on the agent bridge + optional localhost webhook POST (e.g., `http://127.0.0.1:<port>/hooks/ferrule` with shared secret in `SecretStore`) emitting safe metadata (`new_message`, `new_invite`) so Hermes can wake up on incoming mail without polling.
4. **In-Pane OpenAI-Compatible AI**:
   - Provider-agnostic base URL + model in Settings, API key in `SecretStore` (points at Hermes, Ollama, or any hosted OpenAI-compatible API).
   - Thread summary card, selection Q&A, and one-click draft staging.
