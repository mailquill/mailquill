ALTER TABLE user_settings ADD COLUMN load_external_images INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS image_sender_allowlist (
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sender     TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (user_id, sender)
);
