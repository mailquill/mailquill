CREATE TABLE IF NOT EXISTS contact_keys (
    id               TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    email            TEXT NOT NULL,
    public_key_data  TEXT NOT NULL,
    source           TEXT NOT NULL,
    fingerprint      TEXT NOT NULL,
    fetched_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(email, fingerprint)
);
