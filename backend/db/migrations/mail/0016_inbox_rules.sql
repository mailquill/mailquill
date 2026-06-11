-- Mail filtering rules. Conditions and actions are stored as JSON so the rule
-- model can evolve without schema churn. `engine` selects how a rule would be
-- compiled for the server: a Sieve script or an Exchange inbox rule.
CREATE TABLE inbox_rules (
    id          TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    account_id  TEXT,
    name        TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    engine      TEXT NOT NULL DEFAULT 'sieve',
    match_all   INTEGER NOT NULL DEFAULT 1,
    conditions  TEXT NOT NULL DEFAULT '[]',
    actions     TEXT NOT NULL DEFAULT '[]',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
