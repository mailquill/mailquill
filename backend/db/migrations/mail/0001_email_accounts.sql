CREATE TABLE IF NOT EXISTS email_accounts (
    id                  TEXT    NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    display_name        TEXT    NOT NULL,
    primary_email       TEXT    NOT NULL,
    imap_host           TEXT    NOT NULL,
    imap_port           INTEGER NOT NULL,
    imap_auth_scheme    TEXT    NOT NULL,
    smtp_host           TEXT    NOT NULL,
    smtp_port           INTEGER NOT NULL,
    smtp_auth_scheme    TEXT    NOT NULL,
    credentials_encrypted BLOB  NOT NULL,
    sync_interval_secs  INTEGER NOT NULL DEFAULT 300,
    body_sync_mode      TEXT    NOT NULL DEFAULT 'headers_only',
    created_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
