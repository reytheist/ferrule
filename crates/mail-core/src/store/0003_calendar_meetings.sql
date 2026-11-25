-- Phase C: Local-first SQLite tables for Pigment Shared Calendar, Meeting Docs,
-- shared Action Items (shared with Phase D), inline .ics invites, and local-only
-- read-only Personal .ics Overlays.
-- Discord webhook URLs are NEVER stored here; they live strictly in SecretStore.

CREATE TABLE IF NOT EXISTS calendar_event (
    id BLOB PRIMARY KEY,
    series_id TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    start_at INTEGER NOT NULL,
    end_at INTEGER NOT NULL,
    all_day INTEGER NOT NULL DEFAULT 0 CHECK (all_day IN (0, 1)),
    kind TEXT NOT NULL DEFAULT 'meeting' CHECK (kind IN ('meeting', 'milestone', 'deadline', 'room_schedule', 'show_event')),
    department TEXT,
    location TEXT,
    recurrence TEXT NOT NULL DEFAULT 'none' CHECK (recurrence IN ('none', 'weekly', 'biweekly', 'monthly')),
    discord_url TEXT,
    attendees_json TEXT NOT NULL DEFAULT '[]',
    source_ics_uid TEXT,
    created_by TEXT NOT NULL DEFAULT 'staff',
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_calendar_event_start ON calendar_event(start_at ASC);
CREATE INDEX IF NOT EXISTS idx_calendar_event_series ON calendar_event(series_id);

CREATE TABLE IF NOT EXISTS meeting_doc (
    event_id BLOB PRIMARY KEY REFERENCES calendar_event(id) ON DELETE CASCADE,
    series_id TEXT,
    agenda_json TEXT NOT NULL DEFAULT '[]',
    notes_markdown TEXT NOT NULL DEFAULT '',
    notes_editor_handle TEXT,
    notes_editor_name TEXT,
    rev INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS action_item (
    id BLOB PRIMARY KEY,
    title TEXT NOT NULL,
    department TEXT,
    milestone TEXT,
    assignee_handle TEXT,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done')),
    due_at INTEGER,
    start_time TEXT,
    end_time TEXT,
    location TEXT,
    cost_note TEXT,
    calendar_event_id BLOB REFERENCES calendar_event(id) ON DELETE SET NULL,
    source_event_id BLOB REFERENCES calendar_event(id) ON DELETE SET NULL,
    source_series_id TEXT,
    source_thread_id BLOB REFERENCES thread(id) ON DELETE SET NULL,
    created_by TEXT NOT NULL DEFAULT 'staff',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_action_item_event ON action_item(source_event_id);
CREATE INDEX IF NOT EXISTS idx_action_item_series ON action_item(source_series_id, status);
CREATE INDEX IF NOT EXISTS idx_action_item_status ON action_item(status, due_at ASC);

CREATE TABLE IF NOT EXISTS message_invite (
    message_id BLOB NOT NULL REFERENCES message(id) ON DELETE CASCADE,
    uid TEXT NOT NULL,
    summary TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    location TEXT,
    organizer_name TEXT,
    organizer_email TEXT NOT NULL DEFAULT '',
    start_at INTEGER NOT NULL,
    end_at INTEGER NOT NULL,
    all_day INTEGER NOT NULL DEFAULT 0 CHECK (all_day IN (0, 1)),
    rsvp_status TEXT NOT NULL DEFAULT 'needs_action' CHECK (rsvp_status IN ('needs_action', 'accepted', 'tentative', 'declined')),
    added_event_id BLOB REFERENCES calendar_event(id) ON DELETE SET NULL,
    PRIMARY KEY (message_id, uid)
);

-- Strictly local-only personal .ics overlay feeds (never synced to Staff Sync).
CREATE TABLE IF NOT EXISTS ics_overlay (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT 'violet',
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    last_fetched_at INTEGER,
    last_error TEXT
);

CREATE TABLE IF NOT EXISTS ics_overlay_event (
    overlay_id BLOB NOT NULL REFERENCES ics_overlay(id) ON DELETE CASCADE,
    uid TEXT NOT NULL,
    summary TEXT NOT NULL,
    start_at INTEGER NOT NULL,
    end_at INTEGER NOT NULL,
    all_day INTEGER NOT NULL DEFAULT 0 CHECK (all_day IN (0, 1)),
    PRIMARY KEY (overlay_id, uid, start_at)
);

CREATE INDEX IF NOT EXISTS idx_ics_overlay_event_range ON ics_overlay_event(start_at ASC, end_at ASC);
