## ADDED Requirements

### Requirement: Add calendar account
The system SHALL allow authenticated users to connect a calendar account by selecting a provider type (caldav, graph, google, openxchange) and supplying the required credentials. Each user may connect multiple calendar accounts.

#### Scenario: Add CalDAV account
- **WHEN** a user submits a CalDAV base URL and credentials (Basic or OAuth2)
- **THEN** the system performs CalDAV PROPFIND discovery to locate calendar home, stores the account encrypted, and starts a sync task

#### Scenario: Add Exchange / O365 account via Graph API
- **WHEN** a user initiates Microsoft OAuth2 flow
- **THEN** the system redirects to Microsoft identity, exchanges the code for tokens with `Calendars.ReadWrite` scope, stores the refresh token encrypted, and starts sync

#### Scenario: Add Google Calendar account
- **WHEN** a user initiates Google OAuth2 flow (reuses existing XOAUTH2 infrastructure)
- **THEN** the system requests `calendar.events` scope, stores tokens, and starts sync

#### Scenario: Add Open-Xchange account
- **WHEN** a user submits an OX App Suite base URL and credentials
- **THEN** the system connects to the OX REST API, discovers calendars, and starts sync

#### Scenario: Connection fails
- **WHEN** calendar discovery or initial auth fails
- **THEN** the system returns 422 with the error and does NOT persist the account

---

### Requirement: List and delete calendar accounts
The system SHALL return all calendar accounts for the authenticated user and allow deletion. Deletion SHALL cascade-delete all synced events for that account.

#### Scenario: Delete account
- **WHEN** a user deletes a calendar account
- **THEN** the account, all calendars, and all events are removed; sync task is cancelled

---

### Requirement: Calendar account isolation
A user SHALL NOT be able to read or modify another user's calendar accounts or events.

#### Scenario: Cross-user access
- **WHEN** a user requests a calendar account ID belonging to another user
- **THEN** the system returns 404

---

### Requirement: Sync status per calendar account
The system SHALL expose sync status (idle / syncing / error / last_synced_at) per calendar account.

#### Scenario: Sync status query
- **WHEN** the frontend requests `GET /calendar-accounts/:id/sync-status`
- **THEN** the system returns current sync state and last successful sync timestamp
