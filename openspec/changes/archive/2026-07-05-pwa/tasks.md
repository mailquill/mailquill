## 1. PWA — Backend

- [x] 1.1 Add `web-push` crate; generate VAPID keypair, document `VAPID_PRIVATE_KEY` / `VAPID_PUBLIC_KEY` env vars in `.env.example`
- [x] 1.2 Implement `GET /push-subscriptions/vapid-public-key` — return VAPID public key to frontend
- [x] 1.3 Implement `POST /push-subscriptions` — store subscription for authenticated user's device
- [x] 1.4 Implement `DELETE /push-subscriptions/:id` — remove subscription
- [x] 1.5 Implement push dispatch in IMAP sync task: after storing new messages, send Web Push to all user subscriptions via `web-push` crate
- [x] 1.6 Handle 410 Gone push endpoint response → delete stale subscription (idempotent)

## 2. PWA — Frontend

- [x] 2.1 Configure `vite-plugin-pwa`; generate `manifest.webmanifest` with name, icons, `display: standalone`
- [x] 2.2 Create app icons (192×192, 512×512, maskable variant)
- [x] 2.3 Configure Workbox: cache-first for static assets, network-first for `/api` routes
- [x] 2.4 Implement offline banner component shown when `navigator.onLine` is false
- [x] 2.5 Disable compose send button when offline
- [x] 2.6 Fetch VAPID public key from backend on app init; store in app state
- [x] 2.7 Build "Desktop notifications" toggle in settings; request permission only when toggled on
- [x] 2.8 On permission grant: call `serviceWorkerRegistration.pushManager.subscribe()` and POST subscription to backend
- [x] 2.9 On permission deny: revert toggle, show instructions for re-enabling in browser settings
- [x] 2.10 Implement service worker `push` event handler: show `self.registration.showNotification()` with sender + subject
- [x] 2.11 Implement service worker `notificationclick` handler: `clients.openWindow()` to message URL or unified inbox
