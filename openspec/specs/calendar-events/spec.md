# calendar-events Specification

## Purpose
Define how calendar events are synced, created, edited, deleted, and displayed across connected calendar accounts.

## Requirements

### Requirement: Event sync from calendar accounts
The system SHALL sync events from all connected calendar accounts into the local DB. Sync SHALL be incremental using `ctag` (CalDAV), delta tokens (Graph API), or equivalent. Recurring events SHALL be expanded server-side into individual occurrence rows for a rolling 2-year window.

#### Scenario: Initial sync
- **WHEN** a calendar account is first connected
- **THEN** all events within the 2-year window are fetched, parsed, and stored

#### Scenario: Incremental sync
- **WHEN** a sync runs and the server ctag/sync-token is unchanged
- **THEN** no requests for event data are made (unchanged optimization)

#### Scenario: Recurring event expansion
- **WHEN** an event with RRULE is synced
- **THEN** individual occurrence rows are generated for each instance within the 2-year window, each referencing the master rrule_uid

---

### Requirement: Create event
The system SHALL allow authenticated users to create a new calendar event in any writable connected calendar, specifying title, start/end datetime, all-day flag, location, description, recurrence (RRULE), and attendees.

#### Scenario: Create single event
- **WHEN** a user submits a new event to `POST /calendar-events`
- **THEN** the event is created in the local DB and written to the remote calendar (CalDAV PUT or Graph POST)

#### Scenario: Create recurring event
- **WHEN** a user creates an event with a recurrence rule
- **THEN** the RRULE is stored and occurrences are expanded into the DB for the 2-year window

#### Scenario: Create event with attendees
- **WHEN** a user adds attendees to a new event
- **THEN** attendees are stored with RSVP status `needs-action`; meeting invitation emails are sent via SMTP to each attendee as `text/calendar; method=REQUEST`

---

### Requirement: Edit event
The system SHALL allow editing event properties. For recurring events, the user SHALL be able to edit a single occurrence or all future occurrences (RECURRENCE-ID semantics).

#### Scenario: Edit single occurrence of recurring event
- **WHEN** a user edits one occurrence of a recurring event
- **THEN** a RECURRENCE-ID exception is created in the iCalendar data; other occurrences are unaffected

#### Scenario: Edit all future occurrences
- **WHEN** a user edits "this and following" occurrences
- **THEN** the RRULE UNTIL is set on the original series and a new series is created from the edit point

---

### Requirement: Delete event
The system SHALL allow deleting single events or occurrences. Deleting a single occurrence of a recurring event SHALL add an EXDATE to the master, not delete the series.

#### Scenario: Delete single occurrence
- **WHEN** a user deletes one occurrence of a recurring event
- **THEN** an EXDATE is added for that date; the master series and other occurrences remain

#### Scenario: Delete entire series
- **WHEN** a user deletes the entire series
- **THEN** all occurrence rows are removed and the event is deleted from the remote calendar

---

### Requirement: Calendar views — month, week, day, agenda
The frontend SHALL provide month, week, day, and agenda (list) views. Events from all connected calendars SHALL be shown with per-calendar colour coding.

#### Scenario: Month view
- **WHEN** a user navigates to the month view
- **THEN** all events for the month are displayed as colour-coded blocks spanning their duration

#### Scenario: Week view with time grid
- **WHEN** a user navigates to the week view
- **THEN** a 7-column time grid shows events positioned by start time and duration

#### Scenario: All-day events
- **WHEN** an event has the all-day flag set
- **THEN** it is displayed in the all-day row at the top of the week/day view

---

### Requirement: Event detail
The frontend SHALL show a detail popover/modal on event click with: title, time, location, description, attendees with RSVP status, organizer, recurrence description, and a "Join Teams Meeting" button if a Teams URL is present.

#### Scenario: Teams meeting URL surfaced
- **WHEN** an event contains `X-MICROSOFT-SKYPETEAMSMEETINGURL`
- **THEN** a "Join Teams Meeting" button is shown in the event detail

---

### Requirement: Free/busy indicator
Events synced from Exchange / Graph SHALL display the `X-MICROSOFT-CDO-BUSYSTATUS` value (busy/free/tentative/OOF) as a visual indicator on the event block.

#### Scenario: Out-of-office event
- **WHEN** an event has `BUSYSTATUS=OOF`
- **THEN** it is displayed with a distinct OOF style in calendar views
