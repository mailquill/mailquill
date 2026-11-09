# Security Model

What protects user data in a Mailquill deployment, and where the boundaries
are. Operators should read this together with
[reverse-proxy.md](reverse-proxy.md); contributors should keep the
guarantees listed here intact.

## Authentication

- Local accounts: Argon2 password hashes in `app.db`, minimum password
  length 8. Registration can be disabled with `registration_enabled = false`.
- Access token: HS256 JWT signed with `JWT_SECRET`, valid for 15 minutes,
  sent as `Authorization: Bearer` by the web client, which keeps it in
  browser storage.
- Refresh token: 32 random bytes, stored hashed in `refresh_tokens`,
  delivered as an `HttpOnly; SameSite=Strict` cookie scoped to
  `/api/auth/refresh`. It carries `Secure` whenever `APP_BASE_URL` starts
  with `https://`. Each refresh rotates the token; a token presented again
  within 60 s of its rotation is treated as a concurrent tab, anything later
  as reuse and rejected. "Remember me" sessions live 30 days sliding,
  others 24 hours and end with the browser session.
- Brute force: failed logins are counted per account and, when a proxy sets
  `X-Forwarded-For`/`X-Real-IP`, per client address. Ten failures in 15
  minutes answer `429` until the window passes; registration allows 20
  attempts per address and window. Unknown accounts are verified against a
  dummy hash so response time does not reveal registered addresses.
- Token in the URL: `/api/events` (SSE), the image proxy and the OAuth start
  endpoint cannot carry a header, so they accept `?token=`. Request logging
  redacts that parameter.

## Data isolation

Every user has their own SQLite database (`mail.db`) under
`DATA_DIR/users/<user id>/`; all mailbox routes open the database of the
authenticated user, so message, folder and attachment ids of another user
never resolve. Shared tables in `app.db` (settings, push subscriptions,
refresh tokens) are always queried with the user id.

Mail credentials and OAuth tokens are encrypted with AES-256-GCM under
`CREDENTIAL_ENCRYPTION_KEY`. Generate both secrets with `mailquill secrets`.

## Response headers

All responses carry:

- `Content-Security-Policy`: `default-src 'self'`, scripts only from the app
  origin, inline styles allowed, `object-src 'none'`, `frame-ancestors
  'none'`, `base-uri 'self'`, `form-action 'self'`. Image sources are limited
  to the app origin when the remote image proxy is enabled.
- `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`,
  `Referrer-Policy: strict-origin-when-cross-origin`.

HTTPS and `Strict-Transport-Security` are the reverse proxy's job.

## Untrusted mail content

- HTML bodies render in an `<iframe sandbox>` without `allow-scripts`.
  Before that, `prepareEmailHtml` removes `script`, `iframe`, `object`,
  `embed`, `applet`, every `on*` attribute, `javascript:` URLs and
  `meta http-equiv`.
- Remote resources (images, media, CSS `url()`, `<link href>`) are stripped
  unless the user allows them for the message or sender. With the proxy
  enabled they are rewritten to `/api/remote-content/image`.
- The image proxy accepts only `http(s)` URLs, refuses `localhost` and every
  non-public address range (RFC 1918, loopback, link-local, shared address
  space, multicast, reserved, and the IPv6 forms that embed an IPv4 address),
  resolves the host once and pins the connection to the vetted addresses,
  follows at most three redirects with the same checks, accepts only
  `image/*` up to 10 MB, and serves the bytes with `nosniff` and a `sandbox`
  CSP. Failures return a transparent GIF.
- Attachments: types a browser would execute or parse as a document (HTML,
  XHTML, SVG, XML, JavaScript, CSS) are served as `application/octet-stream`
  with an `attachment` disposition. In-app previews exist only for raster
  images and PDF, with the blob type pinned client-side.
- Phishing analysis flags suspicious messages; the "not spam" action teaches
  the filter per sender domain.

## Outbound connections

Mailquill connects to the IMAP, SMTP, CalDAV and CardDAV servers a user
configures, including hosts on private networks; that is the product's
purpose, not a bypass. TLS exceptions (a pinned certificate or
`accept_invalid_tls`) are per account and explicit. Autoconfig and WKD key
discovery build URLs from the e-mail domain, which is validated as a
hostname first.

## Input handling

- SQL statements use bound parameters; the few dynamically assembled queries
  only splice column names or fixed fragments chosen from allow-lists.
- Outgoing `Subject`, `In-Reply-To` and `References` lose CR/LF before the
  message is built; recipient addresses are parsed by lettre. The sender
  address must be the account's primary address or one of its aliases.
- Sieve scripts quote every user value and keep rule names on their comment
  line.
- All free-text fields have length limits (`validate.rs`).

## Reporting

Security issues: see [CONTRIBUTING.md](../CONTRIBUTING.md) for contact
details.
