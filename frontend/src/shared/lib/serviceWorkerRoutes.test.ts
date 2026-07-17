import { describe, expect, it } from 'vitest'
import { shouldCacheStaticRequest } from './serviceWorkerRoutes'

describe('shouldCacheStaticRequest', () => {
  const appOrigin = 'https://mail.example.test'

  it('caches same-origin application images', () => {
    expect(
      shouldCacheStaticRequest(
        { destination: 'image' },
        new URL('/icons/icon-192.png', appOrigin),
        appOrigin,
      ),
    ).toBe(true)
  })

  it('does not intercept cross-origin email images', () => {
    expect(
      shouldCacheStaticRequest(
        { destination: 'image' },
        new URL('https://www.gstatic.com/images/branding/googlelogo.png'),
        appOrigin,
      ),
    ).toBe(false)
  })

  it('does not cache same-origin document requests as static assets', () => {
    expect(
      shouldCacheStaticRequest({ destination: 'document' }, new URL('/mail/unified', appOrigin), appOrigin),
    ).toBe(false)
  })

  it('does not classify streaming API requests as static assets', () => {
    expect(
      shouldCacheStaticRequest(
        { destination: '' },
        new URL('/api/events?token=redacted', appOrigin),
        appOrigin,
      ),
    ).toBe(false)
  })
})
