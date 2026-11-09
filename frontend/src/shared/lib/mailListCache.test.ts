import { QueryClient } from '@tanstack/react-query'
import { describe, expect, it } from 'vitest'
import type { Message } from '@/shared/types'
import type { UnifiedCounts } from '@/shared/lib/unifiedViews'
import {
  optimisticMarkRead,
  optimisticMarkThreadRead,
  optimisticRemove,
  optimisticToggleFlag,
  type InfiniteList,
} from './mailListCache'

function message(overrides: Partial<Message>): Message {
  return {
    id: 'm1',
    account_id: 'a1',
    folder_id: 'f1',
    uid: 1,
    message_id_header: null,
    thread_id: 't1',
    in_reply_to: null,
    references: null,
    list_id: null,
    subject: 'Hello',
    from_addr: 'a@example.test',
    to_addrs: '',
    cc_addrs: '',
    snippet: '',
    date: null,
    internal_date: '2026-01-01T00:00:00Z',
    is_read: false,
    is_flagged: false,
    is_deleted: false,
    thread_size: 1,
    thread_unread: 1,
    ...overrides,
  }
}

const counts: UnifiedCounts = { inbox: 5, starred: 2, sent: 0, drafts: 0, archive: 0, spam: 0, trash: 0 }

function list(messages: Message[]): InfiniteList {
  return { pages: [{ messages, total: messages.length, next_cursor: null }], pageParams: [undefined] }
}

function seed(rows: Message[]) {
  const qc = new QueryClient()
  qc.setQueryData(['unified-counts'], counts)
  // The same view cached under three keys: plain, unread filter, account scope.
  qc.setQueryData(['unified', 'inbox', null, false], list(rows))
  qc.setQueryData(['unified', 'inbox', null, true], list(rows.filter((m) => !m.is_read)))
  qc.setQueryData(['unified', 'inbox', 'a1', false], list(rows))
  qc.setQueryData(['unified', 'starred', null, false], list(rows.filter((m) => m.is_flagged)))
  qc.setQueryData(['folder-messages', 'a1', 'INBOX', false], list(rows))
  return qc
}

const rows = (qc: QueryClient, key: unknown[]) =>
  qc.getQueryData<InfiniteList>(key)?.pages.flatMap((p) => p.messages) ?? []
const badge = (qc: QueryClient) => qc.getQueryData<UnifiedCounts>(['unified-counts'])!

describe('optimisticMarkRead', () => {
  it('adjusts the inbox badge once although the view is cached under several keys', async () => {
    const qc = seed([message({ id: 'm1' }), message({ id: 'm2', thread_id: 't2' })])
    const restore = await optimisticMarkRead(qc, 'm1', true)

    expect(badge(qc).inbox).toBe(4)
    expect(rows(qc, ['unified', 'inbox', null, false])[0]).toMatchObject({ is_read: true, thread_unread: 0 })
    expect(rows(qc, ['folder-messages', 'a1', 'INBOX', false])[0]).toMatchObject({ is_read: true })

    restore()
    expect(badge(qc).inbox).toBe(5)
    expect(rows(qc, ['unified', 'inbox', null, false])[0]).toMatchObject({ is_read: false })
  })
})

describe('optimisticMarkThreadRead', () => {
  it('clears the whole thread and drops its unread count from the badge', async () => {
    const qc = seed([message({ id: 'm1', thread_size: 3, thread_unread: 2 })])
    await optimisticMarkThreadRead(qc, 't1', true)
    expect(badge(qc).inbox).toBe(3)
    expect(rows(qc, ['unified', 'inbox', null, false])[0]).toMatchObject({ is_read: true, thread_unread: 0 })
  })

  it('marking unread counts every message of the thread', async () => {
    const qc = seed([message({ id: 'm1', is_read: true, thread_size: 3, thread_unread: 0 })])
    await optimisticMarkThreadRead(qc, 't1', false)
    expect(badge(qc).inbox).toBe(8)
    expect(rows(qc, ['unified', 'inbox', null, false])[0]).toMatchObject({ is_read: false, thread_unread: 3 })
  })
})

describe('optimisticToggleFlag', () => {
  it('flags the row everywhere and raises the starred badge once', async () => {
    const qc = seed([message({ id: 'm1' })])
    await optimisticToggleFlag(qc, 'm1', true)
    expect(badge(qc).starred).toBe(3)
    expect(rows(qc, ['unified', 'inbox', null, false])[0].is_flagged).toBe(true)
    expect(rows(qc, ['folder-messages', 'a1', 'INBOX', false])[0].is_flagged).toBe(true)
  })

  it('unflagging removes the row from the starred view', async () => {
    const qc = seed([message({ id: 'm1', is_flagged: true })])
    const restore = await optimisticToggleFlag(qc, 'm1', false)
    expect(badge(qc).starred).toBe(1)
    expect(rows(qc, ['unified', 'starred', null, false])).toHaveLength(0)
    expect(rows(qc, ['unified', 'inbox', null, false])[0].is_flagged).toBe(false)
    restore()
    expect(rows(qc, ['unified', 'starred', null, false])).toHaveLength(1)
  })
})

describe('optimisticRemove', () => {
  it('drops the rows, lowers the unread and starred badges once, and keeps totals in sync', async () => {
    const qc = seed([message({ id: 'm1', is_flagged: true, thread_size: 2, thread_unread: 2 }), message({ id: 'm2', thread_id: 't2' })])
    await optimisticRemove(qc, (m) => m.id === 'm1')
    expect(badge(qc)).toMatchObject({ inbox: 3, starred: 1 })
    const page = qc.getQueryData<InfiniteList>(['unified', 'inbox', null, false])!.pages[0]
    expect(page.messages.map((m) => m.id)).toEqual(['m2'])
    expect(page.total).toBe(0)
  })
})
