-- User-approved TLS trust exceptions (Thunderbird-style), one per service.
-- Holds the base64-encoded DER certificate the user explicitly accepted in the
-- account wizard; connections add it as a trust anchor and skip hostname
-- verification. NULL means standard WebPKI verification.
ALTER TABLE email_accounts ADD COLUMN imap_tls_cert TEXT;
ALTER TABLE email_accounts ADD COLUMN smtp_tls_cert TEXT;
