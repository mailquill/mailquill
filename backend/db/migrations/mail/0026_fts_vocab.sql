-- Term vocabulary of the FTS index, used for typo-tolerant search: the search
-- handler matches a query word against these terms by edit distance and expands
-- the FTS query with the closest ones (so "Decatlon" also finds "Decathlon").
-- `fts5vocab` is a read-only view over messages_fts; no extra storage.
CREATE VIRTUAL TABLE IF NOT EXISTS messages_vocab USING fts5vocab('messages_fts', 'row');
