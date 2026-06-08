CREATE TABLE IF NOT EXISTS user_brand_entries (
    id         TEXT NOT NULL PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    domain     TEXT NOT NULL,
    brand_name TEXT NOT NULL,
    UNIQUE(domain)
);
