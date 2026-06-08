## Why

A mail client without contact management forces users to remember email addresses or copy-paste them from previous messages. Contact sync from existing CardDAV / Microsoft Graph / Google People sources makes compose autocomplete work with the addresses users already have. It also enables richer sender display (photo, org, title) and ties into the calendar attendee lookup.

## What Changes

- Contact account management: connect CardDAV, Microsoft Graph contacts, and Google People API per user
- Background contact sync with incremental update support (sync-token per provider)
- Full contact CRUD with write-through to the remote account
- vCard 3.0 + 4.0 parsing, raw vCard storage for faithful CardDAV round-trips
- Contact photo lazy-fetch and storage via BlobStore
- Autocomplete API and UI component: compose To/Cc/Bcc and calendar attendee fields
- Contact list, detail panel, and create/edit form in frontend

## Capabilities

### New Capabilities

- `contact-accounts`: Connect CardDAV, Microsoft Graph contacts, and Google Contacts per user
- `contacts`: Contact CRUD, vCard sync, autocomplete in compose and calendar attendee fields

### Modified Capabilities

## Impact

- Depends on `email-core` (compose To/Cc/Bcc fields, user settings, OAuth2 infra)
- `calendar` attendee input wires the same autocomplete component (soft dependency — works without contacts change, just less useful)
- Rust additions: `contact-sync` crate; vCard parsing crate (`vcard4` or `vcard`); `reqwest` (reuse)
- New DB tables: `contact_accounts`, `contacts`, `contact_groups`, `contact_group_members` (full schema; stubs created in `foundation`)
- New API routes: `/contact-accounts/*`, `/contacts/*`
- Frontend addition: contacts sidebar nav, `/contacts` route, contact list/detail/edit UI, autocomplete component wired into compose and calendar
- Contact photos stored via `BlobStore` at key `contact/{account_id}/{uid}/photo`
- No cross-provider contact deduplication in v1
