import { useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQueryClient } from '@tanstack/react-query'
import { ensureFreshAccessToken } from '@/shared/api'
import { getStoredPushSubscriptionId } from '@/shared/hooks/usePushNotifications'
import type { SyncStatus } from '@/shared/types'
import { z } from 'zod'

interface PushPayload {
  title?: string
  body?: string
  account_name?: string
  message_url?: string
}

const sendStatusSchema = z.object({
  send_id: z.string().min(1),
  status: z.enum(['sent', 'failed']),
  message_id: z.string().nullish(),
  subject: z.string().nullish(),
  error: z.string().nullish(),
})

/** Status emitted after an accepted message finishes background delivery. */
export type SendStatus = z.infer<typeof sendStatusSchema>

/**
 * Parse an untrusted SSE send-status payload.
 * @param data - Raw EventSource message data.
 * @returns A validated send status, or null for malformed input.
 */
export function parseSendStatus(data: string): SendStatus | null {
  try {
    const result = sendStatusSchema.safeParse(JSON.parse(data) as unknown)
    return result.success ? result.data : null
  } catch {
    return null
  }
}

/**
 * Foreground desktop notifications via SSE. Complements the service-worker
 * background push: when web push is unavailable (e.g. Brave blocks the push
 * service) but the app is open, new-message events still surface a native
 * notification. Shown only while the tab is hidden and only when there's no
 * active web-push subscription (otherwise the SW already handles it).
 * @param enabled - Whether foreground desktop mail notifications are enabled.
 * @param onSendStatus - Callback for the terminal result of an accepted send.
 * @returns Nothing; the hook owns and cleans up its EventSource connection.
 */
export function useMailNotifications(enabled: boolean, onSendStatus?: (status: SendStatus) => void) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  useEffect(() => {
    if (typeof EventSource === 'undefined') return

    let source: EventSource | null = null
    let retry: ReturnType<typeof setTimeout> | undefined
    let refresh: ReturnType<typeof setTimeout> | undefined
    let closed = false

    const scheduleMailRefresh = () => {
      if (refresh) clearTimeout(refresh)
      refresh = setTimeout(() => {
        refresh = undefined
        queryClient.invalidateQueries({ queryKey: ['folders'] })
        queryClient.invalidateQueries({ queryKey: ['unified'] })
        queryClient.invalidateQueries({ queryKey: ['unified-counts'] })
        queryClient.invalidateQueries({ queryKey: ['folder-messages'] })
      }, 500)
    }

    const connect = async () => {
      const token = await ensureFreshAccessToken()
      if (closed) return
      if (!token) {
        retry = setTimeout(() => void connect(), 3000)
        return
      }
      source = new EventSource(`/api/events?token=${encodeURIComponent(token)}`)

      source.addEventListener('message', (event) => {
        scheduleMailRefresh()

        // The SW already notifies when web push is active; avoid duplicates.
        if (!enabled) return
        if (getStoredPushSubscriptionId()) return
        if (document.visibilityState === 'visible') return
        if (Notification.permission !== 'granted') return

        let data: PushPayload
        try {
          data = JSON.parse((event as MessageEvent).data)
        } catch {
          return
        }
        const body = data.account_name && data.body ? `${data.account_name}: ${data.body}` : data.body
        const n = new Notification(data.title || 'New mail', {
          body: body || '',
          icon: '/icons/icon-192.png',
        })
        n.onclick = () => {
          window.focus()
          if (data.message_url) navigate(data.message_url)
          n.close()
        }
      })

      source.addEventListener('sync', (event) => {
        let status: SyncStatus
        try {
          status = JSON.parse((event as MessageEvent).data) as SyncStatus
        } catch {
          return
        }
        if (!status.account_id) return
        const previous = queryClient.getQueryData<SyncStatus>(['sync-status', status.account_id])
        queryClient.setQueryData(['sync-status', status.account_id], status)
        if (
          previous?.state !== status.state ||
          previous?.synced !== status.synced ||
          previous?.total !== status.total
        ) {
          scheduleMailRefresh()
        }
      })

      source.addEventListener('contact_sync', () => {
        queryClient.invalidateQueries({ queryKey: ['accounts'] })
        queryClient.invalidateQueries({ queryKey: ['contact-accounts'] })
        queryClient.invalidateQueries({ queryKey: ['contacts'] })
      })

      source.addEventListener('send', (event) => {
        const status = parseSendStatus((event as MessageEvent).data)
        if (!status) return
        onSendStatus?.(status)
        if (status.status === 'sent') scheduleMailRefresh()
      })

      source.onerror = () => {
        // Token may have expired or the connection dropped; reconnect with a
        // fresh token after a short delay.
        source?.close()
        source = null
        if (!closed) retry = setTimeout(() => void connect(), 5000)
      }
    }

    void connect()

    return () => {
      closed = true
      if (retry) clearTimeout(retry)
      if (refresh) clearTimeout(refresh)
      source?.close()
    }
  }, [enabled, navigate, onSendStatus, queryClient])
}
