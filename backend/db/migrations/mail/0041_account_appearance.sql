-- User-chosen account colour (hex, e.g. #2563EB); NULL falls back to the
-- client-side hash-derived palette colour. sort_order drives the sidebar
-- account ordering; existing accounts keep their created_at order.
ALTER TABLE email_accounts ADD COLUMN color TEXT;
ALTER TABLE email_accounts ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

UPDATE email_accounts
SET sort_order = (
    SELECT COUNT(*)
    FROM email_accounts AS other
    WHERE other.created_at < email_accounts.created_at
       OR (other.created_at = email_accounts.created_at AND other.id < email_accounts.id)
);
