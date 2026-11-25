-- Phase E: Durable offline outbox for Pigment Staff Sync mutations.
-- Queues staff edits made while offline and replays them in FIFO order on reconnect.

CREATE TABLE IF NOT EXISTS staff_outbox (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    op_kind TEXT NOT NULL,
    entity_key TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_staff_outbox_entity ON staff_outbox(op_kind, entity_key);
