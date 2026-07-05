# pwa-notifications Specification

## Purpose
TBD - created by archiving change pwa. Update Purpose after archive.
## Requirements
### Requirement: PWA manifest and installability
The frontend SHALL include a Web App Manifest (`manifest.webmanifest`) with name, icons (192px, 512px), `display: standalone`, `start_url`, and `theme_color`. The app MUST be installable via browser "Add to Home Screen" / "Install app" on desktop and mobile.

#### Scenario: Install prompt available
- **WHEN** a user visits the app in a supported browser
- **THEN** the browser shows an install prompt (or the install option appears in browser menu)

#### Scenario: Standalone launch
- **WHEN** the app is launched from the installed icon
- **THEN** it opens without browser chrome in standalone mode

---

### Requirement: Service worker and offline shell
The app SHALL register a Workbox-generated service worker that caches the app shell (HTML, JS, CSS, fonts). When offline, the cached shell SHALL load and display the last-cached message list with an offline banner. API calls SHALL fail gracefully with an offline indicator rather than crashing the UI.

#### Scenario: Offline load
- **WHEN** the device has no network connection and the user opens the app
- **THEN** the app shell loads from cache and shows the message list from the last successful fetch with an "Offline" indicator

#### Scenario: Compose blocked offline
- **WHEN** the user attempts to send an email while offline
- **THEN** the compose send button is disabled and shows "No connection"

---

### Requirement: Web Push subscription management
The system SHALL allow authenticated users to subscribe their browser/device to Web Push notifications using VAPID. Subscription endpoints SHALL be stored per-user per-device.

#### Scenario: Subscribe to push
- **WHEN** a user grants notification permission and the service worker is registered
- **THEN** the frontend obtains a push subscription and sends it to `POST /push-subscriptions`; the backend stores it associated with the user

#### Scenario: Unsubscribe
- **WHEN** a user disables notifications in app settings
- **THEN** the frontend unsubscribes via the Push API and calls `DELETE /push-subscriptions/:id`; the backend removes the subscription

#### Scenario: Stale subscription cleanup
- **WHEN** the backend receives a 410 Gone response from a push endpoint
- **THEN** the subscription is deleted from the database

---

### Requirement: New mail desktop notifications
The system SHALL send a Web Push notification to all of a user's active push subscriptions when new messages arrive during IMAP sync.

#### Scenario: New message push notification
- **WHEN** IMAP sync finds new messages for a user
- **THEN** the backend sends a push notification to all stored subscriptions for that user containing sender name, subject, and account name

#### Scenario: Notification click opens message
- **WHEN** a user clicks a push notification
- **THEN** the app opens (or focuses if already open) and navigates to the relevant message or unified inbox

#### Scenario: Notification permission not granted
- **WHEN** a user has not granted notification permission
- **THEN** no push subscription exists and no notifications are sent (no error)

---

### Requirement: Notification permission prompt
The system SHALL NOT request notification permission on first load. Permission SHALL be requested only after the user explicitly enables notifications in app settings.

#### Scenario: Settings toggle enables notifications
- **WHEN** a user enables "Desktop notifications" in settings
- **THEN** the browser permission prompt is shown; if granted, the push subscription is created

#### Scenario: Permission denied
- **WHEN** the user denies the browser permission prompt
- **THEN** the settings toggle reverts to off and a message explains how to re-enable via browser settings

---

### Requirement: VAPID key configuration
The backend SHALL generate and use VAPID keypairs for Web Push authentication. The VAPID private key SHALL be stored as an environment variable and MUST NOT be logged or exposed via API.

#### Scenario: VAPID key present
- **WHEN** the backend starts with `VAPID_PRIVATE_KEY` and `VAPID_PUBLIC_KEY` env vars set
- **THEN** push dispatch uses these keys for all outgoing push messages

#### Scenario: VAPID public key exposed to frontend
- **WHEN** the frontend requests `GET /push-subscriptions/vapid-public-key`
- **THEN** the backend returns only the public key (base64url encoded)

