## 1. Contacts — Backend & Schema

- [ ] 1.1 Add `contact-sync` crate to Rust workspace; add vCard parsing dep (`vcard4` or `vcard` crate)
- [ ] 1.2 Replace `contact_accounts` stub (from `foundation`) with full schema: (id, display_name, type [cardav|graph|google], base_url, auth_scheme, credentials_encrypted, sync_token, last_synced_at) — no user_id (Postgres mode adds user_id column)
- [ ] 1.3 Replace `contacts` stub with full schema: (id, account_id, uid, display_name, given_name, family_name, org, title, emails JSON, phones JSON, addresses JSON, notes, photo_blob_key TEXT, raw_vcard, synced_at) — no user_id (Postgres mode adds user_id column); photo_blob_key is BlobStore key (`contact/{account_id}/{uid}/photo`), null until photo fetched
- [ ] 1.4 Replace `contact_groups` stub with full schema: (id, account_id, name)
- [ ] 1.5 Replace `contact_group_members` stub with full schema: (contact_id, group_id)
- [ ] 1.6 Create FTS or trigram index on `display_name`, `given_name`, `family_name`, and flattened email array for autocomplete search
- [ ] 1.7 Implement `contact-sync` CardDAV: PROPFIND address-book discovery, REPORT addressbook-query, PUT/DELETE write-through, sync-token incremental
- [ ] 1.8 Implement Graph contacts sync: `GET /me/contacts` + `GET /me/contactFolders`; delta query for incremental; POST/PATCH/DELETE write-through
- [ ] 1.9 Implement Google People API sync: `people.connections.list` with `syncToken`; write via `people.updateContact` / `people.createContact` / `people.deleteContact`
- [ ] 1.10 Implement vCard 3.0 + 4.0 parsing: map EMAIL/TEL/ADR/ORG/PHOTO fields to contact struct; store raw vCard
- [ ] 1.11 Implement contact sync task manager (same pattern as calendar — spawn/cancel per account)
- [ ] 1.12 Implement `GET /contacts/search?q=<term>` — fuzzy search on name fields + email array; max 10 results; used by compose and calendar attendee autocomplete
- [ ] 1.13 Implement `POST /contacts`, `PUT /contacts/:id`, `DELETE /contacts/:id` with write-through to remote account
- [ ] 1.14 Implement `GET /contacts/:id/photo` — lazy fetch from remote; write bytes to `BlobStore` at key `contact/{account_id}/{uid}/photo`; update `contacts.photo_blob_key`; on subsequent requests serve from `BlobStore::get`; return binary with appropriate `Content-Type`
- [ ] 1.15 Implement account CRUD: `POST /contact-accounts`, `GET /contact-accounts`, `DELETE /contact-accounts/:id`, `GET /contact-accounts/:id/sync-status`

## 2. Contacts — Frontend

- [ ] 2.1 Add contact accounts page: connect CardDAV / Exchange (OAuth) / Google (OAuth); show sync status per account
- [ ] 2.2 Add contacts nav item in sidebar; implement route `/contacts`
- [ ] 2.3 Build contact list view: alphabetical grouping, search bar, account filter, contact count per account
- [ ] 2.4 Build contact detail panel: avatar/photo, all field groups (emails, phones, addresses, notes), "Compose" action per email, edit/delete buttons
- [ ] 2.5 Build create/edit contact form: name fields, org/title, multi-value email/phone/address inputs with type labels, notes, photo upload, account selector
- [ ] 2.6 Implement autocomplete component: dropdown triggered on input in compose To/Cc/Bcc fields — calls `GET /contacts/search?q=` debounced; selecting fills field with `Display Name <email>`
- [ ] 2.7 Wire same autocomplete component into calendar event attendee input field (if `calendar` change is deployed)
- [ ] 2.8 Show contact card popover when user clicks sender name in message list or message detail: fetch contact by email via `GET /contacts/search?q=<email>` (exact match), show photo/name/org/phone/"Compose" action
