# Pigment Staff Onboarding Guide

Get set up with Ferrule in about 5 minutes. Ferrule runs locally on your computer with low memory usage, keeps all passwords and tokens in your operating system keychain, and keeps your personal email spaces completely separate from Pigment Staff collaboration.

## 1. Install Ferrule

Sign in with your Pigment staff account at **https://pigmentshow.us/ferrule** to download the latest release installer for your platform:

- **Windows**: Run the `.msi` or `.exe` installer and launch **Ferrule** from the Start menu.
- **macOS**: Open the `.dmg`, drag **Ferrule** into Applications, and launch it.
- **Linux**: Run the `.AppImage` (mark executable with `chmod +x`) or install the `.deb` package.

> Tip: Ferrule stays resident in your system tray so scheduled sends, snoozes, and live staff sync continue in the background.

## 2. Connect Your Pigment Mailbox (1-Click IONOS Setup)

1. Click **Add Account** in the left Account Space switcher (or open **Settings**).
2. Choose the **Pigment Staff (IONOS)** preset.
3. Enter your Pigment role email address (for example `info@pigmentshow.us`, `sponsors@pigmentshow.us`, `exhibition@pigmentshow.us`, `marketing@pigmentshow.us`, or `volunteers@pigmentshow.us`) and your IONOS mailbox password.
4. Click **Add Account**. Ferrule automatically configures `imap.ionos.com:993` (TLS) and `smtp.ionos.com:587` (STARTTLS) and saves your password strictly in your OS keychain.

**Identity guard**: Pigment Staff spaces are color-coded and lock the **From** address to that space. If you also add a personal Gmail, Outlook, or iCloud account, personal spaces block accidental staff sharing by default.

## 3. Connect Pigment Staff Sync

Staff Sync lets the team collaborate on shared threads, reply drafts, show contacts, templates, the shared calendar, and action items without sharing IMAP logins.

1. Ask the Show Director for two values (the server URL is also shown on **https://pigmentshow.us/ferrule**):
   - **Staff Sync Server URL** (`https://pigmentshow.us/api/ferrule`, or your self-hosted sync URL)
   - **Invite / Staff Token**
2. Open **Settings -> Pigment Staff Sync**.
3. Paste the **Staff Sync Server URL** and **Invite / Staff Token**.
4. Fill in **Your Name**, **@handle** (for `@mentions`), **Pigment Email**, and **Role**.
5. Click **Connect Staff Sync**.

Your invite token is stored strictly in your OS keychain (Windows Credential Manager, macOS Keychain, or Linux Secret Service), never in SQLite or browser storage.

## 4. Share a Thread and Hand Off a Draft

1. Open any email thread inside a **Pigment Staff** space.
2. Click **Share with Staff** to publish a sanitized snapshot (tracking pixels stripped, no IMAP credentials shared) into **Shared with Me**.
3. Use the right-hand collaboration sidebar to coordinate:
   - **Hand Off**: Assign an owner (`@handle`) and set status to **Open**, **Waiting**, or **Done**.
   - **Internal Comments**: Leave staff-only notes using `@handle` mentions. External recipients never see these comments.
   - **Draft Lock & Handoff**: Only one person edits a shared reply at a time:
     - Click **Claim** to start editing (or **Take Over** if someone stepped away).
     - Click **Request Review** to tag a teammate for a second pair of eyes.
     - Once reviewed, click **Approve**, then load the approved reply directly into your composer to send from the Pigment mailbox.

## 5. Shared Calendar, Meeting Docs, and Discord

- **Calendar**: Open **Calendar** from the sidebar to switch between Day, Week, and Month views, or use natural-language quick-add to schedule staff calls and show milestones.
- **Inline `.ics` RSVP**: When a vendor or sponsor sends a calendar invite by email, RSVP directly from the reading pane and add it to the Pigment calendar in one click.
- **Personal Busy Overlays**: Add read-only `.ics` feeds from your personal calendar so Ferrule can suggest open availability slots without exposing personal event details to staff.
- **Meeting Docs**: Every meeting event can hold an attached agenda and notes doc. Unchecked action items automatically carry forward into the next occurrence of a recurring meeting and appear in **Action Items**.
- **Discord**: Click the Discord link on a meeting to jump straight into the staff voice/text channel, or use **Copy / Post Meeting Recap** to share a formatted Markdown summary to Discord.

## 6. What the Offline and Pending Badges Mean

Because Ferrule is local-first, you can keep commenting, updating handoffs, editing drafts, and checking off action items even if your Wi-Fi drops or the sync server restarts:

| Badge | Location | What it means |
|---|---|---|
| `Offline` or `Offline (N pending)` | **Shared with Me** header & **Settings -> Pigment Staff Sync** | The sync server is unreachable right now. Your `N` local edits are safely stored in your local SQLite outbox (`staff_outbox`) and will sync automatically in order as soon as the connection returns. |
| `N pending` or `Syncing (N pending)` | **Shared with Me** header & **Settings -> Pigment Staff Sync** | Ferrule is actively flushing queued local edits to the sync server. |
| `Sync error` or `Sync error (N pending)` | **Shared with Me** header & **Settings -> Pigment Staff Sync** | The server rejected the request (most commonly after a staff token rotation, HTTP `401`). None of your queued edits are lost. Open **Settings -> Pigment Staff Sync**, reconnect with the updated token, and click **Sync Now** to flush the queue. |
