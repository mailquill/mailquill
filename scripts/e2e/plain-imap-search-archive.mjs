#!/usr/bin/env node
import {
  assert,
  authHeaders,
  optionalEnv,
  poll,
  registerOrLogin,
  request,
  requiredEnv,
  uniqueEmail,
} from './lib/mailquill-e2e.mjs'

const password = optionalEnv('E2E_USER_PASSWORD', 'Password123!')
const userEmail = optionalEnv('E2E_USER_EMAIL', uniqueEmail('plain-imap-e2e'))
const token = await registerOrLogin(userEmail, password)

const accountPayload = {
  display_name: optionalEnv('E2E_IMAP_DISPLAY_NAME', 'Plain IMAP E2E'),
  primary_email: requiredEnv('E2E_IMAP_EMAIL'),
  imap_host: requiredEnv('E2E_IMAP_HOST'),
  imap_port: Number(requiredEnv('E2E_IMAP_PORT')),
  imap_username: requiredEnv('E2E_IMAP_USERNAME'),
  imap_password: requiredEnv('E2E_IMAP_PASSWORD'),
  imap_auth_scheme: optionalEnv('E2E_IMAP_AUTH_SCHEME', 'plain'),
  smtp_host: requiredEnv('E2E_SMTP_HOST'),
  smtp_port: Number(requiredEnv('E2E_SMTP_PORT')),
  smtp_username: requiredEnv('E2E_SMTP_USERNAME'),
  smtp_password: requiredEnv('E2E_SMTP_PASSWORD'),
  smtp_auth_scheme: optionalEnv('E2E_SMTP_AUTH_SCHEME', 'plain'),
  body_sync_mode: optionalEnv('E2E_BODY_SYNC_MODE', 'full'),
  sync_interval_secs: 300,
}

const { body: account } = await request('/api/accounts', {
  method: 'POST',
  headers: authHeaders(token),
  body: JSON.stringify(accountPayload),
})
assert(account.id, 'account creation did not return an id')

await request(`/api/accounts/${account.id}/sync`, {
  method: 'POST',
  headers: authHeaders(token),
})

await poll('plain IMAP sync to finish', async () => {
  const { body } = await request(`/api/accounts/${account.id}/sync-status`, {
    headers: authHeaders(token),
  })
  assert(body.state !== 'error', `sync entered error state: ${body.error || 'unknown error'}`)
  return body.last_synced_at ? body : null
})

const searchQuery = encodeURIComponent(optionalEnv('E2E_SEARCH_QUERY', ''))
const { body: searchResults } = await request(`/api/search?q=${searchQuery}&account_id=${account.id}`, {
  headers: authHeaders(token),
})
assert(Array.isArray(searchResults.items), 'search response did not include an items array')
assert(searchResults.items.length > 0, 'search returned no messages to archive')

const messageId = searchResults.items[0].id
await request(`/api/messages/${messageId}/archive`, {
  method: 'POST',
  headers: authHeaders(token),
})

console.log(JSON.stringify({
  ok: true,
  flow: 'plain-imap-sync-search-archive',
  account_id: account.id,
  archived_message_id: messageId,
}))
