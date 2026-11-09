import { useMutation, useQuery } from '@tanstack/react-query'
import { apiDelete, apiGet, apiPost } from '@/shared/api'

const SUBSCRIPTION_ID_KEY = 'mailquill-push-subscription-id'

interface VapidPublicKeyResponse {
  public_key: string
}

interface PushSubscriptionResponse {
  id: string
}

/**
 * The server's VAPID public key from `GET /push-subscriptions/vapid-public-key`,
 * cached forever under `['push-subscriptions', 'vapid-public-key']`; it only
 * changes with a server redeploy.
 * @returns A query for `{ public_key }` in URL-safe base64.
 */
export function useVapidPublicKey() {
  return useQuery({
    queryKey: ['push-subscriptions', 'vapid-public-key'],
    queryFn: () => apiGet<VapidPublicKeyResponse>('/push-subscriptions/vapid-public-key'),
    staleTime: Infinity,
  })
}

/** serviceWorker.ready never rejects; cap the wait so a missing/failed SW
 *  surfaces as an error instead of a silently hanging toggle. */
async function serviceWorkerReady(): Promise<ServiceWorkerRegistration> {
  return Promise.race([
    navigator.serviceWorker.ready,
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error('service worker not ready')), 5000),
    ),
  ])
}

/**
 * Subscribe this browser to web push: waits for the service worker, reuses or
 * creates the `PushManager` subscription with the VAPID key, registers it
 * through `POST /push-subscriptions` and remembers the returned id in
 * localStorage so the foreground SSE notifier knows push is active.
 * @returns A mutation taking the VAPID public key and resolving to the
 *   subscription `{ id }`.
 */
export function useEnablePushNotifications() {
  return useMutation({
    mutationFn: async (publicKey: string) => {
      const registration = await serviceWorkerReady()
      const existing = await registration.pushManager.getSubscription()
      const subscription =
        existing ??
        (await registration.pushManager.subscribe({
          userVisibleOnly: true,
          applicationServerKey: urlBase64ToArrayBuffer(publicKey),
        }))

      const response = await apiPost<PushSubscriptionResponse>('/push-subscriptions', subscription.toJSON())
      localStorage.setItem(SUBSCRIPTION_ID_KEY, response.id)

      return response
    },
  })
}

/**
 * Unsubscribe this browser from web push: drops the `PushManager`
 * subscription, deletes the server record through
 * `DELETE /push-subscriptions/:id` and forgets the stored id.
 * @returns A mutation taking no variables.
 */
export function useDisablePushNotifications() {
  return useMutation({
    mutationFn: async () => {
      const registration = await serviceWorkerReady()
      const subscription = await registration.pushManager.getSubscription()
      await subscription?.unsubscribe()

      const id = localStorage.getItem(SUBSCRIPTION_ID_KEY)
      if (id) {
        await apiDelete(`/push-subscriptions/${id}`)
        localStorage.removeItem(SUBSCRIPTION_ID_KEY)
      }
    },
  })
}

/**
 * Whether this browser holds an active push subscription registered with the server.
 * @returns The subscription id saved on enable, or null when push is off.
 */
export function getStoredPushSubscriptionId() {
  return localStorage.getItem(SUBSCRIPTION_ID_KEY)
}

/**
 * Feature-detect the Notification, Service Worker and Push APIs.
 * @returns True when the browser can deliver web push at all.
 */
export function pushNotificationsSupported() {
  return 'Notification' in window && 'serviceWorker' in navigator && 'PushManager' in window
}

/** Decode the URL-safe base64 VAPID key into the bytes `pushManager.subscribe` expects. */
function urlBase64ToArrayBuffer(base64String: string): ArrayBuffer {
  const padding = '='.repeat((4 - (base64String.length % 4)) % 4)
  const base64 = `${base64String}${padding}`.replace(/-/g, '+').replace(/_/g, '/')
  const rawData = window.atob(base64)
  const outputArray = new Uint8Array(rawData.length)

  for (let index = 0; index < rawData.length; index += 1) {
    outputArray[index] = rawData.charCodeAt(index)
  }

  return outputArray.buffer
}
