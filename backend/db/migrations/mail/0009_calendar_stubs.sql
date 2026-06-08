-- Calendar tables stub — full schema defined in the calendar change
CREATE TABLE IF NOT EXISTS calendar_accounts (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16))))
);

CREATE TABLE IF NOT EXISTS calendars (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16))))
);

CREATE TABLE IF NOT EXISTS calendar_events (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16))))
);

CREATE TABLE IF NOT EXISTS meeting_invitations (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16))))
);
