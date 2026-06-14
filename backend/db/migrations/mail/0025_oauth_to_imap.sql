-- Switch Gmail/Outlook accounts from the provider APIs to IMAP/SMTP+XOAUTH2.
-- Outlook becomes plain IMAP; Gmail becomes the gmail_imap hybrid (IMAP for
-- mail, Gmail API only for label handling).
--
-- The old API providers assigned synthetic per-folder uids (1,2,3…); IMAP uids
-- are different, so the previously synced rows must be cleared or they would
-- coexist as duplicates with the freshly IMAP-synced rows. Child rows of
-- messages are deleted explicitly because a `PRAGMA foreign_keys` toggle is a
-- no-op inside the migration transaction, so ON DELETE CASCADE can't be relied
-- on here.

-- 1) Drop synced state for the affected (API) accounts.
DELETE FROM message_bodies WHERE message_id IN (
    SELECT m.id FROM messages m JOIN email_accounts a ON a.id = m.account_id
    WHERE a.provider_kind IN ('gmail_api', 'outlook_api')
);
DELETE FROM attachments WHERE message_id IN (
    SELECT m.id FROM messages m JOIN email_accounts a ON a.id = m.account_id
    WHERE a.provider_kind IN ('gmail_api', 'outlook_api')
);
DELETE FROM phishing_analysis WHERE message_id IN (
    SELECT m.id FROM messages m JOIN email_accounts a ON a.id = m.account_id
    WHERE a.provider_kind IN ('gmail_api', 'outlook_api')
);
DELETE FROM messages WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind IN ('gmail_api', 'outlook_api')
);
DELETE FROM remote_message_ids WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind IN ('gmail_api', 'outlook_api')
);
UPDATE folders SET last_uid = NULL, uidvalidity = NULL, unread_count = 0
WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind IN ('gmail_api', 'outlook_api')
);

-- 2) Flip the provider kind.
UPDATE email_accounts SET provider_kind = 'imap'        WHERE provider_kind = 'outlook_api';
UPDATE email_accounts SET provider_kind = 'gmail_imap'  WHERE provider_kind = 'gmail_api';

-- 3) Rebuild the FTS index so no stale rows survive a messages rowid being
-- reused by a future insert. (`delete-all` is the FTS5 reset for a contentless
-- table; reinsert subject + from_addr like migration 0024.)
INSERT INTO messages_fts(messages_fts) VALUES('delete-all');
INSERT INTO messages_fts(rowid, subject, from_addr, body_text)
SELECT rowid, COALESCE(subject, ''), COALESCE(from_addr, ''), ''
FROM messages;
