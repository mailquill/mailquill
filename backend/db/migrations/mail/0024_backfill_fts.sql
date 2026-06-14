-- Backfill the FTS index. The sync code populated messages_fts with
-- `ON CONFLICT ...`, which FTS5 rejects ("UPSERT not implemented for virtual
-- table"); the error was swallowed (`let _ =`), so the index stayed empty and
-- search always returned zero hits. Seed every existing message with its
-- subject + from_addr now; body_text is filled in on the next body fetch.
-- INSERT OR IGNORE leaves any row already present (e.g. with body) untouched.
INSERT OR IGNORE INTO messages_fts(rowid, subject, from_addr, body_text)
SELECT rowid, COALESCE(subject, ''), COALESCE(from_addr, ''), ''
FROM messages;
