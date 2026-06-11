-- Mailbox backend per account: 'imap' (default), 'gmail_api', 'outlook_api'.
ALTER TABLE email_accounts ADD COLUMN provider_kind TEXT NOT NULL DEFAULT 'imap';

-- API providers (Gmail / Microsoft Graph) address messages by opaque string
-- ids while the sync pipeline uses per-folder integer uids — this table is the
-- stable mapping between the two. IMAP accounts don't use it.
CREATE TABLE IF NOT EXISTS remote_message_ids (
    account_id  TEXT    NOT NULL REFERENCES email_accounts(id) ON DELETE CASCADE,
    folder_path TEXT    NOT NULL,
    uid         INTEGER NOT NULL,
    remote_id   TEXT    NOT NULL,
    PRIMARY KEY (account_id, folder_path, uid),
    UNIQUE (account_id, folder_path, remote_id)
);
