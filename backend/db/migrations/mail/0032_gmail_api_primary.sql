-- Prefer the Gmail REST API for Google OAuth accounts.
--
-- Existing gmail_imap accounts used IMAP UIDs and localized IMAP folder paths.
-- The Gmail API provider uses provider message ids bridged through
-- remote_message_ids and label ids such as INBOX/SENT/DRAFT as folder paths, so
-- the local mail cache must be rebuilt when switching backends.

DELETE FROM message_bodies WHERE message_id IN (
    SELECT m.id FROM messages m JOIN email_accounts a ON a.id = m.account_id
    WHERE a.provider_kind = 'gmail_imap'
);
DELETE FROM attachments WHERE message_id IN (
    SELECT m.id FROM messages m JOIN email_accounts a ON a.id = m.account_id
    WHERE a.provider_kind = 'gmail_imap'
);
DELETE FROM phishing_analysis WHERE message_id IN (
    SELECT m.id FROM messages m JOIN email_accounts a ON a.id = m.account_id
    WHERE a.provider_kind = 'gmail_imap'
);
DELETE FROM messages WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind = 'gmail_imap'
);
DELETE FROM remote_message_ids WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind = 'gmail_imap'
);
DELETE FROM folders WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind = 'gmail_imap'
);

UPDATE email_accounts SET provider_kind = 'gmail_api' WHERE provider_kind = 'gmail_imap';

INSERT INTO messages_fts(messages_fts) VALUES('delete-all');
INSERT INTO messages_fts(rowid, subject, from_addr, body_text)
SELECT rowid, COALESCE(subject, ''), COALESCE(from_addr, ''), ''
FROM messages;
