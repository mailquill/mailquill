const INLINE_IMAGE_PLACEHOLDER =
  'data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs='

/**
 * Make untrusted email HTML inert and resolve `cid:` resources before iframe parsing.
 *
 * @param html - Original HTML body extracted from the message.
 * @param cidUrls - Loaded Content-ID to blob-URL mappings.
 * @returns A complete, script-free HTML document safe to pass to `srcdoc`.
 */
export function prepareEmailHtml(html: string, cidUrls: Record<string, string>): string {
  const urls = new Map(Object.entries(cidUrls).map(([cid, url]) => [normalizeCid(cid), url]))
  const withResolvedCids = html.replace(/cid:([^"'\s)<>]+)/gi, (_match, cid: string) => {
    return urls.get(normalizeCid(cid)) ?? INLINE_IMAGE_PLACEHOLDER
  })
  const doc = new DOMParser().parseFromString(withResolvedCids, 'text/html')

  // Nested browsing/plugin contexts can hide executable markup inside attributes
  // such as iframe `srcdoc`. Remove the complete container before the browser
  // gets a chance to parse that secondary document.
  doc.querySelectorAll('script, iframe, frame, object, embed, applet').forEach((element) => {
    element.remove()
  })
  doc.querySelectorAll<HTMLElement>('*').forEach((element) => {
    for (const attribute of Array.from(element.attributes)) {
      if (/^on/i.test(attribute.name)) element.removeAttribute(attribute.name)
      if (/^javascript:/i.test(attribute.value.trim())) element.removeAttribute(attribute.name)
    }
  })

  // Refresh directives are not useful inside an inert mail viewer. Removing
  // them also avoids browser diagnostics from malformed legacy content values.
  doc.querySelectorAll('meta[http-equiv]').forEach((meta) => meta.remove())
  doc.querySelectorAll<HTMLMetaElement>('meta[name="viewport"]').forEach((meta) => {
    meta.content = meta.content.replace(/;\s*/g, ', ')
  })

  return `<!DOCTYPE html>${doc.documentElement.outerHTML}`
}

function normalizeCid(cid: string): string {
  let decoded = cid
  try {
    decoded = decodeURIComponent(cid)
  } catch {
    // Preserve malformed percent escapes for a deterministic lookup miss.
  }
  return decoded.trim().replace(/^<|>$/g, '').toLowerCase()
}
