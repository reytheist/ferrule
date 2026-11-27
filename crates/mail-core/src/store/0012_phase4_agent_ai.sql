-- Phase 4: Agent permissions, undoable agent activity log.

CREATE TABLE IF NOT EXISTS agent_account_permission (
    account_id BLOB PRIMARY KEY REFERENCES account(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    allow_read INTEGER NOT NULL DEFAULT 1 CHECK (allow_read IN (0, 1)),
    allow_triage INTEGER NOT NULL DEFAULT 1 CHECK (allow_triage IN (0, 1)),
    allow_drafts INTEGER NOT NULL DEFAULT 1 CHECK (allow_drafts IN (0, 1)),
    blocked_folders_json TEXT NOT NULL DEFAULT '[]',
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_activity_log (
    id BLOB PRIMARY KEY,
    agent_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    summary TEXT NOT NULL,
    account_id BLOB REFERENCES account(id) ON DELETE SET NULL,
    thread_id BLOB REFERENCES thread(id) ON DELETE SET NULL,
    undo_payload_json TEXT,
    undone INTEGER NOT NULL DEFAULT 0 CHECK (undone IN (0, 1)),
    undone_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_activity_created ON agent_activity_log(created_at DESC);
