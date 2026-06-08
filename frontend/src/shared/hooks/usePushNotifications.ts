import { useMutation, useQuery } from '@tanstack/react-query'
import { apiDelete, apiGet, apiPost } from '@/shared/api'

const SUBSCRIPTION_ID_KEY = 'mailquill-push-subscription-id'

interface VapidPublicKeyResponse {
  public_key: string
}

interface PushSubscriptionResponse {
  id: string
}

export function useVapidPublicKey() {
  return useQuery({
    queryKey: ['push-subscriptions', 'vapid-public-key'],
    queryFn: () => apiGet<VapidPublicKeyResponse>('/push-subscriptions/vapid-public-key'),
    staleTime: Infinity,
  })
}

export function useEnablePushNotifications() {
  return useMutation({
    mutationFn: async (publicKey: string) => {
      const registration = await navigator.serviceWorker.ready
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

export function useDisablePushNotifications() {
  return useMutation({
    mutationFn: async () => {
      const registration = await navigator.serviceWorker.ready
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

export function getStoredPushSubscriptionId() {
  return localStorage.getItem(SUBSCRIPTION_ID_KEY)
}

export function pushNotificationsSupported() {
  return 'Notification' in window && 'serviceWorker' in navigator && 'PushManager' in window
}

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
