# Ferrule — Pigment Staff Collaboration & Planning Spec

This specification captures the settled product and architecture decisions for expanding Ferrule from a single-user desktop email client into a non-commercial **email, calendar, meeting, and operations collaboration client** for the **Pigment Miniatures Show** staff.

See [BLUEPRINT.md](BLUEPRINT.md) for the top-level product summary and [ARCHITECTURE.md](ARCHITECTURE.md) for the existing `mail-core` / Mosaic boundaries.

---

## 1. Staff Context & Privacy Model

- **Role-owned mailboxes**: Each staff member owns their own IONOS webmail account (`info@` for the Director, plus `sponsors@`, `exhibition@`, `marketing@`, `volunteers@`). **No IMAP mailboxes are shared across users.**
- **Optional personal email**: Staff may optionally add personal Gmail/Outlook/IMAP accounts in separate color-coded Mosaic Spaces.
- **Private by default**:
  - All email remains 100% local in SQLite (`mail-core`) unless a user explicitly invokes **Share with Staff** on a specific thread.
  - **Personal Space guardrail**: Non-Pigment Account Spaces have Staff Sharing disabled by default so a personal thread can never be accidentally shared with the show team.
  - All credentials (IONOS passwords, OAuth refresh tokens, Staff Sync session tokens) remain strictly in the OS keychain via `SecretStore`—never in SQLite or `localStorage`.
- **Zero AI clutter**: All Hermes/MCP/`AgentPort` capabilities are **disabled by default** and live strictly under **Settings → Developer Tools**.

---

## 2. Hybrid Sync Architecture

Ferrule keeps its local-first Rust + SQLite core (`mail-core`) and adds an **opt-in, decoupled Staff Sync client** talking to a lightweight standalone sync service.

- **Decoupled from `pigment2027`**: The sync service exposes a small, self-hostable **HTTP + SSE (Server-Sent Events)** contract with simple staff invite/token authentication. Ferrule does not depend on `pigment2027`'s commerce schema or Clerk auth.
- **Offline-first caching**: Shared threads, comments, drafts, calendar events, meeting docs, action items, shared templates, contacts, and pinned links are cached in local SQLite tables so the app stays instant and usable offline, syncing deltas on reconnect.
- **Synced entity boundary** (only these ever leave the local machine):
  1. Explicitly shared thread snapshots + internal thread comments + draft lock/handoff state.
  2. Shared Templates, Shared Show Contacts, and Pinned Staff Links.
  3. Pigment Shared Calendar events + Meeting Docs (agendas & notes).
  4. Shared Action Items.

---

## 3. Feature Specification by Phase

### Phase A — Staff-Ready Core

1. **One-Click IONOS / Pigment Staff Preset**
   - Add a **Pigment Staff (IONOS)** option at the top of the Add Account wizard.
   - Auto-configures IONOS IMAP/SMTP settings (`imap.ionos.com:993` TLS, `smtp.ionos.com:587` STARTTLS) so staff only enter their email address and password.
   - Marks the account as a **Pigment Staff Space** (eligible for Staff Sharing, distinct from personal spaces).
2. **macOS Desktop Target**
   - Enable macOS Tauri v2 build/packaging alongside Windows and Linux (`keyring`'s `apple-native` feature is already enabled in `crates/mail-core`).
3. **Developer Tools Gate (`AgentPort` / MCP)**
   - Add **Settings → Developer Tools** (off by default).
   - Gate any future local MCP server controls, `AgentPort` configuration, and the Agent Draft Approval Queue behind this toggle so standard staff UI has zero AI/agent surface.

---

### Phase B — Staff Sync & Email Collaboration

1. **Staff Workspace Connection & Directory**
   - Connect to the Staff Sync server URL with an invite/staff token stored in `SecretStore`.
   - Sync a lightweight **Staff Directory** (name, role, `@handle`, Pigment email) for `@mentions`, assignment dropdowns, and compose autocomplete.
2. **Selective Thread Sharing & Handoff**
   - From any thread in a Pigment Staff Space, click **Share with Staff** (choose specific teammates or All Staff).
   - Uploads a sanitized thread snapshot (headers, participants, messages) to the Staff Sync service so invited teammates see it in a **Shared with Me** view without needing IMAP access to the sender's mailbox.
   - Supports **Hand Off** (assigning a teammate as owner with status `Open / Waiting / Done`) and **Internal Comments** (private staff discussion sidebar beside the email thread, supporting `@mentions`).
3. **Draft Lock & Handoff**
   - On a shared thread, staff can collaborate on a reply using a **single-editor lock**:
     - Shows `"Editing: <Name>"` when someone holds the draft lock.
     - Actions: **Request Review** (releases lock and pings reviewer(s) in thread comments), **Take Over** (explicitly claims lock with confirmation), and **Approve** (marks ready for the mailbox owner to send from their locked "From" identity).
   - No live CRDT simultaneous typing—keeps state simple and conflict-free.
4. **Shared Show-Ops Resources**
   - **Shared Templates**: Team-wide canned responses (e.g., sponsor prospectus reply, exhibitor FAQ, volunteer onboarding), kept visually distinct from personal snippets.
   - **Shared Show Contacts**: Lightweight directory of sponsors, vendors, instructors, VIPs, volunteers, and venue contacts (name, email(s), role/category, tier/level, internal notes). When viewing an email from a known contact, display a compact **Contact Card** in the reading pane sidebar.
   - **Pinned Staff Links**: Shared list of quick links in the sidebar and `Ctrl+K` command bar (Pigment Admin, Google Drive Operations Workbook, Canva Marketing folder, venue floor plan, etc.).

---

### Phase C — Calendar & Meetings

1. **Pigment Shared Calendar**
   - Day / Week / Month views in the Mosaic shell backed by the Staff Sync service.
   - Supports recurring staff meetings, show milestones/payment deadlines, and show-weekend room schedules (`Where` room tags).
   - **Natural-language quick-add** (e.g., `"Staff sync Thu 7pm #operations"`) and keyboard navigation.
2. **Email ↔ Calendar Bridges**
   - **`.ics` Invite Handling**: Parse incoming `.ics` attachments inline in the reading pane with `Accept / Maybe / Decline` RSVP buttons and 1-click add to the Pigment calendar.
   - **Read-Only Personal `.ics` Overlays**: Staff can paste private read-only `.ics` feed URLs (Google Calendar, Apple iCloud, Outlook) to see personal busy blocks alongside Pigment events without OAuth setup.
   - **1-Click Availability Slot Insertion**: From the compose/reply dock, pick open windows from the calendar grid and insert formatted availability text into the email reply.
3. **Meeting Docs & Recurring Carryover**
   - Every Pigment calendar event can open an attached **Meeting Doc** containing:
     - **Agenda**: Collaborative bullet list where any staff member can add topics before the meeting.
     - **Notes**: Shared Markdown notes (section/lock-saved).
     - **Action Items**: Promote any bullet directly into the shared Action Items list (Phase D).
     - **Automatic Carryover**: Unfinished action items from a recurring meeting automatically surface in the next occurrence's agenda (Fellow-style).
4. **Discord Meeting Integration**
   - Calendar events support a **Discord Voice/Channel Link** (`discord://` or web URL) rendered as a 1-click **Join in Discord** button alongside the Meeting Doc.
   - **Copy / Post Recap to Discord**: Formats the meeting's key notes and newly created/updated action items as clean Discord Markdown (copy to clipboard, or optional Discord webhook post).

---

### Phase D — Action Items & Operations Workbook Split

1. **Action Items View**
   - Dedicated **Action Items** view in the left rail with filters: `Mine`, `Unassigned`, `By Dept.`, `By Meeting`, `Upcoming / Overdue`, and `All`.
   - Core fields modeled directly on `2027 Operations Workbook`:
     - `Title / Task`
     - `Dept.` (`marketing`, `sponsors`, `exhibition`, `volunteers`, `operations`, `workshops`, `shopping-list`, `code-monkeys`, `general-business`)
     - `Event / Milestone` (`What` — e.g., `PMS 2027`, `ReaperCon 2027`, `Baltimore Show 2027`)
     - `Assignee` (staff member)
     - `Status` (`Open` / `Done`)
     - `Due Date` + optional `Start Time` / `End Time` + `Where` (room) — items with dates/times can optionally appear on the Pigment Shared Calendar
     - Optional `Cost / $$$` note
     - **Source Link**: Deep link back to the originating email thread or Meeting Doc
2. **Operations Workbook Split Strategy**
   - **Imported into Ferrule** (via `.xlsx` / CSV importer and CSV exporter for backups):
     - `2027 Operations Workbook` tab → Action Items + Pigment Shared Calendar milestones/room schedule.
     - Contact rows from `Sponsors / Vendors`, `Instructors`, `VIPs`, and `Volunteers` → Shared Show Contacts.
   - **Kept in Google Drive** (surfaced via Pinned Staff Links):
     - Dense visual and matrix sheets that belong in a spreadsheet: `Floor Plan`, `Vendor MapPayment`, `Room Supplies`, `Class Schedule` grid, and the 26-column `Sponsors / Vendors` fulfillment checklist.

---

## 4. Explicit Non-Goals (Guardrails)

- **No commercial SaaS features**: No billing, multi-tenant org management, SSO/SAML, or SLA/response-time analytics.
- **No chat or voice replacement**: Staff continue meeting and chatting on Discord; Ferrule integrates via deep links and recap export rather than duplicating chat channels.
- **No audio recording or AI transcription**: Meeting notes are human-authored Markdown.
- **No public booking pages**: External scheduling links (e.g., Cal.com) remain external if ever needed.
- **No two-way Google Sheets or Google/Outlook Calendar write-back** in Phases A–D (read-only `.ics` overlays and `.xlsx`/CSV import/export keep maintenance minimal).
