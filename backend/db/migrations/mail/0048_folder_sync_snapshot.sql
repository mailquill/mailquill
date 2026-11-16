-- Server state of a folder (IMAP STATUS on a CONDSTORE server) as it was when
-- the folder was last synced completely. A sync cycle that finds the same
-- UIDVALIDITY, UIDNEXT, message count and HIGHESTMODSEQ again skips the
-- folder; otherwise the modification sequence bounds the flag fetch to the
-- messages that changed since. All NULL until the first complete sync, and on
-- servers without CONDSTORE.
ALTER TABLE folders ADD COLUMN synced_modseq INTEGER;
ALTER TABLE folders ADD COLUMN synced_uidnext INTEGER;
ALTER TABLE folders ADD COLUMN synced_exists INTEGER;
