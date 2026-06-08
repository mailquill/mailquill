## ADDED Requirements

### Requirement: Add email account
The system SHALL allow authenticated users to connect an email account by providing IMAP/SMTP server details, credentials, and `body_sync_mode` (`lazy` | `full`, default `lazy`). Supported auth schemes: Plain/Login, CRAM-MD5, OAuth2, XOAUTH2.

#### Scenario: Add account with plain credentials
- **WHEN** a user submits IMAP host/port/username/password, SMTP host/port/username/password, and optional `body_sync_mode`
- **THEN** the system stores the account (credentials encrypted at rest), tests the IMAP connection, and returns 201 on success

#### Scenario: Default body sync mode
- **WHEN** a user adds an account without specifying `body_sync_mode`
- **THEN** the account is created with `body_sync_mode=lazy`

#### Scenario: Add account with XOAUTH2 (Gmail)
- **WHEN** a user initiates OAuth2 flow for a Gmail account
- **THEN** the system redirects to Google, receives the authorization code, exchanges for refresh token, stores it encrypted, and associates the account with the user

#### Scenario: Connection test fails
- **WHEN** IMAP connection with provided credentials fails during account add
- **THEN** the system returns 422 with the connection error message and does NOT persist the account

---

### Requirement: List email accounts
The system SHALL return all email accounts belonging to the authenticated user.

#### Scenario: List accounts
- **WHEN** a user requests `GET /accounts`
- **THEN** the system returns all accounts for that user (no credentials in response)

---

### Requirement: Update email account
The system SHALL allow users to update server settings and credentials for a connected account.

#### Scenario: Update SMTP password
- **WHEN** a user submits updated credentials for an existing account
- **THEN** the system re-encrypts credentials and re-tests connectivity before saving

---

### Requirement: Delete email account
The system SHALL allow users to remove a connected account. Deletion SHALL cascade-delete all synced messages for that account.

#### Scenario: Delete account
- **WHEN** a user deletes an account
- **THEN** the account record, all synced messages, and the associated sync task are removed

---

### Requirement: Credential isolation
Each user's account credentials SHALL be isolated — a user MUST NOT be able to read or modify another user's accounts.

#### Scenario: Cross-user access attempt
- **WHEN** a user requests an account ID that belongs to another user
- **THEN** the system returns 404 (not 403, to avoid enumeration)
