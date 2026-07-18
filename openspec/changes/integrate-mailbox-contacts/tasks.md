## 1. Storage and Domain Model

- [x] 1.1 Add a mail-database migration for mailbox-owned contact sources, management/capability state, unique email-account linkage, provider cursor metadata, sync generations, remote contact versions, photo versions, and contact books/groups with cascade-safe foreign keys and indexes.
- [x] 1.2 Implement idempotent migration/link logic that preserves existing contact rows and cursors, links only unambiguous legacy Google/Microsoft/CardDAV sources, and leaves ambiguous sources independent.
- [x] 1.3 Extend provider-neutral contact-sync domain types with source/book identity, primary markers, remote versions, tombstones, photo metadata, group memberships, continuation state, and categorized provider errors.
- [x] 1.4 Add SQLite repository operations and tests for source reconciliation, paged upsert/delete application, full-sync generation pruning, per-book cursors, transactional final-cursor advancement, and migration deduplication.

## 2. Shared Credentials and Mailbox Reconciliation

- [x] 2.1 Extend OAuth consent and encrypted credential metadata to request/store Google contacts read/write and Microsoft `Contacts.ReadWrite` grants while preserving mail operation when contact consent is missing.
- [x] 2.2 Expose the centralized fresh-token and permanent reauthentication behavior to mailbox-managed contact sync, including granted-scope checks and sanitized `consent_required` versus `reauth_required` outcomes.
- [x] 2.3 Implement a mailbox-contact reconciler that selects Google People, Microsoft Graph, CardDAV, or unavailable from authoritative mailbox metadata and upserts at most one managed source idempotently.
- [x] 2.4 Reuse mailbox CardDAV credentials and DAV TLS trust/certificate policy without copying secrets into managed contact-source rows.
- [x] 2.5 Add tests for provider selection, scope-state classification, credential isolation, repeated reconciliation, generic mailbox CardDAV selection, and unsupported mailbox behavior.

## 3. Provider Change Adapters

- [x] 3.1 Define a common paged provider adapter contract for contact books/groups, contact upserts, tombstones, opaque continuation values, final cursors, photos, and versioned mutations.
- [x] 3.2 Upgrade CardDAV discovery to enumerate multiple address books, capability-detect sync-collection support, retain href/ETag identity, parse deletion responses, and fall back to full address-book queries safely.
- [x] 3.3 Add conditional CardDAV create/update/delete and photo handling with ETag/`If-Match`, faithful vCard preservation, idempotent not-found deletion, and deterministic mock-server tests.
- [x] 3.4 Upgrade Google People sync to preserve fixed request parameters, exhaust page tokens, commit only final sync tokens, apply deleted-person tombstones, detect expired sync tokens, and synchronize contact groups/memberships.
- [x] 3.5 Add Google People versioned create/update/delete/photo operations using contact-source ETags and explicit field masks, serialize mutations per source, and cover pagination, expiry, deletion, conflict, and rate-limit responses with tests.
- [x] 3.6 Upgrade Microsoft Graph sync to enumerate contact folders, follow opaque folder/contact next links through final delta links, apply `@removed` tombstones, and retain per-folder cursor state.
- [x] 3.7 Add Microsoft Graph versioned create/update/delete/photo operations for default and nested contact folders with conditional requests where supported, plus tests for pagination, folder changes, deletion, conflict, and throttling.

## 4. Synchronization Orchestration

- [x] 4.1 Replace delete-and-reinsert contact sync with transactional initial/incremental orchestration that exhausts every provider page and advances only final durable cursors.
- [x] 4.2 Implement full-sync generations that retain the existing cache until a complete run succeeds, then prune unseen contacts/books/groups and invalidate changed photo blobs.
- [x] 4.3 Implement incremental upsert/tombstone application, expired-cursor full-sync fallback, interruption replay safety, and immediate reconciliation after partial remote-write failures.
- [x] 4.4 Make contact sync tasks restart-safe at application startup with durable states, manual triggers, bounded exponential backoff/Retry-After handling, and paused consent/reauth states.
- [x] 4.5 Publish credential-free contact sync status events and structured provider/source/operation metrics without logging tokens or contact payloads.
- [x] 4.6 Add orchestration tests for multi-page initial sync, no-change delta, deletion delta, page failure, cursor expiry, rate limiting, startup resume, task cancellation, and status transitions.

## 5. API and Mailbox Lifecycle

- [x] 5.1 Extend email-account list/detail responses with the managed contact capability summary and reconcile/start sources after mailbox create, OAuth callback/reconnect, relevant account updates, and startup.
- [x] 5.2 Update mailbox deletion to stop the managed contact task and cascade local source/contact/book/group/photo data without issuing remote contact deletions or touching independent sources.
- [x] 5.3 Update contact-source endpoints to list managed and independent sources, retain independent CardDAV creation/deletion, reject manual Google/Microsoft token sources, and prevent direct deletion of managed sources.
- [x] 5.4 Rework contact create/update/delete routes for writable book selection, provider-returned IDs/versions, remote-first conditional mutations, idempotent deletion, conflict responses, and partial-failure reconciliation.
- [x] 5.5 Implement provider-aware lazy contact-photo fetch/cache/invalidation for CardDAV, Google People, and Microsoft Graph with correct content types and placeholder-compatible not-found behavior.
- [x] 5.6 Extend contact list/search/autocomplete APIs with mailbox and book filters, mailbox-context ranking, cached offline behavior, source writability/status, groups, and stable pagination/counts.
- [x] 5.7 Add API integration tests for user isolation, capability summaries, consent/reconnect states, source lifecycle, independent CardDAV behavior, conflict-safe CRUD, books/groups, photos, and compose/calendar autocomplete context.
- [x] 5.8 Add mailbox-scoped enable, disable-with-retention, disable-and-remove-cache, discovery/test, and OAuth return-context endpoints with idempotency and no provider-side deletion.

## 6. Frontend Integration

- [x] 6.1 Extend frontend account/contact schemas and query hooks for managed-source ownership, provider/book metadata, all capability states/actions including `disabled`, pagination, credential-free sync events, and OAuth return context.
- [x] 6.2 Add a concise Contacts capability review to first-time mailbox setup with provider-derived defaults, an explicit sync toggle, plain-language permission explanation, CardDAV availability, and a continue-without-contacts path.
- [x] 6.3 Add a mailbox completion state that separates mail success from contact attention, shows non-blocking initial-sync progress, and provides “View contacts”, “Fix contacts”, and “Set up later” actions as applicable.
- [x] 6.4 Add an always-visible Contacts row to each existing mailbox card showing provider, labeled state, last sync, and one primary enable/sync/reconnect/fix action without requiring mailbox recreation.
- [x] 6.5 Build the later CardDAV setup assistant to try discovery with stored credentials first, select discovered address books, preserve form values on failure, reuse the explicit TLS trust flow, and reveal manual URL fields only under an advanced disclosure.
- [x] 6.6 Replace the Contacts workspace's generic no-account/empty state with eligible mailbox cards and the same enable/fix actions, while preserving cached contacts and actionable attention banners.
- [x] 6.7 Implement a non-destructive disable dialog with “keep downloaded contacts” as the default, separately confirmed local-cache removal, read-only cache labeling, and re-enable behavior.
- [x] 6.8 Remove manual Google/Microsoft token entry from contact-source creation while retaining an advanced independent CardDAV flow with actionable discovery/auth/TLS errors.
- [x] 6.9 Update the populated contacts workspace to group/filter by mailbox and book, display group membership and photos, surface sync progress/errors without color-only meaning, and default creation to the selected writable book.
- [x] 6.10 Wire mailbox-context contact ranking into compose To/Cc/Bcc autocomplete and calendar attendee inputs while retaining cross-account fallback results.
- [x] 6.11 Add English and German copy plus accessible labels, focus restoration, associated validation errors, live status announcements, keyboard coverage, reduced-motion handling, and semantic-token styling for every setup/recovery state.
- [x] 6.12 Add frontend integration and end-to-end tests for first-time opt-in/skip, later OAuth re-consent return, automatic/manual CardDAV setup, TLS recovery, progress, error recovery, disable/cache choices, empty-state entry, and compose/calendar autocomplete selection.

## 7. Validation, Migration, and Documentation

- [x] 7.1 Add migration fixture tests for fresh databases, linked legacy provider sources, ambiguous independent sources, repeated migration/reconciliation, and mailbox deletion cascades.
- [x] 7.2 Run provider, repository, API, frontend integration/E2E, keyboard, and accessibility test suites plus warning-free Rust checks, clippy, frontend typecheck/lint/build, and strict OpenSpec validation; resolve all failures in changed code.
- [x] 7.3 Document OAuth application scope changes, Google People API enablement, Microsoft Graph delegated permissions, CardDAV/TLS behavior, re-consent rollout, operational status/error categories, and rollback steps.
- [ ] 7.4 Verify the staged rollout against representative Google, Microsoft work/school and personal, multi-book CardDAV, expired-consent, rate-limit, and large-address-book accounts without logging contact content.
