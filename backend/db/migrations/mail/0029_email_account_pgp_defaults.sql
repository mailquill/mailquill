ALTER TABLE email_accounts ADD COLUMN pgp_key_id TEXT REFERENCES pgp_keys(id) ON DELETE SET NULL;
ALTER TABLE email_accounts ADD COLUMN sign_by_default INTEGER NOT NULL DEFAULT 0;
