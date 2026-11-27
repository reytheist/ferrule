-- Phase 3: Saved searches, sender screener, VIP senders, labels, and client-side rules.

CREATE TABLE IF NOT EXISTS saved_search (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    query TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sender_decision (
    address TEXT PRIMARY KEY,
    decision TEXT NOT NULL CHECK (decision IN ('inbox', 'paper_trail', 'feed', 'blocked')),
    display_name TEXT,
    decided_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS vip_sender (
    address TEXT PRIMARY KEY,
    display_name TEXT,
    added_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS label (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT NOT NULL DEFAULT 'azure',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS thread_label (
    thread_id BLOB NOT NULL REFERENCES thread(id) ON DELETE CASCADE,
    label_id BLOB NOT NULL REFERENCES label(id) ON DELETE CASCADE,
    PRIMARY KEY (thread_id, label_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_label_label ON thread_label(label_id);

CREATE TABLE IF NOT EXISTS mail_rule (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    match_all INTEGER NOT NULL DEFAULT 1 CHECK (match_all IN (0, 1)),
    position INTEGER NOT NULL DEFAULT 0,
    conditions_json TEXT NOT NULL,
    actions_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
