-- Phase B: Local-first SQLite cache tables for Pigment Staff Sync & Collaboration.
-- Secrets (Staff Sync session/invite tokens) are NEVER stored here; they live
-- strictly in the OS keychain via SecretStore.

CREATE TABLE IF NOT EXISTS staff_workspace (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    server_url TEXT NOT NULL,
    staff_name TEXT NOT NULL,
    staff_handle TEXT NOT NULL,
    staff_email TEXT NOT NULL,
    staff_role TEXT NOT NULL,
    connected_at INTEGER NOT NULL,
    last_sync_at INTEGER,
    sync_status TEXT NOT NULL DEFAULT 'connected' CHECK (sync_status IN ('disconnected', 'connected', 'offline', 'error')),
    last_error TEXT
);

CREATE TABLE IF NOT EXISTS staff_member (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    role TEXT NOT NULL,
    handle TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    department TEXT,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS shared_thread (
    id BLOB PRIMARY KEY,
    source_thread_id BLOB,
    source_account_id BLOB,
    owner_email TEXT NOT NULL,
    owner_name TEXT NOT NULL,
    subject TEXT NOT NULL,
    snippet TEXT NOT NULL,
    participants_json TEXT NOT NULL,
    messages_json TEXT NOT NULL,
    visibility TEXT NOT NULL CHECK (visibility IN ('all_staff', 'specific')),
    invited_handles_json TEXT NOT NULL,
    assignee_handle TEXT,
    handoff_status TEXT NOT NULL DEFAULT 'open' CHECK (handoff_status IN ('open', 'waiting', 'done')),
    shared_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_shared_thread_source ON shared_thread(source_thread_id);
CREATE INDEX IF NOT EXISTS idx_shared_thread_updated ON shared_thread(updated_at DESC);

CREATE TABLE IF NOT EXISTS shared_comment (
    id BLOB PRIMARY KEY,
    shared_thread_id BLOB NOT NULL REFERENCES shared_thread(id) ON DELETE CASCADE,
    author_handle TEXT NOT NULL,
    author_name TEXT NOT NULL,
    body_text TEXT NOT NULL,
    mentions_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_shared_comment_thread ON shared_comment(shared_thread_id, created_at ASC);

CREATE TABLE IF NOT EXISTS shared_draft (
    shared_thread_id BLOB PRIMARY KEY REFERENCES shared_thread(id) ON DELETE CASCADE,
    subject TEXT NOT NULL,
    to_json TEXT NOT NULL,
    cc_json TEXT NOT NULL,
    body_text TEXT NOT NULL,
    editor_handle TEXT,
    editor_name TEXT,
    lock_state TEXT NOT NULL DEFAULT 'unlocked' CHECK (lock_state IN ('unlocked', 'editing', 'review_requested', 'approved')),
    review_requested_by TEXT,
    approved_by TEXT,
    approved_at INTEGER,
    rev INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS shared_template (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    shortcut TEXT,
    category TEXT NOT NULL DEFAULT 'general',
    subject TEXT NOT NULL,
    body_text TEXT NOT NULL,
    updated_by TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS show_contact (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    emails_json TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('sponsor', 'vendor', 'instructor', 'vip', 'volunteer', 'venue', 'other')),
    organization TEXT,
    tier TEXT,
    notes TEXT NOT NULL DEFAULT '',
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS pinned_link (
    id BLOB PRIMARY KEY,
    title TEXT NOT NULL,
    url TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'operations',
    description TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);
