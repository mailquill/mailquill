-- Gmail mail sync uses IMAP/XOAUTH2 plus IDLE. The Gmail API remains available
-- to the hybrid provider for label operations, addressed through X-GM-MSGID.
--
-- The REST provider used synthetic per-label uids and Gmail label ids as folder
-- paths. IMAP exposes server UIDs and selectable mailbox paths, so its cache
-- must be rebuilt once when switching transports.

DELETE FROM message_bodies WHERE message_id IN (
    SELECT m.id FROM messages m JOIN email_accounts a ON a.id = m.account_id
    WHERE a.provider_kind = 'gmail_api'
);
DELETE FROM attachments WHERE message_id IN (
    SELECT m.id FROM messages m JOIN email_accounts a ON a.id = m.account_id
    WHERE a.provider_kind = 'gmail_api'
);
DELETE FROM phishing_analysis WHERE message_id IN (
    SELECT m.id FROM messages m JOIN email_accounts a ON a.id = m.account_id
    WHERE a.provider_kind = 'gmail_api'
);
DELETE FROM messages WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind = 'gmail_api'
);
DELETE FROM remote_message_ids WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind = 'gmail_api'
);
DELETE FROM folders WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind = 'gmail_api'
);

UPDATE email_accounts
SET provider_kind = 'gmail_imap', sync_mode = 'idle'
WHERE provider_kind = 'gmail_api';

INSERT INTO messages_fts(messages_fts) VALUES('delete-all');
INSERT INTO messages_fts(rowid, subject, from_addr, body_text)
SELECT rowid, COALESCE(subject, ''), COALESCE(from_addr, ''), ''
FROM messages;
