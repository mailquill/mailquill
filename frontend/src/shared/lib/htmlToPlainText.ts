// Break markers collected while walking, resolved into real newlines at the
// end: adjacent breaks must merge into the strongest one instead of stacking up
// into a blank line for every nested element that happens to close there.
const LINE_BREAK = '\u0001'
const PARAGRAPH_BREAK = '\u0002'

// Elements rendered as a block with space around it — the reader expects a
// blank line between them.
const PARAGRAPH_TAGS = new Set([
  'ADDRESS',
  'ARTICLE',
  'ASIDE',
  'BLOCKQUOTE',
  'DL',
  'FIELDSET',
  'FIGURE',
  'FOOTER',
  'FORM',
  'H1',
  'H2',
  'H3',
  'H4',
  'H5',
  'H6',
  'HEADER',
  'HR',
  'MAIN',
  'NAV',
  'OL',
  'P',
  'PRE',
  'SECTION',
  'TABLE',
  'UL',
])

// Elements that merely start a new line: consecutive list items, rows or divs
// belong directly underneath each other.
const LINE_TAGS = new Set(['DD', 'DIV', 'DT', 'LI', 'TR'])

// Cells sit next to each other on one line, so they are separated, not broken.
const CELL_TAGS = new Set(['TD', 'TH'])

const IGNORED_TAGS = new Set(['HEAD', 'NOSCRIPT', 'SCRIPT', 'STYLE', 'TEMPLATE', 'TITLE'])

// Append rendered text, dropping a space that would only double the one the
// previous chunk already ends with. Whitespace inside <pre> is significant and
// therefore never touched, which rules out any later global cleanup pass.
function pushText(out: string[], text: string): void {
  const previous = out[out.length - 1] ?? ''
  const merged = /\s$/.test(previous) ? text.replace(/^ +/, '') : text
  if (merged) out.push(merged)
}

function collect(node: Node, out: string[], preformatted: boolean): void {
  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      const text = child.nodeValue ?? ''
      if (preformatted) {
        out.push(text)
        continue
      }
      // Outside <pre>, every whitespace run renders as a single space.
      pushText(out, text.replace(/\s+/g, ' '))
      continue
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue
    const element = child as Element
    const tag = element.tagName.toUpperCase()
    if (IGNORED_TAGS.has(tag)) continue
    if (tag === 'BR') {
      out.push(LINE_BREAK)
      continue
    }
    const boundary = PARAGRAPH_TAGS.has(tag) ? PARAGRAPH_BREAK : LINE_TAGS.has(tag) ? LINE_BREAK : ''
    if (boundary) out.push(boundary)
    collect(element, out, preformatted || tag === 'PRE')
    if (boundary) out.push(boundary)
    else if (CELL_TAGS.has(tag)) pushText(out, ' ')
  }
}

/**
 * Render an HTML mail body as readable plain text.
 *
 * Stripping the tags with a regex would drop exactly the information that
 * carries the layout: paragraph and `<br>` boundaries become nothing, while the
 * insignificant newlines of the markup survive. Parsing the document instead
 * keeps the line structure, decodes entities, and drops script and style
 * content rather than printing it. The document is inert — `DOMParser` never
 * executes or loads anything.
 *
 * @param html - Original HTML body of the message.
 * @returns The text of the body with its line breaks preserved.
 */
export function htmlToPlainText(html: string): string {
  const doc = new DOMParser().parseFromString(html, 'text/html')
  const out: string[] = []
  collect(doc.body ?? doc, out, false)
  return out
    .join('')
    .replace(/\u00a0/g, ' ')
    .replace(
      // A run of break markers, plus the whitespace around it, is one break —
      // as strong as its strongest marker.
      /[^\S\n]*[\u0001\u0002][\s\u0001\u0002]*/g,
      (run) => (run.includes(PARAGRAPH_BREAK) ? '\n\n' : '\n'),
    )
    .trim()
}
