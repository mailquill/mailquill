import { afterEach, describe, expect, it, vi } from 'vitest'
import { apiGet, ensureFreshAccessToken, setAccessToken } from './client'

function tokenExpiringAt(epochSeconds: number): string {
  const payload = btoa(JSON.stringify({ exp: epochSeconds }))
    .replaceAll('+', '-')
    .replaceAll('/', '_')
    .replaceAll('=', '')
  return `header.${payload}.signature`
}

afterEach(() => {
  setAccessToken(null)
  vi.unstubAllGlobals()
})

describe('access-token renewal', () => {
  it('refreshes an expired token before API and SSE consumers use it', async () => {
    const expired = tokenExpiringAt(Math.floor(Date.now() / 1000) - 1)
    const fresh = tokenExpiringAt(Math.floor(Date.now() / 1000) + 900)
    setAccessToken(expired)
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ access_token: fresh }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(ensureFreshAccessToken()).resolves.toBe(fresh)
    await expect(apiGet<{ ok: boolean }>('/accounts')).resolves.toEqual({ ok: true })

    expect(fetchMock).toHaveBeenNthCalledWith(1, '/api/auth/refresh', {
      method: 'POST',
      credentials: 'include',
    })
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      '/api/accounts',
      expect.objectContaining({
        credentials: 'include',
        headers: expect.objectContaining({ Authorization: `Bearer ${fresh}` }),
      }),
    )
  })

  it('deduplicates concurrent proactive refresh requests', async () => {
    const expired = tokenExpiringAt(Math.floor(Date.now() / 1000) - 1)
    const fresh = tokenExpiringAt(Math.floor(Date.now() / 1000) + 900)
    setAccessToken(expired)
    let resolveRefresh: ((response: Response) => void) | undefined
    const fetchMock = vi.fn(() => new Promise<Response>((resolve) => {
      resolveRefresh = resolve
    }))
    vi.stubGlobal('fetch', fetchMock)

    const first = ensureFreshAccessToken()
    const second = ensureFreshAccessToken()
    expect(fetchMock).toHaveBeenCalledTimes(1)
    resolveRefresh?.(new Response(JSON.stringify({ access_token: fresh }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }))

    await expect(Promise.all([first, second])).resolves.toEqual([fresh, fresh])
  })
})
