## ADDED Requirements

### Requirement: Detect meeting invitations in email
The IMAP sync engine SHALL detect `text/calendar` MIME parts in messages and parse the METHOD field. Detected invitations SHALL be stored in `meeting_invitations` and linked to the originating message.

#### Scenario: Meeting request detected
- **WHEN** a synced message contains a `text/calendar; method=REQUEST` part
- **THEN** a `meeting_invitations` row is created linking to the message, with `method=REQUEST` and `user_rsvp_status=pending`

#### Scenario: Cancellation detected
- **WHEN** a synced message contains a `text/calendar; method=CANCEL` part
- **THEN** the corresponding local calendar event is marked cancelled and the invitation shows a cancellation notice

#### Scenario: Reply detected (attendee responded to your invite)
- **WHEN** a synced message contains `method=REPLY`
- **THEN** the attendee's RSVP status on the local calendar event is updated

---

### Requirement: Inline RSVP card in message view
Messages containing a meeting invitation SHALL display an inline RSVP card above the message body showing: event title, date/time, organizer, location, attendee list, and Accept / Tentative / Decline buttons.

#### Scenario: RSVP card displayed
- **WHEN** a user opens a message with a linked meeting invitation
- **THEN** an RSVP card is shown with event details and action buttons

#### Scenario: Already responded
- **WHEN** the user has already responded to the invitation
- **THEN** the card shows the current RSVP status with an option to change it

#### Scenario: Outlook Teams URL surfaced
- **WHEN** the invitation contains `X-MICROSOFT-SKYPETEAMSMEETINGURL`
- **THEN** a "Join Teams Meeting" button is shown on the RSVP card

---

### Requirement: RSVP — Accept
When a user accepts a meeting invitation, the system SHALL: (1) create or update the event in the user's default calendar, (2) send a `METHOD:REPLY` response email to the organizer via SMTP.

#### Scenario: Accept invitation
- **WHEN** a user clicks Accept on an RSVP card
- **THEN** `user_rsvp_status` is set to `accepted`, the event is added to the user's calendar with `PARTSTAT=ACCEPTED`, and a reply email is sent to the organizer

#### Scenario: Accept adds to correct calendar
- **WHEN** a user accepts and has multiple calendar accounts
- **THEN** the system uses the user's designated default calendar (configurable in settings)

---

### Requirement: RSVP — Tentative
When a user responds tentatively, the system SHALL create/update the event with `PARTSTAT=TENTATIVE` and send a `METHOD:REPLY` with tentative status.

#### Scenario: Tentative response
- **WHEN** a user clicks Tentative
- **THEN** event is created with tentative status and reply is sent to organizer

---

### Requirement: RSVP — Decline
When a user declines, the system SHALL send a `METHOD:REPLY` with `PARTSTAT=DECLINED` and NOT add the event to the calendar (or mark existing event as declined).

#### Scenario: Decline invitation
- **WHEN** a user clicks Decline
- **THEN** a decline reply is sent; no event is added to the calendar

---

### Requirement: ICS attachment download
The system SHALL allow downloading the raw `.ics` file from any message containing a calendar attachment, for use in external calendar apps.

#### Scenario: Download ICS
- **WHEN** a user clicks "Download .ics" on the RSVP card
- **THEN** the raw iCalendar data is downloaded as a `.ics` file

---

### Requirement: RSVP reply email format
RSVP reply emails SHALL be sent as `text/calendar; method=REPLY` MIME parts with correct `ATTENDEE` PARTSTAT, `ORGANIZER`, and `UID` fields matching the original invitation. The reply SHALL also include a human-readable `text/plain` part.

#### Scenario: Reply email headers
- **WHEN** an RSVP reply is sent
- **THEN** the outgoing message has `Content-Type: text/calendar; method=REPLY` and the iCalendar body contains the correct UID and PARTSTAT
