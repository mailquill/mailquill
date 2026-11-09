const BASE = '/api'

let accessToken: string | null = null
let refreshSubscriber: ((token: string | null) => void) | null = null

/**
 * Store the bearer token that every following API request sends. Pass null on
 * logout so requests go out unauthenticated.
 * @param token - The access token, or null to clear it.
 */
export function setAccessToken(token: string | null) {
  accessToken = token
}

/**
 * Read the access token currently held in memory, without checking its expiry.
 * @returns The stored token, or null when nobody is signed in.
 */
export function getAccessToken() {
  return accessToken
}

/**
 * Register the single listener told about tokens produced by the refresh flow:
 * the new token after a successful refresh, or null when the server rejected
 * the refresh cookie. The auth store uses it to persist or drop the session.
 * @param subscriber - Receives the new token, or null on a genuine logout.
 */
export function setRefreshSubscriber(subscriber: (token: string | null) => void) {
  refreshSubscriber = subscriber
}

let refreshInFlight: Promise<string | null> | null = null

const ACCESS_TOKEN_REFRESH_SKEW_SECONDS = 60

/**
 * Decode the JWT payload locally and report whether `exp` falls within the
 * given number of seconds. Malformed tokens count as not expiring.
 */
function accessTokenExpiresWithin(token: string, seconds: number): boolean {
  try {
    const payload = token.split('.')[1]
    if (!payload) return false
    const normalized = payload.replaceAll('-', '+').replaceAll('_', '/')
    const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, '=')
    const decoded = JSON.parse(atob(padded)) as { exp?: unknown }
    return typeof decoded.exp === 'number'
      && decoded.exp <= Math.floor(Date.now() / 1000) + seconds
  } catch {
    return false
  }
}

/**
 * Return the current access token, refreshing it first when it is close to
 * expiry. Calls share the same refresh request through `refreshAccessToken`.
 *
 * @returns The usable access token, or null when no session can be refreshed.
 */
export async function ensureFreshAccessToken(): Promise<string | null> {
  if (!accessToken) return null
  if (accessTokenExpiresWithin(accessToken, ACCESS_TOKEN_REFRESH_SKEW_SECONDS)) {
    return refreshAccessToken()
  }
  return accessToken
}

/**
 * Exchange the HTTP-only refresh cookie for a new access token through
 * `POST /auth/refresh`. Concurrent callers share one in-flight request so the
 * rotating refresh token is spent only once.
 * @returns The new access token, or null when no token could be obtained. The
 *   session is dropped (subscriber told with null) only on a 401; network and
 *   server errors leave the current token in place for a later retry.
 */
export async function refreshAccessToken(): Promise<string | null> {
  // Single-flight: when the access token expires the app fires many requests at
  // once, each hitting 401. The refresh token rotates server-side (the old one
  // is revoked), so parallel /auth/refresh calls would revoke each other and
  // log the user out. De-dupe them onto one in-flight refresh.
  if (refreshInFlight) return refreshInFlight
  refreshInFlight = doRefresh().finally(() => {
    refreshInFlight = null
  })
  return refreshInFlight
}

/** The actual refresh request; see the error handling for when a failure logs out. */
async function doRefresh(): Promise<string | null> {
  try {
    const res = await fetch(`${BASE}/auth/refresh`, {
      method: 'POST',
      credentials: 'include',
    })
    if (res.status === 401) {
      // The server explicitly rejected the refresh cookie (missing, expired,
      // or revoked) — this is a genuine logout, not a hiccup.
      accessToken = null
      refreshSubscriber?.(null)
      return null
    }
    if (!res.ok) {
      // A non-401 failure (5xx, maintenance, ...) says nothing about whether
      // the refresh cookie is still good. Leave the session alone so the
      // next attempt — once the server recovers — can still succeed.
      return null
    }
    const data = await res.json()
    accessToken = data.access_token
    refreshSubscriber?.(accessToken)
    return accessToken
  } catch {
    // fetch() throwing means we couldn't reach the server at all (offline,
    // DNS, timeout) — not that the session is invalid. A PWA left offline
    // overnight must not be logged out by this: keep the current access
    // token and the persisted session as-is, and let the next reachable
    // attempt (once back online) refresh normally via the still-valid
    // "remember me" cookie.
    return null
  }
}

/**
 * `fetch` against the `/api` base: adds the JSON content type and the bearer
 * header, sends cookies, and on a 401 refreshes the token once and replays the
 * request.
 * @param path - Path relative to the API base, for example `/messages/123`.
 * @param options - Standard `fetch` options; headers are merged over the defaults.
 * @returns The raw response, non-2xx ones included; callers decide how to parse it.
 */
async function apiFetch(
  path: string,
  options: RequestInit = {},
): Promise<Response> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string>),
  }
  const currentToken = await ensureFreshAccessToken()
  if (currentToken) {
    headers['Authorization'] = `Bearer ${currentToken}`
  }

  let res = await fetch(`${BASE}${path}`, {
    ...options,
    headers,
    credentials: 'include',
  })

  // JWT expired — try refresh once
  if (res.status === 401) {
    const newToken = await refreshAccessToken()
    if (newToken) {
      headers['Authorization'] = `Bearer ${newToken}`
      res = await fetch(`${BASE}${path}`, {
        ...options,
        headers,
        credentials: 'include',
      })
    }
  }

  return res
}

// Endpoints that respond 204 No Content (thread actions, allowlist writes, …)
// have no body to parse; res.json() would throw and turn a successful call
// into a mutation error.
async function parseResponse<T>(res: Response): Promise<T> {
  if (!res.ok) throw new ApiError(res.status, await res.text())
  if (res.status === 204) return undefined as T
  const text = await res.text()
  return (text ? JSON.parse(text) : undefined) as T
}

/**
 * GET a JSON resource.
 * @param path - Path relative to the API base.
 * @returns The parsed response body; a 204 or empty body resolves to undefined.
 * @throws {ApiError} When the response status is not 2xx.
 */
export async function apiGet<T>(path: string): Promise<T> {
  return parseResponse(await apiFetch(path))
}

/**
 * GET a binary resource (attachments, inline images) without JSON parsing.
 * @param path - Path relative to the API base.
 * @returns The response body as a Blob.
 * @throws {ApiError} When the response status is not 2xx.
 */
export async function apiGetBlob(path: string): Promise<Blob> {
  const res = await apiFetch(path)
  if (!res.ok) throw new ApiError(res.status, await res.text())
  return res.blob()
}

/**
 * POST a JSON body and parse the JSON reply.
 * @param path - Path relative to the API base.
 * @param body - Serialised with `JSON.stringify`; the request has no body when undefined.
 * @returns The parsed response body; a 204 or empty body resolves to undefined.
 * @throws {ApiError} When the response status is not 2xx.
 */
export async function apiPost<T>(path: string, body?: unknown): Promise<T> {
  const res = await apiFetch(path, {
    method: 'POST',
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  return parseResponse(res)
}

/**
 * PATCH a JSON body and parse the JSON reply.
 * @param path - Path relative to the API base.
 * @param body - Serialised with `JSON.stringify`; the request has no body when undefined.
 * @returns The parsed response body; a 204 or empty body resolves to undefined.
 * @throws {ApiError} When the response status is not 2xx.
 */
export async function apiPatch<T>(path: string, body?: unknown): Promise<T> {
  const res = await apiFetch(path, {
    method: 'PATCH',
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  return parseResponse(res)
}

/**
 * PUT a JSON body and parse the JSON reply.
 * @param path - Path relative to the API base.
 * @param body - Serialised with `JSON.stringify`; the request has no body when undefined.
 * @returns The parsed response body; a 204 or empty body resolves to undefined.
 * @throws {ApiError} When the response status is not 2xx.
 */
export async function apiPut<T>(path: string, body?: unknown): Promise<T> {
  const res = await apiFetch(path, {
    method: 'PUT',
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  return parseResponse(res)
}

/**
 * DELETE a resource. The reply body is ignored, so a 204 and a 200 both succeed.
 * @param path - Path relative to the API base.
 * @returns Resolves once the server has accepted the deletion.
 * @throws {ApiError} When the response status is not 2xx.
 */
export async function apiDelete(path: string): Promise<void> {
  const res = await apiFetch(path, { method: 'DELETE' })
  if (!res.ok && res.status !== 204) throw new ApiError(res.status, await res.text())
}

/**
 * Error thrown by the `api*` helpers for a non-2xx response. `message` holds
 * the raw response body; the getters decode the server's JSON error envelope.
 * @property status - HTTP status code of the failed response.
 */
export class ApiError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }

  /** The server's `{"error": "..."}` detail, when the body carries one. */
  get detail(): string | null {
    const error = this.json?.error
    return typeof error === 'string' ? error : null
  }

  /** Stable machine-readable error code supplied by the server, when available. */
  get code(): string | null {
    const code = this.json?.code
    return typeof code === 'string' ? code : null
  }

  /** The full parsed JSON body, for errors carrying structured payloads. */
  get json(): Record<string, unknown> | null {
    try {
      const parsed = JSON.parse(this.message)
      return parsed && typeof parsed === 'object' ? parsed : null
    } catch {
      return null
    }
  }
}
