## Why

Email and calendar are deeply intertwined: meeting invitations arrive as emails, RSVPs are sent via SMTP, and calendar events reference email attendees. Adding calendar support to Mailquill lets users manage their full communication workflow in one place without depending on a separate calendar app or a third-party SaaS. All calendar data syncs locally — no Google Calendar web UI or Outlook.com required.

## What Changes

- Calendar account management: connect CalDAV, Microsoft Exchange (Graph API), Google Calendar, and Open-Xchange accounts per user
- Background sync engine for each provider with incremental update support (ctag/sync-token/delta)
- Full event CRUD: create, read, update, delete across all connected providers with write-through
- RRULE recurrence expansion server-side (rolling 2-year window), EXDATE exceptions, RECURRENCE-ID overrides
- Meeting invitation detection in emails (iCalendar MIME parts), inline RSVP UI, SMTP reply with METHOD:REPLY
- Microsoft Teams URL surfacing from Outlook-specific iCal fields
- Calendar frontend: month, week, day, and agenda views; event create/edit modal; RSVP card in message view

## Capabilities

### New Capabilities

- `calendar-accounts`: Connect CalDAV, Exchange (Graph API), Google Calendar, Open-Xchange calendar accounts per user
- `calendar-events`: Full event CRUD, recurrence (RRULE), attendees, month/week/day/agenda views
- `meeting-invitations`: Detect iCalendar in emails, inline RSVP (Accept/Tentative/Decline), reply via SMTP, Teams URL surfacing

### Modified Capabilities

## Impact

- Depends on `email-core` (IMAP sync pipeline, SMTP send, user settings, message view)
- Rust additions: `calendar-sync` crate; `icalendar` crate (VEVENT/VCALENDAR/VTIMEZONE/RRULE parsing); `reqwest` for CalDAV WebDAV and REST API calls; `oauth2` (reuse existing)
- New DB tables: `calendar_accounts`, `calendars`, `calendar_events`, `meeting_invitations` (full schema; stubs created in `foundation`)
- New API routes: `/calendar-accounts/*`, `/calendars/*`, `/calendar-events/*`, `/meeting-invitations/*`
- Frontend addition: calendar sidebar nav item, route `/calendar`, 4 calendar views, event modals, RSVP card component in message view
- EWS (Exchange on-premises SOAP) deferred — Graph API covers Exchange Online / O365; EWS needed only for legacy on-prem
- VTODO (tasks) not implemented in v1
