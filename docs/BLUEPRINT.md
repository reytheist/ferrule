# Ferrule — Agreed Product & Architecture Blueprint

## Goal
A free, non-commercial, low-memory desktop **email and staff collaboration client** for **Windows, Linux, and macOS** built primarily for the staff of the **Pigment Miniatures Show** (`info@`, `sponsors@`, `exhibition@`, `marketing@`, `volunteers@`), combining a modern Gmail-like email workflow with paid-client productivity, selective team collaboration, shared calendar/meetings, and action-item tracking—while keeping optional agentic tooling strictly gated under **Settings → Developer Tools**.

## Settled Decisions

| Area | Decision |
|---|---|
| **Target Audience & Distribution** | Pigment Miniatures Show staff (non-commercial, MIT-licensed, shareable); **Windows + Linux + macOS** desktop (no mobile app) |
| **Stack** | **Tauri v2** (modular **Rust** `mail-core` engine + lightweight OS WebView + React/TypeScript **Mosaic** UI) |
| **Accounts & Onboarding** | Unified **IMAP/SMTP** engine with a 1-click **IONOS / Pigment Staff** preset (`imap.ionos.com` / `smtp.ionos.com`), plus Gmail (App Password or BYO OAuth2 + `X-GM-LABELS` / `X-GM-THRID`), Outlook (Microsoft Entra OAuth2), and generic IMAP/SMTP. Personal accounts in Mosaic Spaces are strictly optional per staff member. |
| **Sync & Privacy Architecture** | **Hybrid**: strictly local-first SQLite + FTS5 for all email by default, plus an opt-in, decoupled **Staff Sync service** (lightweight HTTP + SSE contract) for Pigment staff collaboration. Because every role mailbox has a single owner (no shared IMAP accounts), email stays 100% private unless a user explicitly shares a thread; personal (non-Pigment) spaces have Staff Sharing disabled by default. |
| **Local Storage & Secrets** | 90-day initial full sync + on-demand archive/attachments in **SQLite + FTS5** instant search; all credentials/tokens strictly in **OS keychain** (Windows Credential Manager / Linux Secret Service / macOS Keychain via `keyring`) |
| **Core Email Features (Built)** | Unified inbox, color-coded Mosaic Account Spaces with locked "From" identity guard, Gmail-style threading/labels/shortcuts, toggleable split/classic reading pane, light & dark mode, Snooze, Send Later, Follow-up reminders, Undo Send, `Ctrl+K` command bar, personal templates/snippets, spy-pixel blocking, one-click unsubscribe, tray runner |
| **Email Collaboration** | **Selective Thread Sharing & Handoff** (explicit per-thread opt-in), internal private comments on shared threads, **Draft Lock & Handoff** (single active editor with review/take-over, not CRDT co-authoring), **Shared Templates**, **Shared Show Contacts** (with lightweight email sidebar Contact Card), and **Pinned Staff Links** |
| **Calendar & Meetings** | Built-in **Pigment Shared Calendar** (synced via Staff Sync), `.ics` invite RSVP in email, read-only personal `.ics` URL overlays, 1-click availability slot insertion in compose, **Meeting Docs** (collaborative agendas, Markdown notes, automatic carryover of unfinished action items across recurring meetings), and **Discord** meeting links + recap copy/webhook |
| **Action Items & Operations Workbook** | Dedicated **Action Items** left-rail view (`Mine / Unassigned / By Dept / By Meeting / All`) with deep links back to source emails/meetings and `.xlsx`/CSV import & export to absorb the `2027 Operations Workbook` timeline/tasks (`Dept.`, `Date`, `Start/End Time`, `Where`, `$$$`, `Done`). Dense spatial/matrix sheets (`Floor Plan`, `Vendor MapPayment`, `Room Supplies`, `Class Schedule`, 26-column fulfillment tracker) stay in Google Drive as pinned links. |
| **Explicit Non-Goals** | Not a commercial SaaS product; no standalone team chat channels (staff use Discord); no voice/video calling or audio transcription; no public Calendly-style booking pages; no Kanban/CRM/SLA suite; no two-way Google Sheets or Google/Outlook Calendar write-back in initial phases |
| **Developer Tools & Assistant Readiness** | `mail_core::agent::AgentPort` (read, search, `stage_draft` with compile-time `PendingApproval` safety) and any local MCP server (Hermes / Grok Bot style) are **off by default** and gated strictly behind **Settings → Developer Tools** |

## Build Order

### Core Email Foundation (Completed)
1. **UI Bake-Off** — Done. **Mosaic** selected.
2. **Core Scaffold + Account Sync** — Done. Tauri v2 + Rust `mail-core` + SQLite/FTS5 + OS keychain + Gmail/Outlook/IMAP sync.
3. **Core Email Experience & Paid-Client Workflows** — Done. Unified inbox, threads, reading (spy-pixel blocking), compose/reply/archive, search, shortcuts, Snooze, Send Later, Follow-up reminders, Undo Send, `Ctrl+K`, templates/snippets, unsubscribe, tray runner.

### Staff Collaboration & Planning Roadmap (see [COLLABORATION_SPEC.md](COLLABORATION_SPEC.md))
4. **Phase A — Staff-Ready Core**: One-click IONOS / Pigment Staff preset, macOS desktop build target, and **Settings → Developer Tools** gate for `AgentPort`/MCP.
5. **Phase B — Staff Sync & Email Collaboration**: Decoupled lightweight Staff Sync service (HTTP+SSE), Selective Thread Sharing & Handoff, internal comments, Draft Lock & Handoff, Shared Templates, Shared Show Contacts + sidebar Contact Card, and Pinned Staff Links.
6. **Phase C — Calendar & Meetings**: Pigment Shared Calendar, `.ics` email RSVP, read-only personal `.ics` overlays, 1-click availability slot insertion, Meeting Docs with recurring action-item carryover, and Discord meeting deep-links + recap export.
7. **Phase D — Action Items & Operations Workbook Import**: Shared Action Items view with department tags, show-weekend room schedule integration, email/meeting deep links, and `.xlsx`/CSV import & export.
