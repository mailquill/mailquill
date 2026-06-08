-- Contact tables stub — full schema defined in the contacts change
CREATE TABLE IF NOT EXISTS contact_accounts (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16))))
);

CREATE TABLE IF NOT EXISTS contacts (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16))))
);

CREATE TABLE IF NOT EXISTS contact_groups (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16))))
);

CREATE TABLE IF NOT EXISTS contact_group_members (
    id TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16))))
);
