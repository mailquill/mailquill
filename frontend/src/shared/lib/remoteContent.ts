/**
 * Strips remote (http/https/protocol-relative) resource references from email
 * HTML so the sandboxed reader never fetches tracking pixels or images unless
 * the user opted in. Inline content (`cid:`, `data:`) is left untouched.
 */

const REMOTE_URL = /^\s*(?:https?:)?\/\//i
const CSS_REMOTE_URL = /url\(\s*(['"]?)\s*(?:https?:)?\/\/[^)'"]*\1\s*\)/gi

export interface RemoteBlockResult {
  html: string
  blocked: boolean
}

export function blockRemoteContent(html: string): RemoteBlockResult {
  const doc = new DOMParser().parseFromString(html, 'text/html')
  let blocked = false

  const stripAttr = (el: Element, attr: string) => {
    const value = el.getAttribute(attr)
    if (value && REMOTE_URL.test(value)) {
      el.removeAttribute(attr)
      blocked = true
    }
  }

  doc.querySelectorAll('img, source, input, video, audio, embed, iframe, object').forEach((el) => {
    stripAttr(el, 'src')
    stripAttr(el, 'poster')
    stripAttr(el, 'data')
    const srcset = el.getAttribute('srcset')
    if (srcset && /(?:https?:)?\/\//.test(srcset)) {
      el.removeAttribute('srcset')
      blocked = true
    }
  })

  // Legacy HTML-email markup: <body background>, <td background>, …
  doc.querySelectorAll('[background]').forEach((el) => stripAttr(el, 'background'))

  const stripCss = (css: string) => {
    const replaced = css.replace(CSS_REMOTE_URL, 'url()')
    if (replaced !== css) blocked = true
    return replaced
  }

  doc.querySelectorAll('[style]').forEach((el) => {
    const style = el.getAttribute('style')
    if (style) el.setAttribute('style', stripCss(style))
  })
  doc.querySelectorAll('style').forEach((el) => {
    const css = el.textContent
    if (css) el.textContent = stripCss(css)
  })

  return { html: `<!DOCTYPE html>${doc.documentElement.outerHTML}`, blocked }
}
