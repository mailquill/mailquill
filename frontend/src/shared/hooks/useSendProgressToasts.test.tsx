import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useSendProgressToasts } from './useSendProgressToasts'

const toastMocks = vi.hoisted(() => ({
  loading: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
}))
const navigateMock = vi.hoisted(() => vi.fn())

vi.mock('sonner', () => ({ toast: toastMocks }))
vi.mock('react-router-dom', () => ({ useNavigate: () => navigateMock }))

beforeEach(() => {
  toastMocks.loading.mockReset()
  toastMocks.success.mockReset()
  toastMocks.error.mockReset()
  navigateMock.mockReset()
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

  it('offers a credentials-check action that jumps to the failing account, when known', () => {
    const { result } = renderHook(() => useSendProgressToasts())

    act(() => result.current.showSendStatus({
      send_id: 'send-3', account_id: 'acc-1', status: 'failed', subject: 'Invoice', message_id: null,
      error: '535 5.7.8 Username and Password not accepted',
    }))

    const call = toastMocks.error.mock.calls[0]
    const action = call[1].action as { label: string; onClick: () => void }
    expect(action.label).toBe('Check credentials')
    action.onClick()
    expect(navigateMock).toHaveBeenCalledWith('/mail/settings?focus=acc-1')
  })

  it('omits the action when the failed send has no account_id (e.g. an older client)', () => {
    const { result } = renderHook(() => useSendProgressToasts())

    act(() => result.current.showSendStatus({
      send_id: 'send-4', status: 'failed', subject: 'Invoice', message_id: null, error: 'boom',
    }))

    const call = toastMocks.error.mock.calls[0]
    expect(call[1].action).toBeUndefined()
  })
})
