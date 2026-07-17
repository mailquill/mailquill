-- Mailbox-owned contact sources share credentials and lifecycle with an email
-- account. Existing standalone sources remain independent and keep their
-- credentials so this migration is non-destructive and rollback-safe.
ALTER TABLE contact_accounts ADD COLUMN email_account_id TEXT
    REFERENCES email_accounts(id) ON DELETE CASCADE;
ALTER TABLE contact_accounts ADD COLUMN management_mode TEXT NOT NULL DEFAULT 'independent'
    CHECK (management_mode IN ('mailbox', 'independent'));
ALTER TABLE contact_accounts ADD COLUMN capability_state TEXT NOT NULL DEFAULT 'idle'
    CHECK (capability_state IN (
        'disabled', 'pending', 'syncing', 'idle', 'consent_required',
        'reauth_required', 'error', 'unavailable'
    ));
ALTER TABLE contact_accounts ADD COLUMN capability_reason TEXT;
ALTER TABLE contact_accounts ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1
    CHECK (enabled IN (0, 1));
ALTER TABLE contact_accounts ADD COLUMN cache_retained INTEGER NOT NULL DEFAULT 1
    CHECK (cache_retained IN (0, 1));
ALTER TABLE contact_accounts ADD COLUMN sync_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE contact_accounts ADD COLUMN provider_metadata TEXT NOT NULL DEFAULT '{}';

CREATE UNIQUE INDEX idx_contact_accounts_email_account
    ON contact_accounts(email_account_id)
    WHERE email_account_id IS NOT NULL;
CREATE INDEX idx_contact_accounts_management_state
    ON contact_accounts(management_mode, capability_state);

CREATE TABLE contact_books (
    id                TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    account_id        TEXT NOT NULL REFERENCES contact_accounts(id) ON DELETE CASCADE,
    remote_id         TEXT NOT NULL,
    display_name      TEXT NOT NULL,
    parent_remote_id  TEXT,
    is_default        INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    is_writable       INTEGER NOT NULL DEFAULT 1 CHECK (is_writable IN (0, 1)),
    sync_cursor       TEXT,
    sync_generation   INTEGER NOT NULL DEFAULT 0,
    provider_metadata TEXT NOT NULL DEFAULT '{}',
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(account_id, remote_id)
);

CREATE INDEX idx_contact_books_account ON contact_books(account_id);
CREATE INDEX idx_contact_books_generation ON contact_books(account_id, sync_generation);

ALTER TABLE contacts ADD COLUMN book_id TEXT
    REFERENCES contact_books(id) ON DELETE CASCADE;
ALTER TABLE contacts ADD COLUMN remote_version TEXT;
ALTER TABLE contacts ADD COLUMN provider_metadata TEXT NOT NULL DEFAULT '{}';
ALTER TABLE contacts ADD COLUMN photo_reference TEXT;
ALTER TABLE contacts ADD COLUMN photo_version TEXT;
ALTER TABLE contacts ADD COLUMN photo_content_type TEXT;
ALTER TABLE contacts ADD COLUMN sync_generation INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_contacts_book ON contacts(book_id);
CREATE INDEX idx_contacts_generation ON contacts(account_id, sync_generation);

ALTER TABLE contact_groups ADD COLUMN book_id TEXT
    REFERENCES contact_books(id) ON DELETE CASCADE;
ALTER TABLE contact_groups ADD COLUMN remote_id TEXT;
ALTER TABLE contact_groups ADD COLUMN remote_version TEXT;
ALTER TABLE contact_groups ADD COLUMN is_writable INTEGER NOT NULL DEFAULT 1
    CHECK (is_writable IN (0, 1));
ALTER TABLE contact_groups ADD COLUMN sync_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE contact_groups ADD COLUMN provider_metadata TEXT NOT NULL DEFAULT '{}';

CREATE UNIQUE INDEX idx_contact_groups_remote
    ON contact_groups(account_id, remote_id)
    WHERE remote_id IS NOT NULL;
CREATE INDEX idx_contact_groups_book ON contact_groups(book_id);
CREATE INDEX idx_contact_groups_generation ON contact_groups(account_id, sync_generation);

-- A configured CardDAV collection URL is the only legacy identity available
-- in plaintext. Link it only when exactly one mailbox has the same normalized
-- URL; OAuth identities are reconciled later after credential decryption.
UPDATE contact_accounts AS source
SET email_account_id = (
        SELECT mailbox.id
        FROM email_accounts AS mailbox
        WHERE source.type = 'cardav'
          AND source.base_url IS NOT NULL
          AND mailbox.carddav_url IS NOT NULL
          AND lower(rtrim(source.base_url, '/')) = lower(rtrim(mailbox.carddav_url, '/'))
    ),
    management_mode = 'mailbox'
WHERE source.type = 'cardav'
  AND source.email_account_id IS NULL
  AND (
      SELECT count(*)
      FROM email_accounts AS mailbox
      WHERE mailbox.carddav_url IS NOT NULL
        AND source.base_url IS NOT NULL
        AND lower(rtrim(source.base_url, '/')) = lower(rtrim(mailbox.carddav_url, '/'))
  ) = 1;
