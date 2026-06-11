-- CardDAV/CalDAV collection URLs per account. Left NULL when unknown; the sync
-- routine falls back to RFC 6764 well-known discovery from the email domain.
ALTER TABLE email_accounts ADD COLUMN carddav_url TEXT;
ALTER TABLE email_accounts ADD COLUMN caldav_url TEXT;
