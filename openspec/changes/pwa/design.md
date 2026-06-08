## Context

Builds on `email-core`. The IMAP sync task is already running; this change adds Web Push dispatch to it. The frontend settings page exists; this change adds a notifications toggle. All push must go direct VAPID — no FCM, no relay services (incompatible with self-hosted privacy requirements: FCM routes all notifications through Google servers).

## Decisions

### D1: Web Push — direct VAPID, no relay

Backend holds VAPID private key (`VAPID_PRIVATE_KEY`), sends push directly to browser vendor endpoints. `web-push` Rust crate handles VAPID signing and HTTP delivery. Push subscription endpoint URL is browser-vendor-specific (Chrome → Google, Firefox → Mozilla) — this is inherent to the Web Push standard, not a relay choice.

Store `push_subscriptions`: `(id, user_id, endpoint, p256dh, auth, created_at)`. One row per device. On 410 Gone response from push endpoint: delete stale subscription (idempotent).

### D2: Service worker — Workbox via vite-plugin-pwa

`vite-plugin-pwa` wraps Workbox and auto-generates service worker + manifest from Vite config. Caching strategy:
- **Cache-first** for static assets (JS, CSS, fonts, icons) — fast loads, no network round-trip
- **Network-first** for `/api/*` routes — mail data must be fresh; fall back to cache only when offline

Offline behaviour: app shell loads from cache. Message list shows cached data. Compose send button disabled when `!navigator.onLine`. Body fetch blocked → "Body not available offline" state (already implemented in `email-core` frontend).

### D3: Push permission UX — defer until first new mail

Browser push permission prompt is a one-shot: if denied, recovery requires browser settings. Show permission prompt only after the first new-mail event in the current session (not on login). This maximises acceptance rate by deferring until the user has seen value.

On deny: revert toggle, show instructions for re-enabling in browser settings.

### D4: VAPID key rotation

VAPID key rotation invalidates all existing push subscriptions — all users must re-subscribe. Treat keys as long-lived infrastructure secrets. Document rotation procedure in ops README. Do not rotate automatically.
