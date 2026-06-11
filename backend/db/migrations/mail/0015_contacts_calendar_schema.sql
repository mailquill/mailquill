-- Real schema for the contacts and calendar modules. The earlier *_stubs
-- migrations created id-only placeholder tables; these are empty, so we drop
-- and recreate them with the full column set the API and UI need.

DROP TABLE IF EXISTS contact_group_members;
DROP TABLE IF EXISTS contact_groups;
DROP TABLE IF EXISTS contacts;

CREATE TABLE contacts (
    id           TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    account_id   TEXT,
    display_name TEXT NOT NULL,
    email        TEXT,
    phone        TEXT,
    company      TEXT,
    job_title    TEXT,
    notes        TEXT,
    favorite     INTEGER NOT NULL DEFAULT 0,
    group_name   TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_contacts_display_name ON contacts(display_name);
CREATE INDEX idx_contacts_account ON contacts(account_id);

DROP TABLE IF EXISTS calendar_events;
DROP TABLE IF EXISTS calendars;

CREATE TABLE calendars (
    id         TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    account_id TEXT,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL DEFAULT '#2563EB',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE calendar_events (
    id          TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    calendar_id TEXT NOT NULL,
    title       TEXT NOT NULL,
    description TEXT,
    location    TEXT,
    starts_at   TEXT NOT NULL,
    ends_at     TEXT NOT NULL,
    all_day     INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_events_calendar ON calendar_events(calendar_id);
CREATE INDEX idx_events_start ON calendar_events(starts_at);
