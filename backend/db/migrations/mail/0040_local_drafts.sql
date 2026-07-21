ALTER TABLE messages
    ADD COLUMN is_local_draft INTEGER NOT NULL DEFAULT 0;

CREATE TABLE local_drafts (
    message_id       TEXT NOT NULL PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    to_addrs_json    TEXT NOT NULL DEFAULT '[]',
    cc_addrs_json    TEXT NOT NULL DEFAULT '[]',
    bcc_addrs_json   TEXT NOT NULL DEFAULT '[]',
    attachments_json TEXT NOT NULL DEFAULT '[]',
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
