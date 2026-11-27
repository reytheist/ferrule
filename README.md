<img src="assets/icon.svg" width="64" height="64" alt="">

# Ferrule

A local-first, low-memory personal desktop email and calendar client for Windows, macOS, and Linux built with Tauri v2, Rust (`mail-core`), SQLite + FTS5, OS keychain storage, and React (Mosaic UI).

## What is included

- **Multi-account Mosaic workspace**: Color-coded Account Space switcher and a locked "From" identity guard for Gmail (App Password or BYO OAuth2), Outlook (OAuth2), and pre-configured IMAP/SMTP presets (iCloud, Yahoo, Fastmail, Zoho, GMX, IONOS, or custom).
- **Power-user email workflow built in**: Unified inbox, Gmail-style threading and category tabs, toggleable Split/Classic reading pane, Light and Dark themes, Snooze, Send Later, Follow-up reminders, Undo Send, `Ctrl+K` command palette, saved templates/snippets, spy-pixel blocking with CSP-sandboxed HTML rendering, and 1-click unsubscribe.
- **Personal Calendar & `.ics` bridges**: Day/Week/Month calendar views, natural-language quick-add, inline `.ics` invite RSVP (with `VTIMEZONE`, IANA/Windows `TZID`, `Z`, and `VALUE=DATE` support), read-only personal `.ics` busy overlays, and 1-click compose availability slot insertion.
- **Local-first and low-memory**: Local SQLite + contentless FTS5 search, all credentials stored strictly in the OS keychain (`SecretStore`), and a lightweight system-tray runner.
- **Hermes / Assistant boundary**: `mail_core::agent::AgentPort` exposes read, search, and `stage_draft` (`PendingApproval`) with no send or approve method so future MCP agents cannot send mail without human approval.

## Run locally

```bash
# Desktop app (Tauri v2 + Rust mail-core + Mosaic UI)
pnpm install
pnpm tauri dev

# Browser UI preview (uses in-memory mock store)
pnpm dev

# Verification (local-only; no GitHub Actions CI)
pnpm verify

# Local desktop build
pnpm tauri build
```

## Docs

- [Personal Premium Mail & Hermes Roadmap](docs/PLAN.md)
- [Blueprint](docs/BLUEPRINT.md)
- [Architecture](docs/ARCHITECTURE.md)
- [UI Bake-Off](design/bakeoff/full-desktop.html)
