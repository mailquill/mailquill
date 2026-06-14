-- When a refresh token is rotated, record when. The /auth/refresh handler uses
-- this for a short reuse grace: a token rotated within the last minute that is
-- presented again is a benign concurrent/multi-tab refresh (parallel requests,
-- or a second browser tab) rather than token theft, so the client gets a fresh
-- token instead of being logged out. Reuse outside the grace window — or of a
-- token revoked by logout (replaced_at stays NULL) — is still rejected.
ALTER TABLE refresh_tokens ADD COLUMN replaced_at TEXT;
