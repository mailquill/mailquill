CREATE TABLE IF NOT EXISTS phishing_analysis (
    message_id   TEXT NOT NULL PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    score        INTEGER NOT NULL,
    verdict      TEXT    NOT NULL,
    checks_json  TEXT    NOT NULL DEFAULT '{}',
    analysed_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
