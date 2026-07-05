PRAGMA foreign_keys = OFF;

DROP TABLE IF EXISTS contacts_fts;
DROP TRIGGER IF EXISTS contacts_ai;
DROP TRIGGER IF EXISTS contacts_ad;
DROP TRIGGER IF EXISTS contacts_au;
DROP TABLE IF EXISTS contact_group_members;
DROP TABLE IF EXISTS contact_groups;
DROP TABLE IF EXISTS contacts;
DROP TABLE IF EXISTS contact_accounts;

CREATE TABLE contact_accounts (
    id                    TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    display_name          TEXT NOT NULL,
    type                  TEXT NOT NULL CHECK (type IN ('cardav', 'graph', 'google')),
    base_url              TEXT,
    auth_scheme           TEXT NOT NULL DEFAULT 'basic' CHECK (auth_scheme IN ('basic', 'oauth2')),
    credentials_encrypted BLOB NOT NULL,
    sync_token            TEXT,
    last_synced_at        TEXT,
    sync_status           TEXT NOT NULL DEFAULT 'idle' CHECK (sync_status IN ('idle', 'syncing', 'error')),
    sync_error            TEXT,
    created_at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE contacts (
    id             TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    account_id     TEXT NOT NULL REFERENCES contact_accounts(id) ON DELETE CASCADE,
    uid            TEXT NOT NULL,
    display_name   TEXT,
    given_name     TEXT,
    family_name    TEXT,
    org            TEXT,
    title          TEXT,
    emails         TEXT NOT NULL DEFAULT '[]',
    phones         TEXT NOT NULL DEFAULT '[]',
    addresses      TEXT NOT NULL DEFAULT '[]',
    notes          TEXT,
    photo_blob_key TEXT,
    raw_vcard      TEXT,
    synced_at      TEXT,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(account_id, uid)
);

CREATE TABLE contact_groups (
    id         TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    account_id TEXT NOT NULL REFERENCES contact_accounts(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    UNIQUE(account_id, name)
);

CREATE TABLE contact_group_members (
    contact_id TEXT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    group_id   TEXT NOT NULL REFERENCES contact_groups(id) ON DELETE CASCADE,
    PRIMARY KEY(contact_id, group_id)
);

CREATE INDEX idx_contact_accounts_type ON contact_accounts(type);
CREATE INDEX idx_contacts_account ON contacts(account_id);
CREATE INDEX idx_contacts_display_name ON contacts(display_name COLLATE NOCASE);
CREATE INDEX idx_contacts_name_parts ON contacts(given_name COLLATE NOCASE, family_name COLLATE NOCASE);

CREATE VIRTUAL TABLE contacts_fts USING fts5(
    display_name,
    given_name,
    family_name,
    emails,
    content='contacts',
    content_rowid='rowid'
);

CREATE TRIGGER contacts_ai AFTER INSERT ON contacts BEGIN
    INSERT INTO contacts_fts(rowid, display_name, given_name, family_name, emails)
    VALUES (new.rowid, new.display_name, new.given_name, new.family_name, new.emails);
END;

CREATE TRIGGER contacts_ad AFTER DELETE ON contacts BEGIN
    INSERT INTO contacts_fts(contacts_fts, rowid, display_name, given_name, family_name, emails)
    VALUES ('delete', old.rowid, old.display_name, old.given_name, old.family_name, old.emails);
END;

CREATE TRIGGER contacts_au AFTER UPDATE ON contacts BEGIN
    INSERT INTO contacts_fts(contacts_fts, rowid, display_name, given_name, family_name, emails)
    VALUES ('delete', old.rowid, old.display_name, old.given_name, old.family_name, old.emails);
    INSERT INTO contacts_fts(rowid, display_name, given_name, family_name, emails)
    VALUES (new.rowid, new.display_name, new.given_name, new.family_name, new.emails);
END;

PRAGMA foreign_keys = ON;
