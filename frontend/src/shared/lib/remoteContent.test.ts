import { describe, expect, it } from 'vitest'
import { blockRemoteContent, proxyRemoteContent } from './remoteContent'

describe('blockRemoteContent', () => {
  it('drops remote <link> elements along with remote images', () => {
    const html =
      '<html><head><link rel="stylesheet" href="https://track.example/a.css"><link rel="icon" href="data:,x"></head>' +
      '<body><img src="https://track.example/p.gif"><img src="cid:logo"></body></html>'
    const { html: out, blocked } = blockRemoteContent(html)
    expect(blocked).toBe(true)
    expect(out).not.toContain('track.example')
    expect(out).toContain('href="data:,x"')
    expect(out).toContain('src="cid:logo"')
  })
})

describe('proxyRemoteContent', () => {
  it('routes images through the proxy and removes remote links it cannot proxy', () => {
    const html =
      '<html><head><link rel="stylesheet" href="//cdn.example/a.css"></head>' +
      '<body><img src="https://img.example/p.png"></body></html>'
    const out = proxyRemoteContent(html, 'tok')
    expect(out).not.toContain('cdn.example')
    expect(out).toContain('/api/remote-content/image?token=tok&amp;url=https%3A%2F%2Fimg.example%2Fp.png')
  })
})
