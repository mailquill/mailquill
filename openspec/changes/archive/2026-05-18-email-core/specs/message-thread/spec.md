## ADDED Requirements

### Requirement: Conversation threading
The system SHALL group messages into threads using Message-ID, In-Reply-To, and References headers. If the IMAP server advertises `THREAD=REFERENCES` capability (RFC 5256), the server-computed thread tree SHALL be used. Otherwise the system SHALL compute threads client-side using the JWZ algorithm. Thread ID is derived as a deterministic hash of the root Message-ID and stored on each message.

#### Scenario: Thread grouping via References chain
- **WHEN** messages share a References or In-Reply-To chain
- **THEN** they are grouped into a single thread visible as one row in the message list, sorted by date of the most recent message

#### Scenario: Thread grouping via IMAP THREAD command
- **WHEN** the IMAP server reports `THREAD=REFERENCES` in its CAPABILITY response
- **THEN** the sync engine issues `UID THREAD REFERENCES UTF-8 ALL` and assigns thread IDs from the server-returned tree rather than computing them locally

#### Scenario: Thread row summary in message list
- **WHEN** a thread contains more than one message
- **THEN** the list row shows participant names (up to 3), message count, latest message snippet, and a per-thread unread badge

#### Scenario: Unread count per thread
- **WHEN** a thread has unread messages
- **THEN** the unread count reflects the number of unread messages in the thread, not just the latest

---

### Requirement: Mailing list threading
The system SHALL detect mailing list messages via the `List-Id` header and group them by list ID and normalised subject when no References chain connects them. The mailing list name SHALL be shown as a badge in the thread view.

#### Scenario: Mailing list thread grouping
- **WHEN** multiple messages share the same `List-Id` value and normalised subject (Re:/Fwd: stripped)
- **THEN** they are grouped into a single thread even if their References chains are disjoint

#### Scenario: Mailing list badge
- **WHEN** a thread originates from a mailing list
- **THEN** the list name extracted from `List-Id` is shown as a label on the thread row and thread detail header

---

### Requirement: Thread conversation view
The system SHALL display the full thread as a stacked conversation (Gmail-style): all messages in the thread visible in one view, collapsed to sender + snippet, expandable to full body on click. The most recent unread message SHALL be expanded by default.

#### Scenario: Expand single message in thread
- **WHEN** a user clicks a collapsed message in the thread view
- **THEN** the full message body is shown inline (body fetched on demand if not cached)

#### Scenario: Latest unread auto-expanded
- **WHEN** a thread is opened
- **THEN** the most recent unread message (or the most recent if all are read) is expanded automatically; all others start collapsed

#### Scenario: Cross-folder thread
- **WHEN** messages in the same thread exist in different folders (e.g. INBOX and Sent)
- **THEN** all messages appear in the thread view regardless of folder; a folder label is shown on each message

---

### Requirement: Thread-level actions
The system SHALL support acting on an entire thread at once: archive all, delete all, mark all as read, mark all as unread.

#### Scenario: Archive entire thread
- **WHEN** a user archives a thread
- **THEN** all messages in the thread are moved to the Archive folder

#### Scenario: Mark thread as read
- **WHEN** a user marks a thread as read
- **THEN** all messages in the thread are marked read and IMAP \\Seen flags are set

---

### Requirement: Message body on demand
When a message is opened and its body has not been fetched (`message_bodies` row absent), the system SHALL fetch the body from IMAP and write it to blob storage. The frontend SHALL show a loading state during fetch. If offline and no blob is stored, the frontend SHALL display "Body not available offline" with a "Download when online" option.

#### Scenario: Body fetched on open (lazy, online)
- **WHEN** a user opens a message from a lazy account and no body blob exists
- **THEN** the backend fetches the body from IMAP, writes it to blob storage, and returns it; subsequent opens stream from blob storage

#### Scenario: Body unavailable offline (lazy, offline)
- **WHEN** a user opens a message from a lazy account while offline and no body blob is stored
- **THEN** the frontend shows "Body not available offline" and does not mark the message as read

#### Scenario: Body already stored
- **WHEN** a user opens any message whose body is already in blob storage (`message_bodies` pointer row present)
- **THEN** the body is streamed from blob storage with no IMAP connection

---

### Requirement: Read / unread state
The system SHALL track read/unread state per message, synchronized with the IMAP \\Seen flag.

#### Scenario: Mark as read
- **WHEN** a user opens a message and its body is successfully loaded
- **THEN** the message is marked read locally and the IMAP \\Seen flag is set on the server on next sync

#### Scenario: Mark as unread
- **WHEN** a user explicitly marks a message as unread
- **THEN** the \\Seen flag is removed on the IMAP server

---

### Requirement: Flag (star) messages
The system SHALL support flagging messages (IMAP \\Flagged), displayed as stars in the UI.

#### Scenario: Flag message
- **WHEN** a user stars a message
- **THEN** the \\Flagged flag is set on the IMAP server

---

### Requirement: Archive message
The system SHALL support archiving messages by moving them to the account's Archive folder (or applying the \\Archive label for providers that support it).

#### Scenario: Archive
- **WHEN** a user archives a message
- **THEN** the message is moved from INBOX to the Archive folder via IMAP MOVE command

---

### Requirement: Delete message
The system SHALL support deleting messages by moving them to Trash (soft delete) or expunging (hard delete).

#### Scenario: Soft delete
- **WHEN** a user deletes a message
- **THEN** the message is moved to the account's Trash folder

#### Scenario: Hard delete from Trash
- **WHEN** a user deletes a message already in Trash
- **THEN** the \\Deleted flag is set and EXPUNGE is issued

---

### Requirement: Move to folder
The system SHALL support moving messages between folders via IMAP MOVE command.

#### Scenario: Move message
- **WHEN** a user selects a destination folder and moves a message
- **THEN** the message appears in the destination folder and is removed from the source
