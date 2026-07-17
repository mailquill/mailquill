## MODIFIED Requirements

### Requirement: Add contact account
The system SHALL automatically manage a contact source for each eligible mailbox account. Authenticated users MAY additionally connect independent CardDAV sources by supplying a base URL and credentials, but MUST NOT create independent Google or Microsoft sources by manually entering access or refresh tokens.

#### Scenario: Add CardDAV account
- **WHEN** a user submits an independent CardDAV base URL and credentials using the advanced contact-source flow
- **THEN** the system performs address-book discovery, stores its independent credentials encrypted, and starts a sync task

#### Scenario: Add Exchange / O365 via Graph API
- **WHEN** a user connects or re-consents a Microsoft mailbox with `Contacts.ReadWrite`
- **THEN** the system derives its managed Graph source from the mailbox and starts contact sync without a second account form

#### Scenario: Add Google Contacts
- **WHEN** a user connects or re-consents a Google mailbox with contact read/write scope
- **THEN** the system derives its managed People API source from the mailbox and starts contact sync without a second account form

#### Scenario: Independent API token is submitted
- **WHEN** a user attempts to create a Google or Microsoft contact source with a manually entered token
- **THEN** the system rejects the request and directs the user to connect or re-consent the corresponding mailbox

#### Scenario: Independent CardDAV connection fails
- **WHEN** CardDAV discovery or initial authentication fails
- **THEN** the system returns 422 with an actionable error and does not persist the independent source

### Requirement: List and delete contact accounts
The system SHALL return all mailbox-managed and independent contact sources for the authenticated user. Users SHALL be able to delete independent sources directly; mailbox-managed sources SHALL be removed only through mailbox deletion or disabled through the mailbox contact-capability controls.

#### Scenario: Delete independent account
- **WHEN** a user deletes an independent CardDAV source
- **THEN** its sync task is cancelled and its local contacts, books, and groups are cascade-deleted without deleting remote contacts

#### Scenario: Delete managed source directly
- **WHEN** a user attempts to delete a mailbox-managed source through the independent source endpoint
- **THEN** the system rejects the request and identifies the owning mailbox controls

### Requirement: Sync status per contact account
The system SHALL durably expose `disabled`, `pending`, `syncing`, `idle`, `consent_required`, `reauth_required`, `error`, and `unavailable` state per contact source together with last successful sync time and an actionable error category. Open clients SHALL receive status changes without polling as the only mechanism.

#### Scenario: Sync status query
- **WHEN** the frontend requests the sync status of a source
- **THEN** the system returns current durable state, last successful sync timestamp, provider, and a sanitized actionable error

#### Scenario: Authentication requires user action
- **WHEN** the source enters `consent_required` or `reauth_required`
- **THEN** automatic retries pause and connected clients receive the new state with the appropriate user action

#### Scenario: Transient provider error
- **WHEN** synchronization fails because of rate limiting, timeout, or a temporary provider failure
- **THEN** the source enters `error`, preserves its last successful cursor, and retries with bounded backoff

#### Scenario: Contact synchronization is disabled
- **WHEN** the user disables contact synchronization for a managed source
- **THEN** the source enters `disabled`, automatic retries stop, and its status explains whether downloaded contacts were retained or removed locally
