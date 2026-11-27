-- Local-first SQLite tables for personal Calendar events, inline .ics invites,
-- and read-only Personal .ics Overlays.

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
    created_by TEXT NOT NULL DEFAULT 'me',
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_calendar_event_start ON calendar_event(start_at ASC);
CREATE INDEX IF NOT EXISTS idx_calendar_event_series ON calendar_event(series_id);

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

-- Strictly local-only personal .ics overlay feeds.
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
