#!/usr/bin/env node
import {
  assert,
  authHeaders,
  baseUrl,
  poll,
  registerOrLogin,
  request,
  requiredEnv,
} from './lib/mailquill-e2e.mjs'

const password = requiredEnv('E2E_USER_PASSWORD')
const userEmail = requiredEnv('E2E_USER_EMAIL')
const token = await registerOrLogin(userEmail, password)

const oauthStart = await fetch(`${baseUrl()}/api/auth/oauth/google/start`, {
  redirect: 'manual',
  headers: authHeaders(token),
})
assert(oauthStart.status >= 300 && oauthStart.status < 400, `OAuth start returned ${oauthStart.status}`)
const location = oauthStart.headers.get('location')
assert(location?.startsWith('https://accounts.google.com/'), 'OAuth start did not redirect to Google')

const accountId = requiredEnv('E2E_GMAIL_ACCOUNT_ID')
await request(`/api/accounts/${accountId}/sync`, {
  method: 'POST',
  headers: authHeaders(token),
})

await poll('Gmail XOAUTH2 sync to finish', async () => {
  const { body } = await request(`/api/accounts/${accountId}/sync-status`, {
    headers: authHeaders(token),
  })
  assert(body.state !== 'error', `sync entered error state: ${body.error || 'unknown error'}`)
  return body.last_synced_at ? body : null
})

const { body: inbox } = await request('/api/mailbox/unified?limit=10', {
  headers: authHeaders(token),
})
assert(Array.isArray(inbox.items), 'unified mailbox response did not include items')
assert(inbox.items.length > 0, 'Gmail sync produced no readable messages')

const row = inbox.items[0]
const messageId = row.message_id || row.id
const { body: message } = await request(`/api/messages/${messageId}`, {
  headers: authHeaders(token),
})
assert(message.body_available, 'message body was not available after on-demand read')

const replyTo = process.env.E2E_REPLY_TO || message.from_addr
const { body: sendResult } = await request('/api/send', {
  method: 'POST',
  headers: authHeaders(token),
  body: JSON.stringify({
    account_id: accountId,
    from: requiredEnv('E2E_GMAIL_FROM'),
    to: [replyTo],
    subject: `Re: ${message.subject || 'Mailquill E2E'}`,
    body_text: 'Automated Mailquill Gmail XOAUTH2 E2E reply.',
    in_reply_to: message.message_id_header,
    references: message.references || message.message_id_header,
  }),
})
assert(sendResult.message_id, 'send response did not include message_id')

console.log(JSON.stringify({
  ok: true,
  flow: 'gmail-xoauth2-sync-read-reply',
  account_id: accountId,
  read_message_id: messageId,
  sent_message_id: sendResult.message_id,
}))
