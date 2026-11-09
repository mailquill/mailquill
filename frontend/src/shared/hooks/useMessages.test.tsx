import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook } from '@testing-library/react'
import { createElement, type PropsWithChildren } from 'react'
import { toast } from 'sonner'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiDelete, apiPatch } from '@/shared/api'
import type { InfiniteList } from '@/shared/lib/mailListCache'
import type { UnifiedCounts } from '@/shared/lib/unifiedViews'
import type { Message } from '@/shared/types'
import { useDeleteMessage, useMarkRead, useToggleFlag } from './useMessages'

vi.mock('@/shared/api', () => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
  apiPatch: vi.fn(),
  apiDelete: vi.fn(),
}))

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }))

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

function seed(rows: Message[]) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  qc.setQueryData(['unified-counts'], counts)
  qc.setQueryData<InfiniteList>(['unified', 'inbox', null, false], {
    pages: [{ messages: rows, total: rows.length, next_cursor: null }],
    pageParams: [undefined],
  })
  return qc
}

function wrapperFor(qc: QueryClient) {
  return ({ children }: PropsWithChildren) => createElement(QueryClientProvider, { client: qc }, children)
}

const rows = (qc: QueryClient) =>
  qc.getQueryData<InfiniteList>(['unified', 'inbox', null, false])?.pages.flatMap((p) => p.messages) ?? []
const badge = (qc: QueryClient) => qc.getQueryData<UnifiedCounts>(['unified-counts'])!

beforeEach(() => {
  vi.mocked(apiPatch).mockReset()
  vi.mocked(apiDelete).mockReset()
  vi.mocked(toast.error).mockReset()
})

describe('useMarkRead', () => {
  it('flips the row and badge before the server answers and keeps them on success', async () => {
    vi.mocked(apiPatch).mockResolvedValue({ ok: true })
    const qc = seed([message({ id: 'm1' }), message({ id: 'm2', thread_id: 't2' })])
    const { result } = renderHook(() => useMarkRead(), { wrapper: wrapperFor(qc) })

    await act(async () => {
      await result.current.mutateAsync({ id: 'm1', is_read: true })
    })

    expect(apiPatch).toHaveBeenCalledWith('/messages/m1/read', { is_read: true })
    expect(rows(qc)[0]).toMatchObject({ id: 'm1', is_read: true, thread_unread: 0 })
    expect(rows(qc)[1]).toMatchObject({ id: 'm2', is_read: false })
    expect(badge(qc).inbox).toBe(4)
    expect(toast.error).not.toHaveBeenCalled()
  })
})

describe('useToggleFlag', () => {
  it('rolls the optimistic flag back and reports the failure', async () => {
    vi.mocked(apiPatch).mockRejectedValue(new Error('boom'))
    const qc = seed([message({ id: 'm1' })])
    const { result } = renderHook(() => useToggleFlag(), { wrapper: wrapperFor(qc) })

    await act(async () => {
      await expect(result.current.mutateAsync({ id: 'm1', is_flagged: true })).rejects.toThrow('boom')
    })

    expect(rows(qc)[0].is_flagged).toBe(false)
    expect(badge(qc).starred).toBe(2)
    expect(toast.error).toHaveBeenCalledTimes(1)
  })

  it('flags the row immediately when the server accepts', async () => {
    vi.mocked(apiPatch).mockResolvedValue({ ok: true })
    const qc = seed([message({ id: 'm1' })])
    const { result } = renderHook(() => useToggleFlag(), { wrapper: wrapperFor(qc) })

    await act(async () => {
      await result.current.mutateAsync({ id: 'm1', is_flagged: true })
    })

    expect(apiPatch).toHaveBeenCalledWith('/messages/m1/flag', { is_flagged: true })
    expect(rows(qc)[0].is_flagged).toBe(true)
    expect(badge(qc).starred).toBe(3)
  })
})

describe('useDeleteMessage', () => {
  it('removes the row and lowers the unread badge', async () => {
    vi.mocked(apiDelete).mockResolvedValue(undefined)
    const qc = seed([message({ id: 'm1' }), message({ id: 'm2', thread_id: 't2', is_read: true, thread_unread: 0 })])
    const { result } = renderHook(() => useDeleteMessage(), { wrapper: wrapperFor(qc) })

    await act(async () => {
      await result.current.mutateAsync('m1')
    })

    expect(apiDelete).toHaveBeenCalledWith('/messages/m1')
    expect(rows(qc).map((m) => m.id)).toEqual(['m2'])
    expect(badge(qc).inbox).toBe(4)
  })

  it('restores the row when the delete fails', async () => {
    vi.mocked(apiDelete).mockRejectedValue(new Error('offline'))
    const qc = seed([message({ id: 'm1' })])
    const { result } = renderHook(() => useDeleteMessage(), { wrapper: wrapperFor(qc) })

    await act(async () => {
      await expect(result.current.mutateAsync('m1')).rejects.toThrow('offline')
    })

    expect(rows(qc).map((m) => m.id)).toEqual(['m1'])
    expect(badge(qc).inbox).toBe(5)
    expect(toast.error).toHaveBeenCalledTimes(1)
  })
})
