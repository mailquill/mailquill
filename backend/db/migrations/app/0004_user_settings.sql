CREATE TABLE IF NOT EXISTS user_settings (
    user_id                       TEXT    NOT NULL PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    body_sync_mode                TEXT    NOT NULL DEFAULT 'headers_only',
    pgp_discovery_wkd_enabled     INTEGER NOT NULL DEFAULT 1,
    pgp_discovery_keyserver_enabled INTEGER NOT NULL DEFAULT 1,
    sign_by_default               INTEGER NOT NULL DEFAULT 0,
    always_encrypt                INTEGER NOT NULL DEFAULT 0
);
