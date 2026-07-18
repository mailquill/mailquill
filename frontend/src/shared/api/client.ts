const BASE = '/api'

let accessToken: string | null = null
let refreshSubscriber: ((token: string | null) => void) | null = null

export function setAccessToken(token: string | null) {
  accessToken = token
}

export function getAccessToken() {
  return accessToken
}

export function setRefreshSubscriber(subscriber: (token: string | null) => void) {
  refreshSubscriber = subscriber
}

let refreshInFlight: Promise<string | null> | null = null

const ACCESS_TOKEN_REFRESH_SKEW_SECONDS = 60

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

async function doRefresh(): Promise<string | null> {
  try {
    const res = await fetch(`${BASE}/auth/refresh`, {
      method: 'POST',
      credentials: 'include',
    })
    if (!res.ok) {
      accessToken = null
      refreshSubscriber?.(null)
      return null
    }
    const data = await res.json()
    accessToken = data.access_token
    refreshSubscriber?.(accessToken)
    return accessToken
  } catch {
    accessToken = null
    refreshSubscriber?.(null)
    return null
  }
}

export async function apiFetch(
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

export async function apiGet<T>(path: string): Promise<T> {
  return parseResponse(await apiFetch(path))
}

/// Binary fetch (attachments, inline images) — bypasses the JSON parsing.
export async function apiGetBlob(path: string): Promise<Blob> {
  const res = await apiFetch(path)
  if (!res.ok) throw new ApiError(res.status, await res.text())
  return res.blob()
}

export async function apiPost<T>(path: string, body?: unknown): Promise<T> {
  const res = await apiFetch(path, {
    method: 'POST',
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  return parseResponse(res)
}

export async function apiPatch<T>(path: string, body?: unknown): Promise<T> {
  const res = await apiFetch(path, {
    method: 'PATCH',
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  return parseResponse(res)
}

export async function apiPut<T>(path: string, body?: unknown): Promise<T> {
  const res = await apiFetch(path, {
    method: 'PUT',
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  return parseResponse(res)
}

export async function apiDelete(path: string): Promise<void> {
  const res = await apiFetch(path, { method: 'DELETE' })
  if (!res.ok && res.status !== 204) throw new ApiError(res.status, await res.text())
}

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
