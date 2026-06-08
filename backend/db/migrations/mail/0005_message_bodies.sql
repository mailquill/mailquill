CREATE TABLE IF NOT EXISTS message_bodies (
    message_id              TEXT NOT NULL PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    blob_key                TEXT NOT NULL,
    size_bytes              INTEGER,
    size_bytes_uncompressed INTEGER,
    fetched_at              TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
