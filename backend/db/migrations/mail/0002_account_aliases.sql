CREATE TABLE IF NOT EXISTS account_aliases (
    id           TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    account_id   TEXT NOT NULL REFERENCES email_accounts(id) ON DELETE CASCADE,
    email        TEXT NOT NULL,
    display_name TEXT,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_alias_account ON account_aliases(account_id);
