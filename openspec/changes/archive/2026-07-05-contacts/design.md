## Context

Builds on `email-core`. Compose fields exist; this change adds autocomplete backed by a synced contact store. The `calendar` change's attendee input uses the same autocomplete component. Credential encryption reuses the same AES-256-GCM scheme and `CREDENTIAL_ENCRYPTION_KEY` as email and calendar accounts.

## Decisions

### D1: Contact protocol support

| Provider | Protocol | Notes |
|---|---|---|
| CardDAV servers (Nextcloud, Apple, Radicale) | CardDAV (RFC 6352) | Same WebDAV infra as CalDAV. PROPFIND discovery, REPORT addressbook-query, PUT/DELETE. `sync-token` incremental. |
| Exchange / O365 / Outlook.com | Microsoft Graph `/me/contacts` | OAuth2 already available. Supports contact folders. |
| Google Contacts | Google People API `people.connections.list` | OAuth2 already available. `syncToken` incremental. |

### D2: vCard parsing

Support vCard 3.0 (RFC 2426) and 4.0 (RFC 6350). Parse: EMAIL, TEL, ADR, ORG, PHOTO, NOTE, FN. Raw vCard stored in `contacts.raw_vcard` for faithful round-trips on CardDAV PUT (preserves unknown properties).

### D3: Contact autocomplete

`GET /contacts/search?q=<term>` — fuzzy search on `display_name`, `given_name`, `family_name`, and all email addresses in the `emails JSON` array. Max 10 results. Used by compose To/Cc/Bcc and calendar attendee input.

DB index: FTS or trigram on name fields + flattened email array for fast prefix search.

Autocomplete component: debounced input, dropdown of suggestions, selecting fills field with `Display Name <email>`.

### D4: Contact photos — lazy BlobStore

Photos not fetched during bulk sync (bandwidth/time cost). Fetched lazily on first `GET /contacts/:id/photo` request: fetched from remote, written to `BlobStore` at key `contact/{account_id}/{uid}/photo`, `contacts.photo_blob_key` updated. Subsequent requests served from `BlobStore::get`. No Gravatar — no email hash sent to external services.

### D5: DB schema — no user_id columns (SQLite per-user mode)

`contact_accounts`, `contacts`, `contact_groups`, `contact_group_members` all in per-user `mail.db`. No `user_id` columns (structural isolation). Postgres mode adds `user_id` columns.

### D6: Sender card popover

Clicking sender name in message list or message detail triggers a contact lookup (`GET /contacts/search?q=<email>`, exact match). If found, shows a contact card popover with photo, name, org, phone, and "Compose" action. No external lookup — local contacts only.
