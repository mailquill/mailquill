import { describe, expect, it } from 'vitest'
import { prepareEmailHtml } from './emailHtml'

describe('prepareEmailHtml', () => {
  it('replaces unresolved and resolved cid resources before iframe parsing', () => {
    const html = prepareEmailHtml(
      '<html><body><img src="cid:Logo%40Example"><img src="cid:missing"></body></html>',
      { 'logo@example': 'blob:resolved-logo' },
    )
    const doc = new DOMParser().parseFromString(html, 'text/html')
    const sources = Array.from(doc.images).map((image) => image.src)

    expect(sources[0]).toBe('blob:resolved-logo')
    expect(sources[1]).toMatch(/^data:image\/gif;base64,/)
    expect(html).not.toContain('cid:')
  })

  it('removes executable content and normalizes legacy viewport separators', () => {
    const html = prepareEmailHtml(
      '<html><head><meta http-equiv="refresh" content="0; url=https://example.test"><meta name="viewport" content="width=device-width; initial-scale=1"></head><body onload="run()"><script>run()</script><iframe srcdoc="&lt;script&gt;run()&lt;/script&gt;"></iframe><object data="data:text/html,&lt;script&gt;run()&lt;/script&gt;"></object><embed src="data:text/html,&lt;script&gt;run()&lt;/script&gt;"><a href="javascript:run()">link</a></body></html>',
      {},
    )
    const doc = new DOMParser().parseFromString(html, 'text/html')

    expect(doc.querySelector('script')).toBeNull()
    expect(doc.querySelector('iframe, frame, object, embed, applet')).toBeNull()
    expect(doc.querySelector('meta[http-equiv]')).toBeNull()
    expect(doc.body.hasAttribute('onload')).toBe(false)
    expect(doc.querySelector('a')?.hasAttribute('href')).toBe(false)
    expect(doc.querySelector<HTMLMetaElement>('meta[name="viewport"]')?.content).toBe(
      'width=device-width, initial-scale=1',
    )
  })
})
