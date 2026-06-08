CREATE TABLE IF NOT EXISTS folders (
    id           TEXT    NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    account_id   TEXT    NOT NULL REFERENCES email_accounts(id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    full_path    TEXT    NOT NULL,
    folder_type  TEXT    NOT NULL DEFAULT 'CUSTOM',
    uidvalidity  INTEGER,
    last_uid     INTEGER,
    unread_count INTEGER NOT NULL DEFAULT 0,
    UNIQUE(account_id, full_path)
);

CREATE INDEX IF NOT EXISTS idx_folder_account ON folders(account_id);
