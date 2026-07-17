## Context

Mailquill already has a provider-neutral contact model, CardDAV/Google People/Microsoft Graph HTTP primitives, contact CRUD routes, and a `contact_accounts` table. However, Google and Microsoft contact accounts currently require separately supplied access tokens, token refresh is not shared with mailbox OAuth, provider pages are not exhausted, incremental pages replace the whole local set, and mailbox lifecycle does not own contact lifecycle. The existing mailbox record already carries provider identity, encrypted credentials, CardDAV configuration, and TLS trust policy, so it is the authoritative integration boundary.

The implementation spans OAuth consent, per-user SQLite migrations, provider adapters, background synchronization, REST contracts, and the contacts/settings/compose/calendar UI. It must preserve user isolation, encrypted credentials, existing independent CardDAV sources, and the current provider-neutral contact API.

## Goals / Non-Goals

**Goals:**

- Create and maintain at most one mailbox-managed contact source for each mailbox account that supports contacts.
- Select Google People, Microsoft Graph, or CardDAV from authoritative mailbox/provider metadata rather than hostname guessing.
- Share renewable credentials, reauthentication state, TLS exceptions, and lifecycle with the mailbox.
- Provide complete, restart-safe, incremental, two-way synchronization including pagination, deletions, groups/books, photos, and optimistic concurrency.
- Preserve provider-neutral contact search, contact management, compose autocomplete, and calendar attendee use.
- Migrate compatible existing sources without duplicating remote contacts or losing independent CardDAV sources.
- Make first-time and later contact setup understandable without requiring provider protocol knowledge, copied tokens, or mailbox recreation.

**Non-Goals:**

- Synchronizing Google Workspace directory profiles, Microsoft organization directory users, social/profile-only people, or suggested/other contacts.
- Automatically merging contacts across different mailbox accounts or providers.
- Adding Exchange Web Services as an Outlook fallback.
- Providing an offline mutation queue; writes remain synchronous remote-first operations.
- Changing mail, calendar, or contact ownership across Mailquill users.

## Decisions

### 1. Model mailbox ownership explicitly

Add a nullable `email_account_id` foreign key to `contact_accounts` with a unique constraint for mailbox-managed sources, plus an explicit management mode (`mailbox` or `independent`). A mailbox-managed source stores provider configuration and sync state but no duplicate mailbox secret. Independent CardDAV sources continue to own encrypted credentials.

This keeps the existing contact-account boundary and APIs usable while making ownership enforceable in storage. Replacing `contact_accounts` entirely with fields on `email_accounts` was rejected because independent CardDAV address books and multiple provider books still need their own sync identity and cursor state.

### 2. Resolve provider from mailbox identity and capability

Provider selection follows this order:

1. A mailbox whose encrypted OAuth metadata identifies `google` uses Google People API.
2. A mailbox whose encrypted OAuth metadata identifies `microsoft` uses Microsoft Graph Contacts.
3. Any other mailbox with a configured or successfully discovered CardDAV collection uses CardDAV.
4. A mailbox without a supported source remains `unavailable` and does not create a failing sync loop.

`provider_kind`, OAuth provider metadata, and explicit CardDAV configuration are authoritative; IMAP/SMTP hostnames are only inputs to CardDAV discovery, never proof that a Google or Microsoft API token exists.

### 3. Share credential refresh and reauthentication

Mailbox-managed Google and Microsoft adapters request a fresh access token through the same centralized OAuth token service used by mail. Google requests the read/write `https://www.googleapis.com/auth/contacts` scope; Microsoft requests delegated `Contacts.ReadWrite`. OAuth credential metadata records granted scopes so the API can distinguish `ready`, `consent_required`, and `reauth_required` instead of repeatedly failing sync.

Existing OAuth accounts missing contact grants remain usable for mail and expose a reconnect action that requests the expanded consent set. CardDAV uses the mailbox's encrypted username/password or bearer credential and the same stored certificate/TLS exception policy as DAV calendar sync. Contact-source rows never expose or copy secrets in API responses.

### 4. Use a common paged change protocol

Each provider adapter returns provider-neutral pages containing upserts, deletion tombstones, book/group changes, a continuation URL/token, and a final durable cursor. The orchestration layer follows every continuation before committing the final cursor.

- Google keeps request parameters stable across page/sync tokens, stores the final `nextSyncToken`, applies `metadata.deleted` tombstones, and falls back to a full synchronization when the provider reports an expired sync token.
- Microsoft follows opaque `@odata.nextLink` values until the final `@odata.deltaLink`. Contact-folder/book cursors are tracked separately where the Graph API requires folder-scoped contact deltas.
- CardDAV uses collection discovery, sync-collection reports when supported, href plus ETag identities, and a full address-book query fallback when the server lacks sync tokens.

During a full synchronization, contacts are marked with a run generation and unseen rows are pruned only after all pages complete. During an incremental synchronization, only explicit upserts and tombstones are applied. All page mutations and final cursor advancement occur transactionally so interruption cannot skip remote changes.

### 5. Persist remote versions and provider metadata

Contacts retain a stable provider resource identifier and gain remote version/concurrency metadata (CardDAV ETag, Google contact-source ETag, or Graph change key/ETag), deletion state as needed during a transaction, source book/group membership, photo metadata, and last-seen sync generation. Provider-specific opaque values remain isolated behind the provider adapter while normalized fields continue to drive the API and FTS index.

Contact books/groups are synchronized as first-class records. Provider system groups that cannot be changed remain read-only; writable books/groups expose supported mutations. Unsupported provider fields are preserved in raw vCard or provider metadata where feasible and are not silently overwritten by unrelated edits.

### 6. Perform conflict-safe remote-first writes

Create, update, and delete validate ownership and source writability, perform the provider mutation first, then commit the returned identifier/version and normalized local state. Updates include the last known provider version (`If-Match` for CardDAV/Graph where supported and Google contact-source ETag). A stale version returns a conflict response, refreshes or schedules refresh of the local contact, and does not overwrite the remote change.

Provider mutations for one source are serialized where provider behavior requires it. Local failures after a successful remote mutation trigger an immediate reconciliation sync and an actionable error rather than pretending the write was rolled back remotely.

### 7. Couple source and task lifecycle to the mailbox

Mailbox creation or OAuth callback reconciles its managed contact source and starts sync after capability validation. Mailbox update/reconnect re-evaluates provider, URL, credentials, consent, and TLS policy without changing the source identity unnecessarily. Application startup starts tasks for eligible managed and independent sources. Mailbox deletion stops the task and cascade-deletes only its managed source and local contact cache.

Sync state is durable (`disabled`, `pending`, `syncing`, `idle`, `consent_required`, `reauth_required`, `error`, `unavailable`) and also published to open clients. Backoff uses bounded exponential retry for transient provider failures; disabled, auth, and consent states pause until user action.

### 8. Present contacts as a mailbox capability

Account settings show the selected contact provider, last sync, errors, consent/reconnect action, CardDAV discovery/configuration, and a manual sync action. The contacts workspace groups/filter contacts by mailbox and book while retaining independent CardDAV sources. Creating a contact defaults to the currently selected writable mailbox/book. Compose and calendar continue using one provider-neutral autocomplete endpoint, with account/book context available for ranking and filtering.

The separate Google/Microsoft token-entry UI is removed once mailbox-managed equivalents are available. Independent CardDAV creation remains available under an advanced contact-source flow.

### 9. Use guided first-time and later setup journeys

The UX treats contact synchronization as an optional mailbox capability, not a separate account type. Provider names can be shown for transparency, but primary labels describe the user outcome: “Sync contacts”, “Enable contacts”, “Reconnect”, “Try again”, and “Set up CardDAV”. Raw URLs, protocol names, and diagnostic detail stay behind an advanced disclosure unless user action requires them.

**First-time mailbox setup:**

1. After provider discovery, the existing mailbox wizard shows a capability review for Mail, Calendar, and Contacts before account creation.
2. Contacts are offered with a clearly labeled toggle. For Google/Microsoft the provider and required contact access are known before OAuth; one sign-in requests the selected capabilities. For password-based mailboxes the wizard proposes CardDAV from discovery and verifies it only after credentials are available.
3. The user can continue with mail when contact consent is declined or CardDAV cannot be verified. The completion screen distinguishes “mailbox connected” from “contacts need attention” and offers a direct fix without making the successful mailbox look failed.
4. Successful setup starts initial contact sync in the background. The completion screen shows progress and a “View contacts” action but does not block on downloading a large address book.

**Enablement after mailbox creation:**

1. Every account card exposes a compact Contacts row with provider, state, last sync, and one primary action. A user never needs to delete or recreate the mailbox.
2. “Enable contacts” for an existing Google/Microsoft mailbox launches scoped re-consent for that same account and returns to its Contacts row with preserved context and progress.
3. “Enable contacts” for a generic mailbox opens a short assistant that first tries discovery with stored credentials, shows the discovered address books, and asks for a URL only under “Advanced setup”. TLS trust failures reuse the existing explicit certificate confirmation pattern.
4. The Contacts workspace empty state lists connected mailboxes and offers the same enable/fix action, so users can recover even if they do not know where account settings live.
5. Error states use a plain-language summary plus one recommended action; technical details are expandable and copyable. Reauthentication and missing consent are never presented as generic transport errors.

**Control and feedback:**

- Disabling sync explains that remote contacts are never deleted and offers two explicit choices: keep downloaded contacts locally as a read-only cache (default) or remove this mailbox's local contact cache. Destructive local removal requires confirmation.
- Initial and manual sync show non-blocking progress, last successful sync, and completion/failure in the account row and Contacts workspace. Status is not conveyed by color alone and important changes use an accessible live region.
- Forms preserve entered values across provider redirects and recoverable failures, return keyboard focus to the initiating control/dialog, associate field errors programmatically, and provide fully localized English/German labels and help text.
- The implementation uses existing semantic design tokens and shared accessible primitives; it does not add raw status colors, protocol-heavy default forms, or a second standalone Google/Microsoft contact-account wizard.

### 10. Verify contracts at provider and application boundaries

Provider HTTP tests use deterministic mock responses for pagination, opaque continuation URLs, tombstones, expired cursors, rate limits, photos, groups, and version conflicts. Repository tests cover transactional cursor safety and migration deduplication. API tests cover isolation, capability states, lifecycle, write conflicts, and OAuth re-consent. Frontend integration and end-to-end tests cover first-time opt-in/skip, later enablement, provider redirect return, CardDAV discovery/manual fallback, disable/cache choices, accessible status/error handling, and contact use in compose/calendar.

## Risks / Trade-offs

- [Expanded OAuth consent can concern existing users] -> Keep mail functional, explain the contacts permission separately, and require explicit reconnect/re-consent before enabling contacts.
- [Provider models do not map losslessly] -> Keep normalized fields conservative, preserve raw/provider metadata, and update only explicitly edited field masks.
- [Incorrect cursor handling can delete or skip contacts] -> Exhaust all pages, apply changes transactionally, advance only final cursors, and use full-sync generations before pruning.
- [CardDAV implementations vary] -> Retain discovery and full-query fallbacks, capability-detect sync collections, and reuse existing TLS diagnostics/trust handling.
- [A remote write can succeed before local persistence fails] -> Return an actionable partial-failure response and schedule immediate reconciliation.
- [Large address books can stress SQLite and provider quotas] -> Bound page processing, batch local writes in transactions, respect retry headers/backoff, and avoid full sync unless the cursor is absent or invalid.
- [Adding contact choices can make mailbox setup feel longer] -> Keep contacts in a concise capability review, preselect sensible provider-derived defaults, hide advanced fields, and never block successful mail setup on optional contact sync.
- [Users may not discover later setup] -> Expose the same enable/fix action in both the mailbox account row and the Contacts workspace empty/attention states.

## Migration Plan

1. Add nullable ownership, management mode, capability state, cursor/version, book/group, and generation fields without removing existing credential columns.
2. Backfill mailbox links only when an existing contact source can be matched unambiguously by provider and account identity; otherwise retain it as independent.
3. Reconcile one managed source per eligible mailbox, initially disabled when OAuth consent is missing.
4. Deploy provider adapters and sync orchestration, then start managed sources in bounded batches; preserve existing contacts until the first complete successful run.
5. Switch settings and contacts UI to mailbox capability/status and remove manual Google/Microsoft token entry.
6. After successful rollout, clear duplicated secrets from mailbox-managed source rows; keep encrypted credentials only for independent sources.

Rollback disables mailbox-managed tasks and UI while retaining the added nullable schema and existing local contact cache. Independent CardDAV sources continue to function. No rollback step deletes remote contacts.

## Open Questions

- Whether writable Google contact-group management belongs in the first implementation slice or should initially be read-only while membership is synchronized.
- Whether a generic mailbox with multiple discovered CardDAV address books should create one source with multiple books or one source per address book; the preferred model is one mailbox source with multiple books unless provider behavior prevents it.
