import { useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { getAccessToken } from '@/shared/api'
import { getStoredPushSubscriptionId } from '@/shared/hooks/usePushNotifications'

interface PushPayload {
  title?: string
  body?: string
  account_name?: string
  message_url?: string
}

/**
 * Foreground desktop notifications via SSE. Complements the service-worker
 * background push: when web push is unavailable (e.g. Brave blocks the push
 * service) but the app is open, new-message events still surface a native
 * notification. Shown only while the tab is hidden and only when there's no
 * active web-push subscription (otherwise the SW already handles it).
 */
export function useMailNotifications(enabled: boolean) {
  const navigate = useNavigate()

  useEffect(() => {
    if (!enabled || typeof EventSource === 'undefined') return

    let source: EventSource | null = null
    let retry: ReturnType<typeof setTimeout> | undefined
    let closed = false

    const connect = () => {
      const token = getAccessToken()
      if (!token) {
        retry = setTimeout(connect, 3000)
        return
      }
      source = new EventSource(`/api/events?token=${encodeURIComponent(token)}`)

      source.addEventListener('message', (event) => {
        // The SW already notifies when web push is active; avoid duplicates.
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

      source.onerror = () => {
        // Token may have expired or the connection dropped; reconnect with a
        // fresh token after a short delay.
        source?.close()
        source = null
        if (!closed) retry = setTimeout(connect, 5000)
      }
    }

    connect()

    return () => {
      closed = true
      if (retry) clearTimeout(retry)
      source?.close()
    }
  }, [enabled, navigate])
}
