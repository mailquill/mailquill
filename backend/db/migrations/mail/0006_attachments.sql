CREATE TABLE IF NOT EXISTS attachments (
    id           TEXT    NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    message_id   TEXT    NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename     TEXT,
    content_type TEXT    NOT NULL DEFAULT 'application/octet-stream',
    size_bytes   INTEGER,
    blob_key     TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_attach_message ON attachments(message_id);
