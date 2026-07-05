# contact-accounts Specification

## Purpose
TBD - created by archiving change contacts. Update Purpose after archive.
## Requirements
### Requirement: Add contact account
The system SHALL allow authenticated users to connect a contact account by selecting a provider type (cardav, graph, google) and supplying credentials. Each user may connect multiple contact accounts.

#### Scenario: Add CardDAV account
- **WHEN** a user submits a CardDAV base URL and credentials (Basic or OAuth2)
- **THEN** the system performs PROPFIND address-book discovery, stores the account encrypted, and starts a sync task

#### Scenario: Add Exchange / O365 via Graph API
- **WHEN** a user initiates Microsoft OAuth2 flow with Contacts scope
- **THEN** the system requests `Contacts.ReadWrite` scope (reuses existing Graph OAuth2 infrastructure), stores tokens, and starts sync

#### Scenario: Add Google Contacts
- **WHEN** a user initiates Google OAuth2 flow (reuses existing XOAUTH2 infrastructure)
- **THEN** the system requests `contacts.readonly` scope, stores tokens, and starts sync

#### Scenario: Connection fails
- **WHEN** address-book discovery or initial auth fails
- **THEN** the system returns 422 with the error and does NOT persist the account

---

### Requirement: List and delete contact accounts
The system SHALL return all contact accounts for the authenticated user and allow deletion. Deletion SHALL cascade-delete all synced contacts for that account.

#### Scenario: Delete account
- **WHEN** a user deletes a contact account
- **THEN** the account and all synced contacts are removed; sync task is cancelled

---

### Requirement: Contact account isolation
A user SHALL NOT be able to read or modify another user's contact accounts or contacts.

#### Scenario: Cross-user access
- **WHEN** a user requests a contact account ID belonging to another user
- **THEN** the system returns 404

---

### Requirement: Sync status per contact account
The system SHALL expose sync status (idle / syncing / error / last_synced_at) per contact account.

#### Scenario: Sync status query
- **WHEN** the frontend requests `GET /contact-accounts/:id/sync-status`
- **THEN** the system returns current sync state and last successful sync timestamp

