-- Whether a refresh token was issued from a "remember me" login. Drives the
-- session's sliding-window lifetime on /auth/refresh: remembered sessions
-- roll forward at a multi-day TTL, unremembered ones at a short one. Existing
-- rows default to remembered so already-issued 30-day cookies keep working.
ALTER TABLE refresh_tokens ADD COLUMN remember INTEGER NOT NULL DEFAULT 1;
