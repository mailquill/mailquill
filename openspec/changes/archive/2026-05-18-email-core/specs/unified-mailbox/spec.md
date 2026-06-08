## ADDED Requirements

### Requirement: Unified inbox view
The system SHALL provide a unified inbox that merges messages from all of a user's connected accounts, sorted by date descending.

#### Scenario: Unified inbox fetch
- **WHEN** a user requests `GET /mailbox/unified`
- **THEN** the system returns paginated messages from all accounts' INBOX folders, sorted by date descending

---

### Requirement: Per-account folder view
The system SHALL provide per-account, per-folder message lists.

#### Scenario: Folder message list
- **WHEN** a user requests `GET /accounts/:id/folders/:folder/messages`
- **THEN** the system returns paginated messages for that specific account and folder

---

### Requirement: Folder tree navigation
The system SHALL expose the folder hierarchy for each account for sidebar navigation.

#### Scenario: Folder list
- **WHEN** a user requests `GET /accounts/:id/folders`
- **THEN** the system returns the folder tree with unread counts per folder

---

### Requirement: Unread counts
The system SHALL maintain and return accurate unread message counts per folder and in aggregate across the unified inbox.

#### Scenario: Unread count update
- **WHEN** a message is marked as read
- **THEN** the unread count for its folder and the unified total decrease by 1

---

### Requirement: Pagination
All message list endpoints SHALL support cursor-based pagination to handle large mailboxes efficiently.

#### Scenario: Paginated list
- **WHEN** a message list endpoint is called with a `cursor` parameter
- **THEN** the response returns the next page of messages and a next cursor token (null if last page)

---

### Requirement: Account badge indicators
The frontend SHALL display per-account unread counts in the account switcher sidebar, mirroring Gmail's account badge behavior.

#### Scenario: Badge shown
- **WHEN** an account has unread messages
- **THEN** the account icon displays the unread count (capped display at 99+)
