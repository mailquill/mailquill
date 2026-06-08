/// <reference lib="webworker" />
import { clientsClaim } from 'workbox-core'
import { precacheAndRoute } from 'workbox-precaching'
import { registerRoute } from 'workbox-routing'
import { CacheFirst, NetworkFirst } from 'workbox-strategies'

clientsClaim()
self.skipWaiting()

precacheAndRoute(self.__WB_MANIFEST)

registerRoute(
  ({ request }) => ['script', 'style', 'font', 'image'].includes(request.destination),
  new CacheFirst({
    cacheName: 'mailquill-static',
  }),
)

registerRoute(
  ({ url }) => url.pathname.startsWith('/api/'),
  new NetworkFirst({
    cacheName: 'mailquill-api',
    networkTimeoutSeconds: 3,
  }),
)

self.addEventListener('push', (event) => {
  const data = readPushPayload(event)
  event.waitUntil(
    self.registration.showNotification(data.title, {
      body: data.body,
      data: {
        url: data.url,
      },
      icon: '/icons/icon-192.png',
      badge: '/icons/icon-192.png',
    }),
  )
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const targetUrl = new URL(event.notification.data?.url ?? '/mail/unified', self.location.origin).href

  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((clientList) => {
      for (const client of clientList) {
        if ('focus' in client && client.url.startsWith(self.location.origin)) {
          client.navigate(targetUrl)
          return client.focus()
        }
      }

      return self.clients.openWindow(targetUrl)
    }),
  )
})

function readPushPayload(event) {
  const fallback = {
    title: 'New mail',
    body: 'Open Mailquill to read the message.',
    url: '/mail/unified',
  }

  if (!event.data) {
    return fallback
  }

  try {
    const value = event.data.json()

    return {
      title: value.title || fallback.title,
      body: value.account_name && value.body ? `${value.account_name}: ${value.body}` : value.body || fallback.body,
      url: value.message_url || fallback.url,
    }
  } catch {
    return fallback
  }
}
