# contacts Specification

## Purpose
TBD - created by archiving change contacts. Update Purpose after archive.
## Requirements
### Requirement: Contact sync
The system SHALL sync contacts from all connected contact accounts into the local DB. Sync SHALL be incremental using `sync-token` (CardDAV / Google) or delta links (Graph). Raw vCard SHALL be stored for faithful CardDAV round-trips.

#### Scenario: Initial sync
- **WHEN** a contact account is first connected
- **THEN** all contacts are fetched, parsed (vCard 3.0 and 4.0), and stored

#### Scenario: Incremental sync
- **WHEN** a sync runs and the server sync-token is unchanged
- **THEN** no contact data is re-fetched

---

### Requirement: Contact fields stored
Each contact SHALL store: display_name, given_name, family_name, organization, title, email addresses (multiple, with label), phone numbers (multiple, with label), postal addresses (multiple), notes, and photo_path. All fields are optional.

#### Scenario: Multi-value fields
- **WHEN** a vCard has multiple EMAIL or TEL entries
- **THEN** all values are stored in the JSON arrays with their type labels (work, home, etc.)

---

### Requirement: Contact autocomplete
The system SHALL provide a fuzzy-search autocomplete endpoint (`GET /contacts/search?q=<term>`) searching across display_name, given_name, family_name, and all stored email addresses. Results are returned ranked by relevance, limited to 10 results.

#### Scenario: Search by partial name
- **WHEN** a user types "joh" in the compose To field
- **THEN** contacts matching "John", "Johnson", etc. are returned

#### Scenario: Search by email prefix
- **WHEN** a user types "john@ex"
- **THEN** contacts with email addresses matching that prefix are returned

#### Scenario: No match
- **WHEN** the query matches no contacts
- **THEN** an empty array is returned (not an error)

---

### Requirement: Create contact
The system SHALL allow users to create a new contact in any writable connected contact account. Required: at least one of display_name or given_name + family_name.

#### Scenario: Create contact
- **WHEN** a user submits a new contact to `POST /contacts`
- **THEN** the contact is created in local DB and written to the remote account (CardDAV PUT or Graph POST)

---

### Requirement: Edit contact
The system SHALL allow editing any stored contact field. Edits are written back to the remote account.

#### Scenario: Edit contact email
- **WHEN** a user updates an email address on a contact
- **THEN** the change is saved locally and PUTed back to CardDAV (or PATCHed via Graph/Google)

---

### Requirement: Delete contact
The system SHALL allow deleting contacts. Deletion writes through to the remote account.

#### Scenario: Delete contact
- **WHEN** a user deletes a contact
- **THEN** the contact is removed from local DB and deleted on the remote account

---

### Requirement: Contact photo
Contact photos SHALL be fetched lazily (not during bulk sync) and stored on disk. A `GET /contacts/:id/photo` endpoint returns the photo. A placeholder avatar is shown when no photo is available.

#### Scenario: Photo fetch on demand
- **WHEN** a contact detail view is opened and photo_path is null
- **THEN** the system attempts to fetch the photo from the remote account, stores it, and returns it

---

### Requirement: Contact detail view
The frontend SHALL show a contact detail panel with all stored fields, grouped by type (emails, phones, addresses), an avatar/photo, and action buttons: compose email, edit, delete.

#### Scenario: Compose from contact
- **WHEN** a user clicks "Compose" on a contact's email address
- **THEN** a compose window opens with that address pre-filled in To

---

### Requirement: Contact list view
The frontend SHALL provide a contacts list with search/filter, alphabetical grouping, and a count of contacts per account.

#### Scenario: Filter by account
- **WHEN** a user selects a contact account in the sidebar
- **THEN** only contacts from that account are shown

---

### Requirement: Compose and calendar integration
The autocomplete endpoint SHALL be called from the compose To/Cc/Bcc fields and from the calendar event attendee input. Selecting a contact autocomplete result SHALL populate the field with the contact's name and primary email address.

#### Scenario: Autocomplete in compose
- **WHEN** a user types in a compose recipient field
- **THEN** matching contacts appear as a dropdown; selecting one fills the field

#### Scenario: Autocomplete in calendar attendees
- **WHEN** a user types in the calendar event attendees field
- **THEN** matching contacts appear as a dropdown

