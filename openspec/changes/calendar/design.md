## Context

Builds on `email-core`. The IMAP sync pipeline already detects `text/calendar` MIME parts and flags them (task 4.15 in `email-core`). This change handles those flags, implements calendar sync, and adds the calendar UI. Credential encryption reuses the same AES-256-GCM scheme as `email_accounts` (same `CREDENTIAL_ENCRYPTION_KEY`).

## Decisions

### D1: Calendar protocol support

| Provider | Protocol | Auth |
|---|---|---|
| CalDAV servers (Nextcloud, Apple, Radicale, FastMail) | CalDAV (RFC 4791) | Basic / OAuth2 |
| Exchange Online / O365 / Outlook.com | Microsoft Graph API | OAuth2 (reuse XOAUTH2 infra) |
| Google Calendar | Google Calendar API | OAuth2 (reuse XOAUTH2 infra) |
| Open-Xchange | OX App Suite REST API (`/api/chronos`) | Basic / OAuth2 |
| EWS (on-prem Exchange) | SOAP/EWS | **Deferred** |

CalDAV: custom `reqwest`-based WebDAV client in `calendar-sync` crate (no mature async CalDAV crate exists in Rust). Graph and Google: standard REST via `reqwest` + `oauth2`.

### D2: iCalendar parsing and RRULE expansion

**Parsing:** `icalendar` Rust crate for VEVENT/VCALENDAR/VFREEBUSY/VTIMEZONE.

**RRULE expansion:** Recurrence rules expanded server-side at sync time. Occurrences generated for a rolling 2-year window. Each occurrence stored as a separate row in `calendar_events` with reference to master `rrule_uid`. This avoids complex on-the-fly expansion in queries. Handles EXDATE exceptions and RECURRENCE-ID overrides.

**Timezones:** All datetimes stored as UTC in DB. `VTIMEZONE` + `TZID` parsed at sync time for conversion. Display timezone from browser `Intl.DateTimeFormat`.

**Outlook-specific fields parsed:**
- `X-MICROSOFT-SKYPETEAMSMEETINGURL` → "Join Teams Meeting" button
- `X-MICROSOFT-CDO-BUSYSTATUS` → free/busy indicator
- `X-MICROSOFT-DISALLOW-COUNTER` → hide "Propose new time"
- `CUTYPE=RESOURCE` → room/resource attendees displayed separately

### D3: Meeting invitations from email

IMAP sync detects `text/calendar` MIME parts (flagged in `email-core` task 4.15). This change handles them:
- `METHOD:REQUEST` → create `meeting_invitations` row linked to message; `user_rsvp_status=pending`
- `METHOD:CANCEL` → mark linked calendar event cancelled; update invitation status
- `METHOD:REPLY` → update attendee PARTSTAT on matching calendar event (by UID)

RSVP sends METHOD:REPLY email via SMTP (composes `text/calendar; method=REPLY` MIME part + `text/plain` human-readable part). Accept → create/update event in user's default calendar.

### D4: Sync task pattern — same as IMAP

Each calendar account gets a background tokio sync task (same spawn/cancel pattern as `email-core` IMAP tasks). Sync uses `ctag`/`sync-token`/`@odata.deltaLink` for incremental updates. Exponential backoff on provider errors; Graph 429 rate limit handled with `Retry-After` header.

### D5: DB schema — no user_id columns (SQLite per-user mode)

`calendar_accounts`, `calendars`, `calendar_events`, `meeting_invitations` all live in the per-user `mail.db` file — no `user_id` columns needed (structural isolation). Postgres mode adds `user_id` columns as for other mail tables.
