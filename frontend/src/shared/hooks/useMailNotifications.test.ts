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
  private readonly listeners = new Map<string, EventListener>()
  readonly url: string
  onerror: (() => void) | null = null

  constructor(url: string) {
    this.url = url
    FakeEventSource.latest = this
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
