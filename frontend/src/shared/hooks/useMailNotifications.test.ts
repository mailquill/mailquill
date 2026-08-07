import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import { createElement, type PropsWithChildren } from 'react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { parseSendStatus, useMailNotifications } from './useMailNotifications'

vi.mock('@/shared/api', () => ({
  ensureFreshAccessToken: vi.fn().mockResolvedValue('access-token'),
}))

vi.mock('@/shared/hooks/usePushNotifications', () => ({
  getStoredPushSubscriptionId: vi.fn().mockReturnValue(null),
}))

class FakeEventSource {
  static latest: FakeEventSource | null = null
  static created = 0
  private readonly listeners = new Map<string, EventListener>()
  readonly url: string
  onerror: (() => void) | null = null
  onopen: (() => void) | null = null

  constructor(url: string) {
    this.url = url
    FakeEventSource.latest = this
    FakeEventSource.created += 1
  }

  addEventListener(type: string, listener: EventListener) {
    this.listeners.set(type, listener)
  }

  emit(type: string, data: string) {
    this.listeners.get(type)?.({ data } as MessageEvent)
  }

  close() {}
}

function Wrapper({ children }: PropsWithChildren) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return createElement(
    MemoryRouter,
    null,
    createElement(QueryClientProvider, { client: queryClient }, children),
  )
}

beforeEach(() => {
  FakeEventSource.latest = null
  FakeEventSource.created = 0
  vi.stubGlobal('EventSource', FakeEventSource)
})

describe('parseSendStatus', () => {
  it('accepts a background send failure', () => {
    expect(
      parseSendStatus(
        JSON.stringify({
          send_id: 'send-1',
          status: 'failed',
          message_id: null,
          subject: 'Quarterly report',
          error: 'SMTP connection timed out',
        }),
      ),
    ).toEqual({
      send_id: 'send-1',
      status: 'failed',
      message_id: null,
      subject: 'Quarterly report',
      error: 'SMTP connection timed out',
    })
  })

  it('rejects malformed and unknown send events', () => {
    expect(parseSendStatus('{not-json')).toBeNull()
    expect(parseSendStatus(JSON.stringify({ send_id: 'send-1', status: 'queued' }))).toBeNull()
  })

  it('reports a provider failure received after queue acceptance', async () => {
    const onSendStatus = vi.fn()
    renderHook(() => useMailNotifications(false, onSendStatus), { wrapper: Wrapper })

    await waitFor(() => expect(FakeEventSource.latest).not.toBeNull())
    act(() => {
      FakeEventSource.latest?.emit(
        'send',
        JSON.stringify({ send_id: 'send-2', status: 'failed', subject: 'Invoice', error: 'OAuth expired' }),
      )
    })

    expect(onSendStatus).toHaveBeenCalledWith(
      expect.objectContaining({ send_id: 'send-2', status: 'failed', error: 'OAuth expired' }),
    )
  })

  it('reports successful background delivery', async () => {
    const onSendStatus = vi.fn()
    renderHook(() => useMailNotifications(false, onSendStatus), { wrapper: Wrapper })

    await waitFor(() => expect(FakeEventSource.latest).not.toBeNull())
    act(() => {
      FakeEventSource.latest?.emit(
        'send',
        JSON.stringify({ send_id: 'send-3', status: 'sent', subject: 'Invoice', message_id: '<sent@example.test>' }),
      )
    })

    expect(onSendStatus).toHaveBeenCalledWith(
      expect.objectContaining({ send_id: 'send-3', status: 'sent', subject: 'Invoice' }),
    )
  })
})

describe('useMailNotifications connection lifecycle', () => {
  it('keeps one connection across rerenders with changing callbacks', async () => {
    const { rerender } = renderHook(({ cb }) => useMailNotifications(false, cb), {
      wrapper: Wrapper,
      initialProps: { cb: vi.fn() },
    })

    await waitFor(() => expect(FakeEventSource.latest).not.toBeNull())
    const first = FakeEventSource.latest
    rerender({ cb: vi.fn() })
    rerender({ cb: vi.fn() })

    expect(FakeEventSource.created).toBe(1)
    expect(FakeEventSource.latest).toBe(first)
  })

  it('refreshes mail lists after a reconnect', async () => {
    vi.useFakeTimers()
    try {
      const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
      const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
      const wrapper = ({ children }: PropsWithChildren) =>
        createElement(
          MemoryRouter,
          null,
          createElement(QueryClientProvider, { client: queryClient }, children),
        )
      renderHook(() => useMailNotifications(false), { wrapper })

      // Flush the async connect(), then the first open — no catch-up refresh.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0)
      })
      expect(FakeEventSource.latest).not.toBeNull()
      act(() => FakeEventSource.latest?.onopen?.())
      await act(async () => {
        await vi.advanceTimersByTimeAsync(600)
      })
      expect(invalidate).not.toHaveBeenCalled()

      // Drop the connection; the retry reconnects after 5s and the reopen
      // reconciles the mail lists once the 500ms debounce elapses.
      act(() => FakeEventSource.latest?.onerror?.())
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5000)
      })
      expect(FakeEventSource.created).toBe(2)
      act(() => FakeEventSource.latest?.onopen?.())
      await act(async () => {
        await vi.advanceTimersByTimeAsync(600)
      })
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['unified'] })
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['folder-messages'] })
    } finally {
      vi.useRealTimers()
    }
  })
})
