# smtp-send Specification

## Purpose
TBD - created by archiving change email-core. Update Purpose after archive.
## Requirements
### Requirement: Send email via SMTP
The system SHALL send outgoing email using the SMTP configuration of the selected sender account.

#### Scenario: Successful send
- **WHEN** a user submits a composed message (from account, to, subject, body)
- **THEN** the system sends via the account's SMTP server and returns 200 with a message ID

#### Scenario: SMTP auth failure
- **WHEN** the SMTP server rejects authentication
- **THEN** the system returns 502 with an error indicating SMTP auth failure

---

### Requirement: Reply and forward
The system SHALL support reply and forward operations, correctly setting In-Reply-To and References headers.

#### Scenario: Reply to message
- **WHEN** a user replies to a message
- **THEN** the outgoing message includes In-Reply-To and References headers referencing the original

---

### Requirement: Attachments
The system SHALL support sending attachments up to a configurable size limit (default: 25 MB total per message).

#### Scenario: Attachment within limit
- **WHEN** a user attaches a file under the size limit
- **THEN** the file is included as a MIME attachment in the outgoing message

#### Scenario: Attachment exceeds limit
- **WHEN** a user attaches a file that causes total size to exceed the limit
- **THEN** the system returns 422 before attempting SMTP delivery

---

### Requirement: Save to Sent folder
After successful send, the system SHALL append the sent message to the account's IMAP Sent folder (APPEND command) if the folder exists.

#### Scenario: Sent folder append
- **WHEN** a message is sent successfully
- **THEN** the system appends it to the IMAP Sent folder and includes it in the next sync

---

### Requirement: Per-account sender identity
The system SHALL use the correct From address and SMTP credentials for the account selected as sender. Users MUST NOT be able to send from accounts they do not own.

#### Scenario: Cross-account send attempt
- **WHEN** a user submits a send request with an account ID that does not belong to them
- **THEN** the system returns 403

