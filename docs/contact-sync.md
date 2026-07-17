# Contact synchronization operations

Mailquill treats contacts as an optional capability of a mailbox. Google
mailboxes use Google People, Microsoft mailboxes use Microsoft Graph, and other
mailboxes use CardDAV when it is configured or discovered. Mailbox-managed
sources reuse encrypted mailbox credentials, OAuth refresh, and DAV certificate
trust; they never store a second provider token or password. Independent
CardDAV sources remain supported under the advanced contact-source flow.

## Provider setup and re-consent

- Enable the Google People API in the same project as the OAuth client and add
  `https://www.googleapis.com/auth/contacts` to the consent screen.
- Add delegated Microsoft Graph `Contacts.ReadWrite` permission. Tenant policy
  may require administrator consent.
- Existing OAuth mailboxes continue to synchronize mail without these grants.
  Users enable Contacts from the mailbox card, complete the scoped reconnect,
  and return to the same row. Roll out re-consent gradually; do not revoke or
  replace working mail refresh tokens as a migration shortcut.
- Generic mailboxes first try CardDAV discovery with their stored mailbox
  credentials. A manually entered URL is available only in Advanced setup.
  The address books selected in the assistant are stored as source metadata.

## CardDAV and TLS

CardDAV discovery and sync use the mailbox's DAV authentication and the same
explicit certificate exception as calendar DAV when the trusted certificate
matches the target server. An invalid hostname, expired certificate, changed
fingerprint, or unrelated host is not silently accepted. Correct the URL or use
the existing certificate confirmation flow, then retry discovery. Form values
and downloaded contacts remain intact after a failed attempt.

## States and safe diagnostics

The API and contact status stream expose these credential-free states:

| State | Meaning | Operator/user action |
| --- | --- | --- |
| `disabled` | Sync was turned off | Enable it; retained cache stays read-only |
| `pending` | Initial or resumed sync is queued | Wait; starting it again is unnecessary |
| `syncing` | A provider page is being processed | Monitor completion or failure |
| `idle` | Last run completed | No action, or run a manual sync |
| `consent_required` | Contact OAuth grant is missing | Complete scoped re-consent |
| `reauth_required` | Provider authorization was revoked | Reconnect the mailbox |
| `error` | A recoverable provider/transport failure occurred | Use the displayed retry or setup action |
| `unavailable` | No supported contact service was found | Configure/discover CardDAV or leave disabled |

Provider failures are categorized as consent, reauthentication,
authentication, conflict, cursor expiry, rate limit, transport, invalid
response, or unavailable. Logs and metrics may contain provider, source ID,
operation, category, page counts, and timing. They must never contain access or
refresh tokens, passwords, vCards, contact fields, photo bytes, or provider
response payloads. Respect `Retry-After`; cursor expiry intentionally starts a
safe full generation without deleting the prior cache until completion.

## Staged verification

Before widening rollout, exercise a test mailbox for each applicable class:

1. Google consumer and Workspace accounts with a multi-page address book.
2. Microsoft personal plus work/school accounts.
3. CardDAV with multiple books and an explicit TLS trust recovery case.
4. Missing/revoked consent, expired cursor, rate limiting, remote version
   conflict, deletion tombstones, provider photos, and a large address book.
5. Disable while retaining the cache, re-enable, then disable with confirmed
   local-cache removal. Confirm that remote contacts are unchanged.

During each run verify contact and book counts, final cursor advancement,
restart resume, compose/calendar autocomplete ranking, accessible status
announcements, and absence of contact content in logs. Record provider class,
result, duration, and counts only.

## Rollback

Disable mailbox-managed contact tasks and hide the capability UI while leaving
migration 0034 and local caches in place. Do not roll the SQLite schema back and
do not issue provider deletes. Independent CardDAV sources continue to work.
OAuth mail remains valid when contact scopes are unused. After the cause is
fixed, re-enable sources in bounded batches; valid cursors resume incrementally
and expired cursors fall back to a generation-safe full sync.
