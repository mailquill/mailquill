import { describe, expect, it } from 'vitest'
import workerSource from '../../sw.js?raw'

describe('service worker policy', () => {
  it('never runtime-caches API responses or streams', () => {
    expect(workerSource).not.toContain('NetworkFirst')
    expect(workerSource).not.toContain("url.pathname.startsWith('/api/')")
    expect(workerSource).not.toContain("cacheName: 'mailquill-api'")
  })

  it('restricts static runtime caching to the service-worker origin', () => {
    expect(workerSource).toContain('shouldCacheStaticRequest(request, url, self.location.origin)')
  })

  it('removes the obsolete authenticated API cache during activation', () => {
    expect(workerSource).toContain("caches.delete('mailquill-api')")
  })
})
