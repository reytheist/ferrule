-- Phase 2: Offline action queue, attachments, draft attachments, and named signatures.

CREATE TABLE IF NOT EXISTS offline_action (
    id BLOB PRIMARY KEY,
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    thread_id BLOB REFERENCES thread(id) ON DELETE CASCADE,
    action_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'in_flight', 'done', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_retry_at INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_offline_action_pending
    ON offline_action(account_id, status, next_retry_at, created_at);

CREATE TABLE IF NOT EXISTS attachment (
    id BLOB PRIMARY KEY,
    message_id BLOB NOT NULL REFERENCES message(id) ON DELETE CASCADE,
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    mime TEXT NOT NULL,
    size INTEGER NOT NULL DEFAULT 0,
    content_id TEXT,
    is_inline INTEGER NOT NULL DEFAULT 0 CHECK (is_inline IN (0, 1)),
    part_id TEXT,
    data BLOB
);

CREATE INDEX IF NOT EXISTS idx_attachment_message ON attachment(message_id);

CREATE TABLE IF NOT EXISTS draft_attachment (
    id BLOB PRIMARY KEY,
    draft_id BLOB NOT NULL REFERENCES draft(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    mime TEXT NOT NULL,
    size INTEGER NOT NULL DEFAULT 0,
    data BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_draft_attachment_draft ON draft_attachment(draft_id);

CREATE TABLE IF NOT EXISTS signature (
    id BLOB PRIMARY KEY,
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    body_html TEXT NOT NULL,
    body_text TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_signature_account
    ON signature(account_id, is_default DESC, name ASC);
