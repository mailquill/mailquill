## ADDED Requirements

### Requirement: Mailbox determines its managed contact source
The system SHALL derive at most one mailbox-managed contact source for each email account from authoritative provider and account configuration. A Google OAuth mailbox SHALL use Google People API, a Microsoft OAuth mailbox SHALL use Microsoft Graph Contacts, and any other mailbox SHALL use an explicitly configured or successfully discovered CardDAV service. Hostname matching alone MUST NOT select an OAuth API provider.

#### Scenario: Google mailbox selects People API
- **WHEN** an email account's encrypted OAuth metadata identifies Google
- **THEN** the system creates or updates one mailbox-managed Google People contact source linked to that email account

#### Scenario: Microsoft mailbox selects Graph
- **WHEN** an email account's encrypted OAuth metadata identifies Microsoft
- **THEN** the system creates or updates one mailbox-managed Microsoft Graph contact source linked to that email account

#### Scenario: Generic mailbox selects CardDAV
- **WHEN** a non-Google/non-Microsoft mailbox has a configured CardDAV URL or CardDAV discovery succeeds
- **THEN** the system creates or updates one mailbox-managed CardDAV source linked to that email account

#### Scenario: No contact provider is available
- **WHEN** a mailbox has neither a supported OAuth contact provider nor a usable CardDAV service
- **THEN** the system reports contacts as unavailable for that mailbox and does not start a failing sync loop

### Requirement: Managed sources share mailbox security context
A mailbox-managed contact source SHALL use the mailbox account's encrypted credentials, centralized OAuth refresh behavior, reauthentication state, and applicable DAV TLS trust policy. The system MUST NOT require or persist a separately entered Google or Microsoft access token for a mailbox-managed source.

#### Scenario: OAuth token expires
- **WHEN** a managed Google or Microsoft contact sync needs an expired access token
- **THEN** the system refreshes it through the shared mailbox OAuth token service before calling the contact provider

#### Scenario: OAuth grant is revoked
- **WHEN** refresh fails with a permanent authorization error or the provider rejects the token as unauthorized
- **THEN** the managed source enters `reauth_required`, contact sync pauses, and the user receives a reconnect action while mail data remains intact

#### Scenario: CardDAV uses a trusted certificate exception
- **WHEN** a mailbox-managed CardDAV request targets the same server covered by the mailbox's stored DAV TLS trust policy
- **THEN** contact discovery, reads, and writes use that policy without storing a second credential or trust exception

### Requirement: Contact consent is capability-specific
The system SHALL request Google contact read/write scope and Microsoft delegated `Contacts.ReadWrite` for two-way managed contact integration, SHALL record the granted OAuth scopes, and SHALL distinguish missing consent from revoked authentication.

#### Scenario: Existing mailbox lacks contact grant
- **WHEN** an existing Google or Microsoft mailbox has valid mail credentials but lacks the required contact scope
- **THEN** mail remains operational, the contact capability enters `consent_required`, and the UI offers an explicit re-consent flow

#### Scenario: Re-consent succeeds
- **WHEN** the user completes provider consent with the required contact scope
- **THEN** the mailbox credentials are updated, the managed source becomes eligible, and initial contact synchronization starts

#### Scenario: User declines contact consent
- **WHEN** the user does not grant the requested contact scope
- **THEN** the mailbox remains connected for mail and the contact capability remains disabled without repeated provider failures

### Requirement: Managed contact lifecycle follows mailbox lifecycle
The system SHALL reconcile and start an eligible managed contact source after mailbox creation, OAuth callback, reconnect, relevant account update, and application startup. Deleting a mailbox SHALL stop its managed contact task and remove only that source's local contact cache.

#### Scenario: Application restarts
- **WHEN** the service starts with eligible mailbox-managed contact sources in storage
- **THEN** it resumes their sync tasks without requiring the user to reopen the contacts page

#### Scenario: Mailbox configuration changes
- **WHEN** provider identity, CardDAV URL, credentials, or TLS policy changes
- **THEN** the system re-evaluates the existing managed source and restarts it with the new effective configuration without creating a duplicate source

#### Scenario: Mailbox is deleted
- **WHEN** a user deletes an email account
- **THEN** the managed contact task stops and its local source, contacts, books, and groups are cascade-deleted without issuing remote contact deletions

### Requirement: Existing contact sources migrate without duplication
The system SHALL link an existing compatible contact source to a mailbox only when provider and account identity match unambiguously. Ambiguous or unrelated CardDAV sources SHALL remain independent, and migration MUST NOT create duplicate local contacts for the same source and remote identifier.

#### Scenario: Existing Google source matches one mailbox
- **WHEN** one legacy Google contact source and one Google mailbox have the same authenticated account identity
- **THEN** migration links the source to that mailbox, retains its local contacts and sync cursor, and removes duplicated source credentials after shared credentials are active

#### Scenario: Existing source is ambiguous
- **WHEN** a legacy contact source cannot be matched to exactly one mailbox
- **THEN** migration leaves it independent and records no mailbox ownership

#### Scenario: Reconciliation runs repeatedly
- **WHEN** mailbox-contact reconciliation runs more than once
- **THEN** the unique mailbox relationship and remote identifiers keep the result idempotent

### Requirement: Mailbox contact capability is visible
The account and contacts APIs SHALL expose the effective contact provider, source identity, capability state, last successful synchronization, actionable error category, and available actions without exposing credentials.

#### Scenario: Account settings loads
- **WHEN** the frontend retrieves a mailbox with a managed contact source
- **THEN** it can render provider, sync time, status, and manual sync or reconnect actions from the API response

#### Scenario: Source requires consent
- **WHEN** the source is `consent_required`
- **THEN** the contacts workspace and account settings show an explanatory consent action instead of a generic sync error

### Requirement: First-time mailbox setup offers contacts clearly
The mailbox setup wizard SHALL present contact synchronization as an optional detected capability before creating the mailbox. It SHALL explain the provider and requested access in plain language, SHALL use a single provider sign-in for the selected mailbox capabilities, and SHALL allow successful mail setup to continue when contacts are skipped or need later attention.

#### Scenario: New Google or Microsoft mailbox enables contacts
- **WHEN** provider discovery identifies Google or Microsoft and the user leaves “Sync contacts” enabled
- **THEN** the capability review explains contact access and the OAuth flow requests the required mail and contact grants in one sign-in

#### Scenario: User skips contacts during first-time setup
- **WHEN** the user disables “Sync contacts” before creating the mailbox
- **THEN** the mailbox is created for mail, its contact capability is `disabled`, and the completion view explains where contacts can be enabled later

#### Scenario: New password mailbox has discoverable CardDAV
- **WHEN** the wizard has valid mailbox credentials and CardDAV discovery succeeds
- **THEN** the capability review shows contacts as available without requiring the user to enter a URL or duplicate credentials

#### Scenario: CardDAV verification fails but mail succeeds
- **WHEN** mail connection succeeds but optional CardDAV discovery or verification fails
- **THEN** mailbox creation remains successful and the completion view offers “Set up contacts later” plus an expandable diagnostic

#### Scenario: Initial contact synchronization is large
- **WHEN** contact setup succeeds but the initial address-book sync is still running
- **THEN** the wizard completes, displays non-blocking progress, and offers navigation to mail or the Contacts workspace

### Requirement: Existing mailboxes can enable contacts later
Account settings and the Contacts workspace SHALL provide the same mailbox-scoped enable, repair, and status actions for mailboxes created before contact integration or initially configured without contacts. Enabling contacts later MUST NOT require mailbox deletion, recreation, or manual provider-token entry.

#### Scenario: Existing OAuth mailbox enables contacts
- **WHEN** the user selects “Enable contacts” on a Google or Microsoft mailbox that lacks the contact grant
- **THEN** the system starts scoped re-consent for that mailbox, preserves the user's return location, and resumes setup at the mailbox's Contacts row after redirect

#### Scenario: Existing generic mailbox enables CardDAV
- **WHEN** the user selects “Enable contacts” on a generic mailbox
- **THEN** a short assistant tries discovery with stored credentials, lets the user choose discovered address books, and hides manual URL entry under an advanced option

#### Scenario: User starts from an empty Contacts workspace
- **WHEN** no enabled contact source exists but one or more mailboxes are connected
- **THEN** the empty state lists eligible mailboxes with provider-specific “Enable contacts” or “Fix setup” actions

#### Scenario: Existing setup needs reauthentication
- **WHEN** an existing mailbox-managed source is `reauth_required`
- **THEN** both account settings and the Contacts workspace offer one reconnect action and preserve cached contacts while authorization is repaired

### Requirement: Contact setup feedback is actionable and accessible
Contact setup SHALL show provider, current state, last successful sync, progress, and one recommended action without relying on color alone. Default views SHALL use user-facing language and hide protocol diagnostics behind an expandable advanced section. All setup and recovery controls SHALL be keyboard operable, localized in English and German, and expose programmatically associated errors and status updates.

#### Scenario: Setup is synchronizing
- **WHEN** an initial or manual contact sync is running
- **THEN** the initiating view shows labeled progress, prevents duplicate starts, and announces completion or failure through an accessible live region

#### Scenario: Setup fails with a known recovery
- **WHEN** consent, authentication, CardDAV discovery, TLS trust, or provider availability requires user action
- **THEN** the UI shows a plain-language summary, one primary recovery action, and expandable copyable technical detail

#### Scenario: OAuth flow returns to Mailquill
- **WHEN** a later-setup OAuth consent flow completes or is cancelled
- **THEN** the user returns to the same mailbox Contacts section with preserved context, an explicit outcome, and focus restored to the relevant action or status

### Requirement: Disabling contact sync is non-destructive by default
The system SHALL allow users to disable a mailbox's contact synchronization without deleting remote contacts. The UI SHALL explain the effect and SHALL offer a default option to retain downloaded contacts as a read-only local cache or an explicit confirmed option to remove that mailbox's local contact cache.

#### Scenario: Disable and keep downloaded contacts
- **WHEN** the user disables contact sync and accepts the default retention option
- **THEN** synchronization and remote writes stop, cached contacts remain locally searchable and visibly read-only, and no provider deletion is issued

#### Scenario: Disable and remove local cache
- **WHEN** the user explicitly chooses to remove downloaded contacts and confirms the destructive local action
- **THEN** that mailbox's local contacts, books, groups, and cached photos are removed while remote provider data remains unchanged

#### Scenario: Re-enable retained source
- **WHEN** the user re-enables a disabled source whose local cache was retained
- **THEN** the system resumes from a valid cursor when possible and keeps cached contacts visible during reconciliation
