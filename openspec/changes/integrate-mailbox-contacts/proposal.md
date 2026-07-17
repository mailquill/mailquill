## Why

Mail contacts are currently configured as separate accounts with duplicated credentials or manually supplied access tokens, even though the connected mailbox already determines the correct provider and owns the renewable credentials. This makes Google and Microsoft contact sync fragile, leaves generic mailbox contacts disconnected from CardDAV settings, and exposes incomplete incremental-sync behavior.

## What Changes

- Derive one managed contact source from each mailbox account: Google OAuth mailboxes use Google People API, Microsoft OAuth mailboxes use Microsoft Graph Contacts, and other mailboxes use configured or discovered CardDAV.
- Reuse mailbox credentials, OAuth refresh and reauthentication state, TLS trust settings, and account lifecycle instead of storing independently entered provider tokens.
- Request the provider contact scopes needed for two-way synchronization and guide existing OAuth accounts through re-consent when those grants are absent.
- Synchronize complete provider datasets with pagination, incremental cursors, remote deletion tombstones, stable identifiers, conflict protection, and transactional cursor advancement.
- Provide provider-neutral contact books/groups, fields, photos, search, autocomplete, two-way create/update/delete, sync status, and actionable recovery states.
- Integrate mailbox-owned contacts into account settings, the contacts workspace, compose recipient fields, and calendar attendees without requiring a second account setup flow.
- Provide one guided, low-friction UX for both paths: enabling contacts during first-time mailbox setup and enabling or repairing contacts later for an existing mailbox.
- Replace raw provider/token configuration with plain-language capability choices, automatic provider discovery, testable defaults, progress feedback, actionable recovery, and a safe non-destructive disable flow.
- Migrate or link compatible existing contact accounts without duplicating contacts; keep explicitly independent CardDAV sources supported where they are not tied to a mailbox.

## Capabilities

### New Capabilities

- `mailbox-contact-integration`: Provider selection, mailbox-to-contact-source ownership, shared credentials, lifecycle coupling, migration, and user-visible capability/recovery states.

### Modified Capabilities

- `contact-accounts`: Replace separately entered Google/Microsoft tokens with mailbox-managed sources while retaining independent CardDAV sources where needed.
- `contacts`: Strengthen provider-neutral synchronization, pagination, deletions, conflicts, groups, photos, two-way writes, search, and consumer integration requirements.
- `email-account-management`: Extend mailbox OAuth consent, reconnect, update, and deletion behavior to manage the account's contact capability and source.

## Impact

- Backend: `backend/contact-sync`, OAuth token handling, mailbox/contact routes, sync managers, provider adapters, and application startup/lifecycle wiring.
- Storage: per-user contact account/source relationships, provider cursors and versions, remote tombstones, groups, migration/link metadata, and constraints preventing duplicate managed sources.
- API and frontend: mailbox/contact account representations, first-time mailbox wizard, later enablement in account settings and contacts empty states, status/re-consent/recovery flows, contacts UI, compose autocomplete, and calendar attendee selection.
- External systems: Google People API, Microsoft Graph Contacts, and CardDAV servers; OAuth application registrations require contact read/write scopes.
- Existing uncommitted mail-search and OAuth-notification changes remain outside this proposal.
