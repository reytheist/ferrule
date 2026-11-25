PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA mmap_size = 0;

CREATE TABLE IF NOT EXISTS account (
    id BLOB PRIMARY KEY,
    display_name TEXT NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('gmail', 'outlook', 'imap')),
    color TEXT NOT NULL CHECK (color IN ('azure', 'berry', 'teal', 'amber', 'violet', 'emerald')),
    space_kind TEXT NOT NULL DEFAULT 'personal' CHECK (space_kind IN ('personal', 'pigment_staff')),
    email TEXT NOT NULL,
    position INTEGER NOT NULL,
    auth_json TEXT NOT NULL,
    sync_state_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS identity (
    id BLOB PRIMARY KEY,
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    address TEXT NOT NULL,
    display_name TEXT,
    is_primary INTEGER NOT NULL CHECK (is_primary IN (0, 1))
);

CREATE TABLE IF NOT EXISTS folder_cursor (
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    folder_name TEXT NOT NULL,
    uid_validity INTEGER NOT NULL DEFAULT 0,
    high_uid INTEGER NOT NULL DEFAULT 0,
    window_floor INTEGER NOT NULL,
    backfill_target INTEGER NOT NULL,
    PRIMARY KEY (account_id, folder_name)
);

CREATE TABLE IF NOT EXISTS thread (
    id BLOB PRIMARY KEY,
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    remote_key TEXT NOT NULL,
    subject TEXT NOT NULL,
    snippet TEXT NOT NULL,
    sender_name TEXT NOT NULL,
    sender_email TEXT NOT NULL,
    last_at INTEGER NOT NULL,
    unread_count INTEGER NOT NULL DEFAULT 0,
    message_count INTEGER NOT NULL DEFAULT 1,
    has_attachments INTEGER NOT NULL DEFAULT 0 CHECK (has_attachments IN (0, 1)),
    starred INTEGER NOT NULL DEFAULT 0 CHECK (starred IN (0, 1)),
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    trashed INTEGER NOT NULL DEFAULT 0 CHECK (trashed IN (0, 1)),
    category TEXT NOT NULL DEFAULT 'primary' CHECK (category IN ('primary', 'receipts', 'newsletters')),
    trackers_blocked INTEGER NOT NULL DEFAULT 0,
    UNIQUE (account_id, remote_key)
);

CREATE INDEX IF NOT EXISTS idx_thread_inbox
    ON thread(archived, trashed, last_at DESC, id);

CREATE INDEX IF NOT EXISTS idx_thread_account
    ON thread(account_id, archived, trashed, last_at DESC, id);

CREATE TABLE IF NOT EXISTS message (
    rowid INTEGER PRIMARY KEY AUTOINCREMENT,
    id BLOB NOT NULL UNIQUE,
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    thread_id BLOB NOT NULL REFERENCES thread(id) ON DELETE CASCADE,
    message_id_hdr TEXT NOT NULL,
    in_reply_to TEXT,
    from_name TEXT,
    from_email TEXT NOT NULL,
    to_json TEXT NOT NULL,
    cc_json TEXT NOT NULL,
    subject TEXT NOT NULL,
    date INTEGER NOT NULL,
    snippet TEXT NOT NULL,
    is_read INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0, 1)),
    body_state TEXT NOT NULL DEFAULT 'full' CHECK (body_state IN ('full', 'headers_only')),
    body_text TEXT NOT NULL,
    raw_html TEXT,
    unsub_json TEXT,
    UNIQUE (account_id, message_id_hdr)
);

CREATE INDEX IF NOT EXISTS idx_message_thread ON message(thread_id, date ASC);

CREATE VIRTUAL TABLE IF NOT EXISTS message_fts USING fts5(
    subject,
    sender,
    body_text,
    content = '',
    contentless_delete = 1,
    tokenize = 'unicode61 remove_diacritics 2',
    prefix = '2 3'
);

CREATE TABLE IF NOT EXISTS draft (
    id BLOB PRIMARY KEY,
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    from_identity_id BLOB NOT NULL REFERENCES identity(id),
    from_locked INTEGER NOT NULL CHECK (from_locked IN (0, 1)),
    reply_to_thread BLOB REFERENCES thread(id) ON DELETE SET NULL,
    to_json TEXT NOT NULL,
    cc_json TEXT NOT NULL,
    bcc_json TEXT NOT NULL,
    subject TEXT NOT NULL,
    body_text TEXT NOT NULL,
    rev INTEGER NOT NULL DEFAULT 1,
    origin_kind TEXT NOT NULL CHECK (origin_kind IN ('human', 'agent')),
    origin_agent TEXT,
    rationale TEXT,
    state_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS job (
    id BLOB PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('send', 'snooze', 'follow_up')),
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    thread_id BLOB REFERENCES thread(id) ON DELETE CASCADE,
    draft_id BLOB REFERENCES draft(id) ON DELETE CASCADE,
    due_at INTEGER NOT NULL,
    live INTEGER NOT NULL CHECK (live IN (0, 1)),
    state_json TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_job_due ON job(due_at) WHERE live = 1;

CREATE TABLE IF NOT EXISTS template (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    shortcut TEXT,
    subject TEXT NOT NULL,
    body_text TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sender_allow (
    account_id BLOB NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    address TEXT NOT NULL,
    PRIMARY KEY (account_id, address)
);

CREATE TABLE IF NOT EXISTS app_setting (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);
