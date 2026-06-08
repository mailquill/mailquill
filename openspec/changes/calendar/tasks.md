## 1. Calendar — Backend Scaffolding & Schema

- [ ] 1.1 Add `calendar-sync` crate to Rust workspace; add deps: `icalendar`, `reqwest`, `oauth2` (reuse)
- [ ] 1.2 Replace `calendar_accounts` stub (from `foundation`) with full schema: (id, display_name, type, base_url, auth_scheme, credentials_encrypted, sync_interval_secs, last_synced_at, ctag, sync_token) — no user_id (Postgres mode adds user_id column)
- [ ] 1.3 Replace `calendars` stub with full schema: (id, account_id, name, color, ctag, sync_token, is_default)
- [ ] 1.4 Replace `calendar_events` stub with full schema: (id, calendar_id, uid, summary, description, location, start_dt, end_dt, all_day, rrule, rrule_uid, recurrence_id, status, organizer_email, organizer_name, attendees JSON, ms_busystatus, ms_teams_url, raw_ical, created_at, updated_at, synced_at) — no user_id (Postgres mode adds user_id column)
- [ ] 1.5 Replace `meeting_invitations` stub with full schema: (id, message_id FK, method, uid, summary, start_dt, end_dt, organizer_email, attendees JSON, user_rsvp_status, raw_ical) — no user_id (Postgres mode adds user_id column)
- [ ] 1.6 Create indexes: `(start_dt, end_dt)` for calendar range queries; `(uid)` for invitation matching; `(account_id)` for sync
- [ ] 1.7 Add `default_calendar_id` (nullable FK to `calendars`) to `user_settings` table for RSVP target

## 2. Calendar — Sync Engine

- [ ] 2.1 Implement CalDAV PROPFIND calendar-home discovery; store calendars
- [ ] 2.2 Implement CalDAV REPORT calendar-query: fetch all VEVENTs in date range; use ctag for change detection
- [ ] 2.3 Implement CalDAV PUT (create/update event) and DELETE
- [ ] 2.4 Implement Microsoft Graph sync: `GET /me/calendars`, `GET /me/calendarView` with delta query (`@odata.deltaLink`) for incremental sync
- [ ] 2.5 Implement Microsoft Graph write: `POST /me/events`, `PATCH /me/events/:id`, `DELETE /me/events/:id`
- [ ] 2.6 Implement Google Calendar sync: `GET /calendars/primary/events` with `syncToken` for incremental sync
- [ ] 2.7 Implement Google Calendar write: `POST /calendars/primary/events`, `PUT`, `DELETE`
- [ ] 2.8 Implement Open-Xchange sync: `/api/chronos` calendar list + event fetch
- [ ] 2.9 Implement iCalendar parsing: VEVENT, VCALENDAR, VTIMEZONE, RRULE, EXDATE, RECURRENCE-ID (`icalendar` crate)
- [ ] 2.10 Implement RRULE server-side expansion: generate occurrence rows for rolling 2-year window; handle EXDATE exceptions and RECURRENCE-ID overrides
- [ ] 2.11 Normalize all datetimes to UTC using VTIMEZONE data at parse time
- [ ] 2.12 Parse Outlook-specific fields: `X-MICROSOFT-SKYPETEAMSMEETINGURL`, `X-MICROSOFT-CDO-BUSYSTATUS`, `X-MICROSOFT-DISALLOW-COUNTER`, `CUTYPE=RESOURCE`
- [ ] 2.13 Implement sync task manager for calendar accounts (same pattern as IMAP — spawn/cancel per account)
- [ ] 2.14 Implement poll loop with exponential backoff on provider errors; handle Graph 429 rate limit with `Retry-After` header

## 3. Calendar — Meeting Invitations from Email

- [ ] 3.1 Consume `text/calendar` MIME flag set by IMAP sync (email-core task 4.15); parse METHOD field
- [ ] 3.2 For `METHOD:REQUEST`: create `meeting_invitations` row linked to message; set `user_rsvp_status=pending`
- [ ] 3.3 For `METHOD:CANCEL`: mark linked calendar event as cancelled; update invitation status
- [ ] 3.4 For `METHOD:REPLY`: update attendee PARTSTAT on the matching calendar event (by UID)
- [ ] 3.5 Implement `POST /meeting-invitations/:id/rsvp` — accept/tentative/decline; update local event; send METHOD:REPLY email via SMTP
- [ ] 3.6 RSVP email: compose `text/calendar; method=REPLY` MIME part with correct UID, ATTENDEE PARTSTAT, ORGANIZER; include `text/plain` human-readable part
- [ ] 3.7 On accept: create/update event in user's default calendar; set `PARTSTAT=ACCEPTED`
- [ ] 3.8 On decline: send reply, do NOT add event to calendar

## 4. Calendar — Account & Event API

- [ ] 4.1 Implement `POST /calendar-accounts`, `GET /calendar-accounts`, `DELETE /calendar-accounts/:id`, `GET /calendar-accounts/:id/sync-status`
- [ ] 4.2 Implement `GET /calendars` — list all calendars across all connected accounts
- [ ] 4.3 Implement `GET /calendar-events?start=&end=` — range query across all user calendars
- [ ] 4.4 Implement `POST /calendar-events`, `PUT /calendar-events/:id`, `DELETE /calendar-events/:id` with write-through to remote account
- [ ] 4.5 Implement `GET /meeting-invitations` — list pending invitations for authenticated user

## 5. Calendar — Frontend

- [ ] 5.1 Add calendar accounts page: connect CalDAV / Exchange (OAuth) / Google (OAuth) / OX; show sync status
- [ ] 5.2 Add calendar nav item in sidebar; implement route `/calendar`
- [ ] 5.3 Build month view: grid of days, event blocks colour-coded by calendar, overflow indicator ("+N more")
- [ ] 5.4 Build week view: 7-column time grid, events positioned by start time and duration, all-day row
- [ ] 5.5 Build day view: single-column time grid
- [ ] 5.6 Build agenda view: chronological list of upcoming events
- [ ] 5.7 Build event detail popover/modal: title, time, location, description, attendees+RSVP status, organizer, recurrence text, "Join Teams Meeting" button (if Teams URL present), free/busy badge
- [ ] 5.8 Build create/edit event modal: title, date/time pickers, all-day toggle, location, description, recurrence selector (none/daily/weekly/monthly/yearly + end date), attendees input with key discovery, calendar picker
- [ ] 5.9 On attendee add in event create: call `GET /keys/discover` (if email-crypto change deployed) AND check if attendee has a calendar for meeting invite email
- [ ] 5.10 Build inline RSVP card in message view: show above message body when `meeting_invitations` row exists; display event summary, time, organizer, attendees, Teams button; Accept / Tentative / Decline buttons; show current status if already responded
- [ ] 5.11 Implement ICS download button on RSVP card
- [ ] 5.12 Handle "edit recurring event" prompt: "This event", "This and following", "All events" options
- [ ] 5.13 Add default calendar selector in user settings (used for RSVP accepts)
