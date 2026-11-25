<img src="assets/icon.svg" width="64" height="64" alt="">

# Ferrule

A free, shareable, local-first desktop email and staff collaboration client for Windows, macOS, and Linux built with Tauri v2, Rust (`mail-core` + `staff-sync-server`), SQLite + FTS5, OS keychain storage, and React (Mosaic UI).

## What is included

- **Multi-account Mosaic workspace (Phase A)**: Color-coded Account Space switcher and a locked "From" identity guard for **Pigment Staff (IONOS)** 1-click preset, Gmail (App Password or BYO OAuth2), Outlook (OAuth2), and generic IMAP/SMTP. Personal spaces block accidental staff sharing by default, and developer/MCP tools stay hidden behind **Settings -> Developer Tools**.
- **Paid-client email workflow built in**: Unified inbox, Gmail-style threading and category tabs, toggleable Split/Classic reading pane, Light and Dark themes, Snooze, Send Later, Follow-up reminders, Undo Send, `Ctrl+K` command palette, saved templates/snippets, spy-pixel blocking with CSP-sandboxed HTML rendering, and 1-click unsubscribe.
- **Staff Sync & Email Collaboration (Phase B)**: Opt-in, decoupled Staff Sync workspace with selective **Share with Staff** thread snapshots, **Hand Off** ownership (`Open / Waiting / Done`), internal staff comments with `@mentions`, single-editor **Draft Lock & Handoff** (`Claim / Take Over / Request Review / Approve`), Shared Templates, Shared Show Contacts (with reading-pane Contact Cards), and Pinned Staff Links.
- **Pigment Shared Calendar & Meetings (Phase C)**: Day/Week/Month calendar views, natural-language quick-add, inline `.ics` invite RSVP (with `VTIMEZONE`, IANA/Windows `TZID`, `Z`, and `VALUE=DATE` support), read-only personal `.ics` busy overlays, 1-click compose availability slot insertion, attached **Meeting Docs** with automatic recurring action-item carryover, and Discord meeting links + Markdown recap export.
- **Action Items & Operations Workbook Split (Phase D)**: Dedicated Action Items view (`Mine`, `Unassigned`, `By Dept.`, `By Meeting`, `Upcoming / Overdue`, `All`), optional calendar sync for dated items, and `.xlsx` / CSV Operations Workbook import and CSV export.
- **Companion Sync Server, Live SSE & Offline Outbox (Phase E/F)**: Standalone self-hostable `crates/staff-sync-server` (`axum` + SQLite + Bearer auth + tombstone delete propagation + `rev` monotonicity), background SSE live sync with automatic reconnect, and a durable SQLite `staff_outbox` with FIFO flush on reconnect and an offline pending counter in the UI.
- **Local-first and low-memory**: 90-day initial local sync window in SQLite with contentless FTS5 search, all credentials (IMAP/SMTP passwords, OAuth refresh tokens, Staff Sync tokens, Discord webhooks) stored strictly in the OS keychain (`SecretStore`), and a lightweight system-tray runner.
- **Assistant boundary**: `mail_core::agent::AgentPort` exposes read, search, and `stage_draft` (`PendingApproval`) with no send or approve method so future MCP agents cannot send mail without human approval.

## Run locally

```bash
# Desktop app (Tauri v2 + Rust mail-core + Mosaic UI)
pnpm install
pnpm tauri dev

# Browser UI preview (uses in-memory mock store)
pnpm dev

# Companion Staff Sync server (optional, for multi-user live sync)
STAFF_SYNC_TOKENS="dev-token" cargo run -p staff-sync-server --bin staff-sync-server

# Verification
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

## Docs

- [Blueprint](docs/BLUEPRINT.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Staff Collaboration Spec](docs/COLLABORATION_SPEC.md)
- [Staff Sync Server Deployment](docs/STAFF_SYNC_DEPLOY.md)
- [UI Bake-Off](design/bakeoff/full-desktop.html)
