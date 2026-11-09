import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Message } from '@/shared/types'
import { MessageList } from './MessageList'

// jsdom reports zero sizes, so the real virtualizer would render no rows.
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({ index, key: index, start: index * 60, size: 60 })),
    getTotalSize: () => count * 60,
    measureElement: () => undefined,
  }),
}))

const mutations = vi.hoisted(() => ({
  markRead: vi.fn(),
  archive: vi.fn(),
  remove: vi.fn(),
  deleteThread: vi.fn(),
  toggleFlag: vi.fn(),
  bulk: vi.fn(),
  move: vi.fn(),
  notSpam: vi.fn(),
  prefetch: vi.fn(),
}))

vi.mock('@/shared/hooks/useMessages', () => ({
  useMarkRead: () => ({ mutate: mutations.markRead }),
  useArchiveMessage: () => ({ mutate: mutations.archive }),
  useDeleteMessage: () => ({ mutate: mutations.remove }),
  useDeleteThread: () => ({ mutate: mutations.deleteThread }),
  useToggleFlag: () => ({ mutate: mutations.toggleFlag }),
  useBulkAction: () => ({ mutate: mutations.bulk }),
  useMoveMessage: () => ({ mutate: mutations.move }),
  useNotSpamMessage: () => ({ mutate: mutations.notSpam }),
  prefetchMessage: mutations.prefetch,
}))

vi.mock('@/shared/hooks/useAccounts', () => ({
  useFolders: () => ({ data: [] }),
}))

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
    subject: 'First subject',
    from_addr: 'Alice <alice@example.test>',
    to_addrs: '',
    cc_addrs: '',
    snippet: 'snippet',
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

const messages = [
  message({ id: 'm1', thread_id: 't1', subject: 'First subject' }),
  message({ id: 'm2', thread_id: 't2', subject: 'Second subject' }),
]

function renderList(props: Partial<Parameters<typeof MessageList>[0]> = {}) {
  const onSelect = vi.fn()
  render(
    <MemoryRouter>
      <QueryClientProvider client={new QueryClient()}>
        <MessageList messages={messages} onSelect={onSelect} {...props} />
      </QueryClientProvider>
    </MemoryRouter>,
  )
  return { onSelect }
}

const rowOf = (subject: string) => screen.getByText(subject).closest('button') as HTMLButtonElement

beforeEach(() => {
  vi.useFakeTimers()
  Object.values(mutations).forEach((fn) => fn.mockReset())
})

afterEach(() => {
  vi.useRealTimers()
})

describe('MessageList', () => {
  it('prefetches a hovered row only after the hover intent delay', () => {
    renderList()
    // The newest row is prefetched on mount; ignore that call.
    mutations.prefetch.mockClear()

    fireEvent.mouseEnter(rowOf('Second subject'))
    expect(mutations.prefetch).not.toHaveBeenCalled()

    act(() => {
      vi.advanceTimersByTime(150)
    })
    expect(mutations.prefetch).toHaveBeenCalledTimes(1)
    expect(mutations.prefetch).toHaveBeenCalledWith(expect.anything(), 'm2')
  })

  it('does not prefetch rows the pointer only passed over', () => {
    renderList()
    mutations.prefetch.mockClear()

    fireEvent.mouseEnter(rowOf('First subject'))
    act(() => {
      vi.advanceTimersByTime(50)
    })
    fireEvent.mouseLeave(rowOf('First subject'))
    fireEvent.mouseEnter(rowOf('Second subject'))
    act(() => {
      vi.advanceTimersByTime(150)
    })

    expect(mutations.prefetch).toHaveBeenCalledTimes(1)
    expect(mutations.prefetch).toHaveBeenCalledWith(expect.anything(), 'm2')
  })

  it('opens the clicked conversation', () => {
    const { onSelect } = renderList()
    fireEvent.click(rowOf('Second subject'))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ id: 'm2' }))
  })

  it('deletes the open conversation on the Delete key, but not while typing', () => {
    render(
      <MemoryRouter>
        <QueryClientProvider client={new QueryClient()}>
          <input aria-label="typing" />
          <MessageList messages={messages} onSelect={vi.fn()} activeId="t2" />
        </QueryClientProvider>
      </MemoryRouter>,
    )

    const input = screen.getByLabelText('typing')
    input.focus()
    fireEvent.keyDown(window, { key: 'Delete' })
    expect(mutations.deleteThread).not.toHaveBeenCalled()

    input.blur()
    fireEvent.keyDown(window, { key: 'Delete' })
    expect(mutations.deleteThread).toHaveBeenCalledWith('t2')
  })

  it('stars a row without opening it', () => {
    const { onSelect } = renderList()
    const star = rowOf('First subject').querySelector('[role="button"]') as HTMLElement
    fireEvent.mouseDown(star, { button: 0 })
    expect(mutations.toggleFlag).toHaveBeenCalledWith({ id: 'm1', is_flagged: true })
    expect(onSelect).not.toHaveBeenCalled()
  })
})
