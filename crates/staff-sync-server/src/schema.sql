-- Standalone SQLite schema for the Pigment Staff Sync Server.
-- Stores only explicitly synced staff collaboration entities (spec section 2).

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;

CREATE TABLE IF NOT EXISTS staff_member (
    id TEXT PRIMARY KEY,
    handle TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS shared_thread (
    id TEXT PRIMARY KEY,
    updated_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS shared_comment (
    id TEXT PRIMARY KEY,
    shared_thread_id TEXT NOT NULL REFERENCES shared_thread(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_srv_shared_comment_thread
    ON shared_comment(shared_thread_id, created_at ASC);

CREATE TABLE IF NOT EXISTS shared_draft (
    shared_thread_id TEXT PRIMARY KEY REFERENCES shared_thread(id) ON DELETE CASCADE,
    rev INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS shared_template (
    id TEXT PRIMARY KEY,
    updated_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS show_contact (
    id TEXT PRIMARY KEY,
    updated_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pinned_link (
    id TEXT PRIMARY KEY,
    updated_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS calendar_event (
    id TEXT PRIMARY KEY,
    updated_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS meeting_doc (
    event_id TEXT PRIMARY KEY,
    rev INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS action_item (
    id TEXT PRIMARY KEY,
    updated_at INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tombstone (
    entity_kind TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    deleted_at INTEGER NOT NULL,
    PRIMARY KEY (entity_kind, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_srv_tombstone_deleted_at
    ON tombstone(deleted_at ASC);
