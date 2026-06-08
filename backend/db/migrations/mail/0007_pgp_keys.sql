CREATE TABLE IF NOT EXISTS pgp_keys (
    id                          TEXT    NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    fingerprint                 TEXT    NOT NULL UNIQUE,
    uid                         TEXT    NOT NULL,
    public_key_armored          TEXT    NOT NULL,
    private_key_encrypted_blob  BLOB    NOT NULL,
    is_primary                  INTEGER NOT NULL DEFAULT 0,
    created_at                  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
