## ADDED Requirements

### Requirement: Full-text message search
The system SHALL support full-text search across message subjects, sender/recipient headers, and body text for all messages belonging to the authenticated user.

#### Scenario: Search across all accounts
- **WHEN** a user submits a search query
- **THEN** the system returns matching messages from all connected accounts, ranked by relevance

#### Scenario: Search scoped to account
- **WHEN** a user submits a search with an account filter
- **THEN** only messages from that account are included in results

---

### Requirement: Header search filters
The system SHALL support filtering search results by: from address, to address, subject, date range, folder, read/unread state, and flagged state.

#### Scenario: From filter
- **WHEN** a user searches with `from:user@example.com`
- **THEN** only messages with that sender are returned

#### Scenario: Date range filter
- **WHEN** a user searches with `after:2024-01-01 before:2024-06-01`
- **THEN** only messages within that date range are returned

---

### Requirement: Search result pagination
Search results SHALL be paginated with a maximum of 50 results per page.

#### Scenario: Paginated search
- **WHEN** search returns more than 50 matches
- **THEN** the response includes a cursor for the next page

---

### Requirement: Search index freshness
Newly synced messages SHALL be indexed for search within one sync cycle of being stored.

#### Scenario: New message searchable
- **WHEN** a message is synced and stored
- **THEN** it appears in search results on the next search query after storage
