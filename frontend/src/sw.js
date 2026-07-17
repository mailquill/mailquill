/// <reference lib="webworker" />
import { clientsClaim } from 'workbox-core'
import { precacheAndRoute } from 'workbox-precaching'
import { registerRoute } from 'workbox-routing'
import { CacheFirst } from 'workbox-strategies'
import { shouldCacheStaticRequest } from './shared/lib/serviceWorkerRoutes'

clientsClaim()
self.skipWaiting()

precacheAndRoute(self.__WB_MANIFEST)

self.addEventListener('activate', (event) => {
  event.waitUntil(caches.delete('mailquill-api'))
})

// Runtime caching only in production. In dev these routes would intercept the
// Vite HMR modules (script/style requests) and serve them CacheFirst, breaking
// hot reload and shipping stale code. The push handler below runs in both, so
// notifications still work in dev (devOptions registers the SW there).
if (import.meta.env.PROD) {
  registerRoute(
    ({ request, url }) => shouldCacheStaticRequest(request, url, self.location.origin),
    new CacheFirst({
      cacheName: 'mailquill-static',
    }),
  )

  // API responses are user-specific and may be streaming (notably /api/events).
  // Let the browser handle them directly: Cache.put cannot store an active SSE
  // body, and authenticated responses must not enter a service-worker cache.
}

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
