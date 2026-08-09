-- Senders the user has explicitly marked "not spam". Stores the registrable
-- domain (github.com, not mail.github.com); the phishing analysis short-cuts
-- to a clean verdict for any sender on a trusted domain, so one click teaches
-- the system instead of fighting the same false positive per message.
CREATE TABLE IF NOT EXISTS phishing_trusted_senders (
    domain     TEXT NOT NULL PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
