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

export async function refreshAccessToken(): Promise<string | null> {
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
  if (accessToken) {
    headers['Authorization'] = `Bearer ${accessToken}`
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
