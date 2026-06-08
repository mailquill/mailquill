-- FTS5 virtual table for full-text search (task 4.18)
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    subject,
    from_addr,
    body_text,
    content='',
    contentless_delete=1
);
