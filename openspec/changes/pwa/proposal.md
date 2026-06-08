## Why

Once the email client works, users expect it to behave like a native app: installable on desktop and mobile, functional offline (at least for cached messages), and able to deliver desktop notifications for new mail without the browser tab open. Web Push + PWA achieves this without FCM or any third-party relay — fully self-hosted.

## What Changes

- PWA manifest (`manifest.webmanifest`): name, icons, `display: standalone` so the app installs from the browser
- Service worker via Workbox: cache-first for static assets, network-first for API routes
- Offline shell: app loads and shows cached message list when offline; compose send blocked offline; "Body not available offline" state already handled in `email-core`
- VAPID keypair generation and `VAPID_PRIVATE_KEY` / `VAPID_PUBLIC_KEY` env vars
- Push subscription storage and management API (`push_subscriptions` table already created in `foundation`)
- Web Push dispatch from IMAP sync task after new messages arrive
- Frontend: "Desktop notifications" toggle in settings, permission request flow, service worker push event handler, notification click → open message

## Capabilities

### New Capabilities

- `pwa-notifications`: PWA manifest, service worker, offline shell, and Web Push desktop notifications for new mail

### Modified Capabilities

## Impact

- Depends on `email-core` (sync task, message routes, settings page)
- Rust addition: `web-push` crate (VAPID push dispatch)
- Frontend addition: `vite-plugin-pwa` (already installed in `foundation`), Workbox service worker config, push subscription management
- New API routes: `GET /push-subscriptions/vapid-public-key`, `POST /push-subscriptions`, `DELETE /push-subscriptions/:id`
- Firebase Cloud Messaging (FCM) and all third-party push relays are explicitly prohibited — direct VAPID only
