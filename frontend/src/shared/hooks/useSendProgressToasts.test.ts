import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useSendProgressToasts } from './useSendProgressToasts'

const toastMocks = vi.hoisted(() => ({
  loading: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
}))

vi.mock('sonner', () => ({ toast: toastMocks }))

beforeEach(() => {
  toastMocks.loading.mockReset()
  toastMocks.success.mockReset()
  toastMocks.error.mockReset()
})

describe('useSendProgressToasts', () => {
  it('updates one send toast from progress to success', () => {
    const { result } = renderHook(() => useSendProgressToasts())

    act(() => result.current.showSendQueued({ sendId: 'send-1', subject: 'Invoice' }))
    expect(toastMocks.loading).toHaveBeenCalledWith(
      'Sending message…',
      expect.objectContaining({ id: 'send-1', description: '“Invoice” is being sent in the background.' }),
    )

    act(() => result.current.showSendStatus({
      send_id: 'send-1', status: 'sent', subject: 'Invoice', message_id: '<sent@example.test>', error: null,
    }))
    expect(toastMocks.success).toHaveBeenCalledWith(
      'Message sent successfully.',
      expect.objectContaining({ id: 'send-1', description: '“Invoice” was sent successfully.' }),
    )
  })

  it('shows a persistent failure and never regresses a terminal toast to progress', () => {
    const { result } = renderHook(() => useSendProgressToasts())

    act(() => result.current.showSendStatus({
      send_id: 'send-2', status: 'failed', subject: 'Invoice', message_id: null, error: 'OAuth expired',
    }))
    act(() => result.current.showSendQueued({ sendId: 'send-2', subject: 'Invoice' }))

    expect(toastMocks.error).toHaveBeenCalledWith(
      'Message could not be sent.',
      expect.objectContaining({ id: 'send-2', description: '“Invoice” failed: OAuth expired' }),
    )
    expect(toastMocks.loading).not.toHaveBeenCalled()
  })
})
