import type { MessageAttachment } from '@/shared/types'

/** Attachment kinds the reading pane can render without downloading the file. */
export type PreviewKind = 'image' | 'pdf'

// Raster formats every target browser decodes natively. SVG is deliberately
// absent: it is a scriptable document, and rendering one from a blob URL would
// execute mail-controlled markup on our own origin.
const IMAGE_TYPES = new Set([
  'image/png',
  'image/jpeg',
  'image/jpg',
  'image/gif',
  'image/webp',
  'image/avif',
  'image/bmp',
])

const IMAGE_EXTENSIONS: Record<string, string> = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  avif: 'image/avif',
  bmp: 'image/bmp',
}

const PDF_TYPE = 'application/pdf'

function extensionOf(filename: string | null): string {
  const name = filename?.split(/[\\/]/).pop() ?? ''
  const dot = name.lastIndexOf('.')
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : ''
}

/**
 * Decide how an attachment can be shown inline.
 *
 * The declared content type wins, but senders routinely ship
 * `application/octet-stream`, so the filename extension is consulted as a
 * fallback.
 *
 * @param attachment - Attachment metadata from the message detail.
 * @returns The renderable kind, or `null` when only downloading makes sense.
 */
export function previewKind(attachment: MessageAttachment): PreviewKind | null {
  const type = attachment.content_type.split(';')[0].trim().toLowerCase()
  if (type === PDF_TYPE) return 'pdf'
  if (IMAGE_TYPES.has(type)) return 'image'
  if (type && type !== 'application/octet-stream' && type !== 'binary/octet-stream') return null
  const extension = extensionOf(attachment.filename)
  if (extension === 'pdf') return 'pdf'
  return IMAGE_EXTENSIONS[extension] ? 'image' : null
}

/**
 * MIME type the preview blob URL is built with.
 *
 * Never derived from the message: a blob URL typed `text/html` would run
 * mail-controlled script on our origin, so the type is pinned to the format the
 * preview actually renders.
 *
 * @param attachment - Attachment metadata from the message detail.
 * @param kind - The preview kind {@link previewKind} resolved for it.
 * @returns A MIME type that can only be decoded as an image or a PDF.
 */
export function previewMimeType(attachment: MessageAttachment, kind: PreviewKind): string {
  if (kind === 'pdf') return PDF_TYPE
  const type = attachment.content_type.split(';')[0].trim().toLowerCase()
  if (IMAGE_TYPES.has(type)) return type === 'image/jpg' ? 'image/jpeg' : type
  return IMAGE_EXTENSIONS[extensionOf(attachment.filename)] ?? 'image/png'
}

/**
 * Filename to save an attachment under, with any path segments stripped.
 *
 * @param attachment - Attachment metadata from the message detail.
 * @param fallback - Localized name for attachments without a filename.
 * @returns A plain file name safe to put into a download link.
 */
export function attachmentFileName(attachment: MessageAttachment, fallback: string): string {
  return attachment.filename?.split(/[\\/]/).pop() || fallback
}
