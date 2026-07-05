-- Full calendar schema for provider accounts, calendars, events, and meeting
-- invitations. Earlier migrations created placeholder/local-only tables and
-- CalDAV metadata; this migration preserves existing local event data while
-- moving to the OpenSpec calendar model.

ALTER TABLE calendar_accounts RENAME TO calendar_accounts_old;
ALTER TABLE calendars RENAME TO calendars_old;
ALTER TABLE calendar_events RENAME TO calendar_events_old;
ALTER TABLE meeting_invitations RENAME TO meeting_invitations_old;

CREATE TABLE calendar_accounts (
    id                    TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    display_name          TEXT NOT NULL,
    type                  TEXT NOT NULL CHECK (type IN ('caldav', 'graph', 'google', 'openxchange')),
    base_url              TEXT,
    auth_scheme           TEXT NOT NULL CHECK (auth_scheme IN ('basic', 'oauth2')),
    credentials_encrypted BLOB NOT NULL,
    sync_interval_secs    INTEGER NOT NULL DEFAULT 300,
    last_synced_at        TEXT,
    ctag                  TEXT,
    sync_token            TEXT,
    sync_status           TEXT NOT NULL DEFAULT 'idle',
    sync_error            TEXT,
    created_at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE calendars (
    id         TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    account_id TEXT,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL DEFAULT '#2563EB',
    ctag       TEXT,
    sync_token TEXT,
    is_default INTEGER NOT NULL DEFAULT 0,
    dav_url    TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE calendar_events (
    id              TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    calendar_id     TEXT NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
    uid             TEXT,
    summary         TEXT NOT NULL,
    description     TEXT,
    location        TEXT,
    start_dt        TEXT NOT NULL,
    end_dt          TEXT NOT NULL,
    all_day         INTEGER NOT NULL DEFAULT 0,
    rrule           TEXT,
    rrule_uid       TEXT,
    recurrence_id   TEXT,
    status          TEXT NOT NULL DEFAULT 'confirmed',
    organizer_email TEXT,
    organizer_name  TEXT,
    attendees       TEXT NOT NULL DEFAULT '[]',
    ms_busystatus   TEXT,
    ms_teams_url    TEXT,
    raw_ical        TEXT,
    href            TEXT,
    etag            TEXT,
    dirty           INTEGER NOT NULL DEFAULT 0,
    deleted         INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    synced_at       TEXT
);

CREATE TABLE meeting_invitations (
    id                 TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    message_id         TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    method             TEXT NOT NULL,
    uid                TEXT NOT NULL,
    summary            TEXT,
    start_dt           TEXT,
    end_dt             TEXT,
    organizer_email    TEXT,
    attendees          TEXT NOT NULL DEFAULT '[]',
    user_rsvp_status   TEXT NOT NULL DEFAULT 'pending',
    raw_ical           TEXT NOT NULL,
    ms_teams_url       TEXT,
    created_at         TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO calendars (id, account_id, name, color, dav_url, created_at)
SELECT id, account_id, name, color, dav_url, created_at
FROM calendars_old;

INSERT INTO calendar_events (
    id, calendar_id, uid, summary, description, location, start_dt, end_dt,
    all_day, href, etag, dirty, deleted, created_at, updated_at
)
SELECT
    id, calendar_id, uid, title, description, location, starts_at, ends_at,
    all_day, href, etag, dirty, deleted, created_at,
    COALESCE(updated_at, created_at, datetime('now'))
FROM calendar_events_old;

CREATE INDEX idx_calendar_events_range ON calendar_events(start_dt, end_dt);
CREATE INDEX idx_calendar_events_uid ON calendar_events(uid);
CREATE INDEX idx_calendar_events_calendar ON calendar_events(calendar_id);
CREATE INDEX idx_calendar_events_rrule_uid ON calendar_events(rrule_uid);
CREATE INDEX idx_calendar_events_calendar_uid
    ON calendar_events(calendar_id, uid) WHERE uid IS NOT NULL;
CREATE INDEX idx_calendars_account ON calendars(account_id);
CREATE INDEX idx_meeting_invitations_uid ON meeting_invitations(uid);
CREATE INDEX idx_meeting_invitations_message ON meeting_invitations(message_id);

DROP TABLE calendar_accounts_old;
DROP TABLE calendars_old;
DROP TABLE calendar_events_old;
DROP TABLE meeting_invitations_old;
