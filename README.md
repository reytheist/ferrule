<img src="assets/icon.svg" width="64" height="64" alt="">

# Ferrule

A free, shareable, local-first desktop email client for Windows and Linux built with Tauri v2, Rust (`mail-core`), SQLite + FTS5, and React (Mosaic UI).

## What is included

- **Multi-account Mosaic workspace**: Color-coded Account Space switcher and a locked "From" identity guard for Gmail (App Password or BYO OAuth2), Outlook (OAuth2), and generic IMAP/SMTP.
- **Paid-client workflow built in**: Unified inbox, Gmail-style threading and category tabs, toggleable Split/Classic reading pane, Light and Dark themes, Snooze, Send Later, Follow-up reminders, Undo Send, `Ctrl+K` command palette, saved templates/snippets, spy-pixel blocking with CSP-sandboxed HTML rendering, and 1-click unsubscribe.
- **Local-first and low-memory**: 90-day initial local sync window in SQLite with contentless FTS5 search, credentials stored only in the OS keychain (Windows Credential Manager / Linux Secret Service), and a lightweight system-tray runner.
- **Phase-2 assistant boundary**: `mail_core::agent::AgentPort` exposes read, search, and `stage_draft` (`PendingApproval`) with no send or approve method so future MCP agents (such as Hermes or Grok Bot) cannot send mail without human approval.

## Run locally

```bash
# Desktop app (Tauri v2 + Rust mail-core + Mosaic UI)
pnpm install
pnpm tauri dev

# Browser UI preview (uses in-memory mock store)
pnpm dev

# Verification
cargo test -p mail-core
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

## Docs

- [Blueprint](docs/BLUEPRINT.md)
- [Architecture](docs/ARCHITECTURE.md)
- [UI Bake-Off](design/bakeoff/full-desktop.html)
