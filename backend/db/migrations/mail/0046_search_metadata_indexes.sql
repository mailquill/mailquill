-- Index-backed free-text search over message metadata.
--
-- `messages_fts` covers subject, sender and body, so the search handler used to
-- add `LIKE '%term%'` scans over the whole `messages` table for everything
-- else. These two indexes replace those scans:
--
--   * `messages_addr_fts`: recipients (To/Cc), word-tokenised like the main
--     index, so a term matches from the start of a name or address part
--     (`mail` finds `mail@…` and `…@mail.example`, not every `gmail.com`).
--   * `messages_subject_tri`: trigram index over the subject for substring
--     matches, which German compounds need (`rechnung` finds `Abrechnung`).
--
-- Both are external-content tables over `messages` (no second copy of the
-- text) and are kept current by the triggers below, so no writer has to know
-- about them.
CREATE VIRTUAL TABLE IF NOT EXISTS messages_addr_fts USING fts5(
    to_addrs,
    cc_addrs,
    content='messages',
    content_rowid='rowid'
);

CREATE VIRTUAL TABLE IF NOT EXISTS messages_subject_tri USING fts5(
    subject,
    content='messages',
    content_rowid='rowid',
    tokenize='trigram remove_diacritics 1'
);

CREATE TRIGGER IF NOT EXISTS messages_search_index_after_insert
AFTER INSERT ON messages
BEGIN
    INSERT INTO messages_addr_fts(rowid, to_addrs, cc_addrs)
    VALUES (NEW.rowid, NEW.to_addrs, NEW.cc_addrs);
    INSERT INTO messages_subject_tri(rowid, subject)
    VALUES (NEW.rowid, NEW.subject);
END;

CREATE TRIGGER IF NOT EXISTS messages_search_index_after_delete
AFTER DELETE ON messages
BEGIN
    INSERT INTO messages_addr_fts(messages_addr_fts, rowid, to_addrs, cc_addrs)
    VALUES ('delete', OLD.rowid, OLD.to_addrs, OLD.cc_addrs);
    INSERT INTO messages_subject_tri(messages_subject_tri, rowid, subject)
    VALUES ('delete', OLD.rowid, OLD.subject);
END;

-- Header sync upserts rewrite these columns with unchanged values on every
-- pass; the WHEN clauses keep that from churning the indexes.
CREATE TRIGGER IF NOT EXISTS messages_search_index_after_addr_update
AFTER UPDATE OF to_addrs, cc_addrs ON messages
WHEN OLD.to_addrs IS NOT NEW.to_addrs OR OLD.cc_addrs IS NOT NEW.cc_addrs
BEGIN
    INSERT INTO messages_addr_fts(messages_addr_fts, rowid, to_addrs, cc_addrs)
    VALUES ('delete', OLD.rowid, OLD.to_addrs, OLD.cc_addrs);
    INSERT INTO messages_addr_fts(rowid, to_addrs, cc_addrs)
    VALUES (NEW.rowid, NEW.to_addrs, NEW.cc_addrs);
END;

CREATE TRIGGER IF NOT EXISTS messages_search_index_after_subject_update
AFTER UPDATE OF subject ON messages
WHEN OLD.subject IS NOT NEW.subject
BEGIN
    INSERT INTO messages_subject_tri(messages_subject_tri, rowid, subject)
    VALUES ('delete', OLD.rowid, OLD.subject);
    INSERT INTO messages_subject_tri(rowid, subject)
    VALUES (NEW.rowid, NEW.subject);
END;

-- Index the messages that already exist.
INSERT INTO messages_addr_fts(messages_addr_fts) VALUES ('rebuild');
INSERT INTO messages_subject_tri(messages_subject_tri) VALUES ('rebuild');
