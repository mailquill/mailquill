## MODIFIED Requirements

### Requirement: Add email account
The system SHALL allow authenticated users to connect an email account by providing IMAP/SMTP details and credentials or by completing a supported OAuth flow. Google and Microsoft OAuth flows SHALL request the grants required for configured mail, calendar, and two-way contact capabilities, record granted scopes, and reconcile the mailbox-managed contact source after account creation. Plain and custom accounts SHALL retain optional CardDAV configuration.

#### Scenario: Add account with plain credentials
- **WHEN** a user submits valid IMAP/SMTP credentials and optional CardDAV configuration
- **THEN** the system stores credentials encrypted, tests mail connectivity, creates the email account, and reconciles a managed CardDAV source when contact configuration is usable

#### Scenario: Default body sync mode
- **WHEN** a user adds an account without specifying `body_sync_mode`
- **THEN** the account is created with `body_sync_mode=lazy`

#### Scenario: Add account with Google OAuth
- **WHEN** a user completes Google OAuth and grants mail plus contact read/write access
- **THEN** the system stores renewable credentials and granted scopes encrypted, creates the mailbox, and starts its managed Google People contact source

#### Scenario: Add account with Microsoft OAuth
- **WHEN** a user completes Microsoft OAuth and grants mail plus delegated `Contacts.ReadWrite`
- **THEN** the system stores renewable credentials and granted scopes encrypted, creates the mailbox, and starts its managed Graph contact source

#### Scenario: Contact scope is declined
- **WHEN** the provider returns valid mail grants without the required contact grant
- **THEN** mailbox creation succeeds for mail and its contact capability is recorded as `consent_required`

#### Scenario: User opts out of contacts
- **WHEN** the user turns off contact synchronization in the mailbox capability review
- **THEN** mailbox creation succeeds, no contact permission or CardDAV verification is required, and the account records the capability as `disabled` for later enablement

#### Scenario: Connection test fails
- **WHEN** mail connectivity or required account identity validation fails during account creation
- **THEN** the system returns 422 with an actionable error and does not persist the account

### Requirement: Update email account
The system SHALL allow users to update server settings and credentials for a connected account. Relevant updates SHALL re-test connectivity where applicable and reconcile the mailbox-managed contact source's provider, CardDAV configuration, credentials, consent state, and TLS policy without creating duplicates.

#### Scenario: Update SMTP password
- **WHEN** a user submits updated mail credentials for an existing account
- **THEN** the system re-encrypts credentials, re-tests connectivity, saves them, and restarts dependent mail and managed DAV tasks with the new credential set

#### Scenario: Update CardDAV URL
- **WHEN** a user changes a generic mailbox's CardDAV URL
- **THEN** the system validates or discovers the address books, updates the existing managed source, preserves contacts until a successful sync, and restarts contact synchronization

#### Scenario: Reconnect OAuth account
- **WHEN** a user reconnects an OAuth mailbox and grants the contact scope
- **THEN** the system updates shared credentials/scopes, clears contact consent or reauthentication errors, and resumes the existing managed source

### Requirement: Delete email account
The system SHALL allow users to remove a connected account. Deletion SHALL stop mail and managed contact tasks and cascade-delete local messages, the mailbox-managed contact source, contacts, books, groups, and cached contact photos. It MUST NOT issue provider-side contact or address-book deletions.

#### Scenario: Delete account
- **WHEN** a user deletes an email account
- **THEN** all local mail and managed contact data for that mailbox are removed and their sync tasks stop

#### Scenario: Independent contact source shares an address
- **WHEN** an independent CardDAV source happens to use the same email identity as the deleted mailbox
- **THEN** it remains intact because it has no mailbox ownership relationship

## ADDED Requirements

### Requirement: List mailbox contact capability
Email-account responses SHALL include a credential-free summary of the managed contact capability containing availability, provider, source ID, state, last successful sync, and required user action.

#### Scenario: List accounts with active contacts
- **WHEN** a user lists email accounts and a mailbox has an idle managed source
- **THEN** its response identifies the contact provider, source, idle state, and last successful sync time

#### Scenario: List account requiring contact consent
- **WHEN** a mailbox has working mail access but lacks the provider contact grant
- **THEN** its response reports `consent_required` and an OAuth reconnect action without exposing credentials

#### Scenario: List account with contacts disabled
- **WHEN** a mailbox was created with contact synchronization turned off
- **THEN** its response reports `disabled`, provider availability when known, and an enable action suitable for later setup
