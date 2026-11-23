# Galapaghost — Agreed Product & Architecture Blueprint

## Goal
A free, shareable, low-memory desktop email client for **Windows and Linux** with a modern Gmail-like workflow and features normally locked behind paid email subscriptions, designed with a clean core boundary for future agentic AI assistants.

## Settled Decisions

| Area | Decision |
|---|---|
| **Platforms & Distribution** | Windows + Linux, free & shareable (MIT license) |
| **Stack** | **Tauri v2** (modular **Rust** mail engine + lightweight OS WebView + React/TypeScript UI) |
| **Accounts** | Unified **IMAP/SMTP** engine supporting Gmail (App Password or BYO OAuth2 + Gmail `X-GM-LABELS` / `X-GM-THRID` extensions), Outlook (Microsoft Entra public client OAuth2), and generic IMAP/SMTP |
| **Privacy & Infra** | Strictly local-only (zero cloud server), optional lightweight system-tray runner for local timers |
| **Local Storage & Secrets** | 90-day initial full sync + on-demand archive/attachments in **SQLite + FTS5** instant search; credentials/tokens in **OS keychain** (Windows Credential Manager / Linux Secret Service) |
| **v1 Scope** | Email-only (no calendar/contacts yet) |
| **v1 Features** | Unified inbox, Gmail-style threading/labels/shortcuts, toggleable split/classic reading pane, light & dark mode, Snooze, Send Later, Follow-up reminders, Undo Send, `Ctrl+K` command bar, templates/snippets, spy-pixel blocking, one-click unsubscribe |
| **Assistant Readiness (Phase 2)** | Rust mail core cleanly decoupled from the UI so a local MCP server + human-in-the-loop **Agent Draft Approval Queue** (for Hermes / Grok Bot style assistants) plugs in without rewriting the core |

## Build Order
1. **UI Bake-Off** — Done. **Mosaic** (Multi-Account Spaces with top color-coded space switcher, account row pills, and color-locked "Replying From" identity guard) selected as the UI direction.
2. **Core Scaffold + Account Sync** — Tauri v2 + Rust `mail-core` + SQLite/FTS5 + OS keychain + Gmail/Outlook/IMAP sync.
3. **Core Email Experience** — Unified inbox, threads, reading (with spy-pixel blocking), compose/reply/archive, search, shortcuts.
4. **Paid-Client Features** — Snooze, Send Later, Follow-up reminders, Undo Send, `Ctrl+K`, templates/snippets, unsubscribe, tray runner.
5. **Phase 2** — Local MCP interface + human-in-the-loop Agent Approval Queue.
