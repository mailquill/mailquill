## MODIFIED Requirements

### Requirement: Contact sync
The system SHALL synchronize all pages from every eligible connected contact source into the local database. Synchronization SHALL be incremental using CardDAV sync tokens and ETags, Google sync tokens, or Microsoft Graph delta links; SHALL apply remote deletion tombstones; and SHALL store raw vCard or provider metadata needed for faithful and conflict-safe round trips.

#### Scenario: Initial sync
- **WHEN** a contact source is synchronized without a valid cursor
- **THEN** all provider pages, books/groups, and contacts are fetched and upserted before unseen local rows are pruned and the final cursor is committed

#### Scenario: Incremental sync has no changes
- **WHEN** an incremental provider response contains no upserts or tombstones
- **THEN** local contacts remain unchanged and the returned final cursor is committed

#### Scenario: Incremental sync contains deletion
- **WHEN** a provider reports a deleted contact
- **THEN** the matching local contact and dependent local data are removed without deleting unrelated contacts

#### Scenario: Synchronization is interrupted
- **WHEN** a page request or local page application fails before the final page
- **THEN** the prior durable cursor remains active and a later sync can safely replay the incomplete change set

#### Scenario: Provider cursor expires
- **WHEN** a provider rejects a stored cursor as expired or invalid
- **THEN** the system preserves the current local cache, completes a new full synchronization, and replaces the cursor only after that full run succeeds

### Requirement: Contact fields stored
Each contact SHALL store provider source and book identity, stable remote identifier, remote concurrency version, display name, given name, family name, organization, title, multiple labeled email addresses, multiple labeled phone numbers, multiple postal addresses, notes, group memberships, photo metadata, synchronization timestamps, and provider round-trip metadata required to avoid destructive field loss. All user fields are optional.

#### Scenario: Multi-value fields
- **WHEN** a provider contact has multiple email, phone, or address values
- **THEN** all supported values, labels, and primary markers are stored in normalized form

#### Scenario: Provider has unsupported fields
- **WHEN** a provider contact contains fields outside Mailquill's normalized model
- **THEN** the system preserves the provider representation needed for round trips and does not erase unsupported fields during an unrelated edit where the provider supports field masks

#### Scenario: Remote version changes
- **WHEN** synchronization receives a newer provider version for a contact
- **THEN** the normalized data and stored concurrency version are updated atomically

### Requirement: Create contact
The system SHALL allow users to create a contact in any writable contact book exposed by a mailbox-managed or independent source. At least a display name or a given/family name SHALL be required. Creation SHALL occur remotely first and local persistence SHALL use the provider-returned identifier and version.

#### Scenario: Create contact in selected mailbox
- **WHEN** a user submits a valid contact for a writable mailbox contact book
- **THEN** the provider contact is created and the normalized local contact is stored with its returned identifier, book, and version

#### Scenario: No book is selected
- **WHEN** a user creates a contact while one writable mailbox source is selected but no book is specified
- **THEN** the provider's default writable contact book is used

#### Scenario: Source is read-only
- **WHEN** a user attempts to create a contact in a read-only book or unavailable source
- **THEN** the system rejects the request before changing local state

### Requirement: Edit contact
The system SHALL allow editing supported stored contact fields and group membership on writable sources. Updates SHALL be remote-first, SHALL include the last known provider concurrency version, and SHALL update only explicitly edited provider fields where supported.

#### Scenario: Edit contact email
- **WHEN** a user updates an email address on a contact whose remote version is current
- **THEN** the provider is updated and the returned normalized data and new version are committed locally

#### Scenario: Remote contact changed concurrently
- **WHEN** the provider rejects an update because the stored ETag or version is stale
- **THEN** the API returns a conflict, preserves both remote and submitted data for resolution where possible, and refreshes or schedules refresh of the local contact

#### Scenario: Remote write succeeds but local commit fails
- **WHEN** the provider accepts an update but local persistence fails
- **THEN** the API reports a partial failure and schedules immediate reconciliation instead of claiming the remote write was rolled back

### Requirement: Delete contact
The system SHALL allow deleting contacts from writable sources. Deletion SHALL use the last known provider concurrency version where supported, occur remotely first, and remove the local row only after provider success or an idempotent remote-not-found response.

#### Scenario: Delete contact
- **WHEN** a user deletes a writable contact and the provider confirms deletion
- **THEN** the local contact and dependent memberships/photo cache are removed

#### Scenario: Contact was already deleted remotely
- **WHEN** the provider reports that the selected contact no longer exists
- **THEN** deletion is treated as idempotent and the stale local contact is removed

#### Scenario: Delete conflicts with remote change
- **WHEN** the provider rejects deletion because the stored version is stale
- **THEN** the API returns a conflict and retains the local row until reconciliation

### Requirement: Contact photo
Contact photos SHALL be synchronized lazily and stored in the blob store with provider version metadata. The contact photo endpoint SHALL fetch from CardDAV, Google People, or Microsoft Graph as appropriate and SHALL invalidate cached bytes when synchronization reports a changed photo version.

#### Scenario: Photo fetch on demand
- **WHEN** a contact detail view requests a photo that is not cached
- **THEN** the system fetches it through the contact's provider, stores it, and returns the provider content type

#### Scenario: Photo changed remotely
- **WHEN** contact synchronization reports a different photo reference or version
- **THEN** the stale cached photo is invalidated and the next request fetches the new photo

#### Scenario: Contact has no photo
- **WHEN** the provider reports no photo
- **THEN** the endpoint returns not found and the frontend renders a deterministic placeholder avatar

### Requirement: Contact list view
The frontend SHALL provide a contacts list with search, alphabetical grouping, and counts/filtering by mailbox source and contact book. Mailbox-managed sources SHALL be labeled with their owning mailbox identity and provider status.

#### Scenario: Filter by account
- **WHEN** a user selects a mailbox or independent contact source in the contacts sidebar
- **THEN** only contacts from that source are shown and its books remain available as narrower filters

#### Scenario: Source needs attention
- **WHEN** a selected source requires consent, reauthentication, or configuration
- **THEN** the contact list preserves cached contacts and shows the actionable source state without presenting the cache as freshly synchronized

#### Scenario: No contact source is enabled
- **WHEN** the user opens Contacts with connected mailboxes but no enabled contact source
- **THEN** the empty state explains contact sync, lists eligible mailboxes, and offers mailbox-scoped enable or repair actions instead of a generic empty list

#### Scenario: Disabled source retained cached contacts
- **WHEN** a user views contacts from a disabled source whose local cache was retained
- **THEN** the list labels them as read-only and offers a clear re-enable action

### Requirement: Compose and calendar integration
The autocomplete endpoint SHALL serve provider-neutral contacts to compose To/Cc/Bcc fields and calendar attendee inputs. It SHALL support account/book context for filtering and ranking while preserving cross-account search when no context is supplied.

#### Scenario: Autocomplete in compose
- **WHEN** a user types in a compose recipient field for a selected sending mailbox
- **THEN** matching contacts from that mailbox rank first, followed by matches from other available sources

#### Scenario: Autocomplete in calendar attendees
- **WHEN** a user types in a calendar attendee field associated with a mailbox calendar
- **THEN** matching contacts from the associated mailbox rank first and selecting one fills its display name and primary email address

#### Scenario: Provider is temporarily unavailable
- **WHEN** autocomplete runs while a source is offline or awaiting reauthentication
- **THEN** locally cached contacts remain searchable and the request does not trigger a provider call

## ADDED Requirements

### Requirement: Contact books and groups
The system SHALL synchronize provider contact books/folders and groups with stable remote identifiers, names, writability, hierarchy where available, membership, and per-book cursor state where required. Provider system groups that cannot be modified SHALL be exposed as read-only.

#### Scenario: Microsoft contact folders are synchronized
- **WHEN** Graph returns default, child, added, renamed, or deleted contact folders
- **THEN** local books and their contact membership reflect those changes without conflating their delta cursors

#### Scenario: Google group is read-only
- **WHEN** Google marks a system contact group as non-writable
- **THEN** the UI displays its membership but disables unsupported rename/delete or membership mutations

#### Scenario: CardDAV exposes multiple address books
- **WHEN** CardDAV discovery returns multiple address-book collections
- **THEN** each collection is represented as a book under the mailbox source and synchronized independently

### Requirement: Provider synchronization is bounded and observable
Contact synchronization SHALL respect provider pagination, retry guidance, and rate limits; serialize mutations when required by the provider; and emit structured provider/source/page/error metrics without logging credentials or contact content.

#### Scenario: Provider returns continuation
- **WHEN** a page contains a provider continuation URL or token
- **THEN** the sync follows the opaque continuation and does not treat it as the final durable cursor

#### Scenario: Provider rate limits synchronization
- **WHEN** the provider returns a retryable rate-limit response
- **THEN** the task preserves its cursor and retries after the provider delay or bounded exponential backoff

#### Scenario: Sync error is logged
- **WHEN** a provider page fails
- **THEN** logs and metrics identify the user-safe source ID, provider, operation, and error category without tokens or contact payloads
