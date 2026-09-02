import { describe, expect, it } from 'vitest'
import { attachmentFileName, previewKind, previewMimeType } from './attachmentPreview'
import type { MessageAttachment } from '@/shared/types'

function attachment(content_type: string, filename: string | null = null): MessageAttachment {
  return { id: 'a1', filename, content_type, content_id: null, size_bytes: 1024 }
}

describe('previewKind', () => {
  it('renders declared PDFs and images inline', () => {
    expect(previewKind(attachment('application/pdf'))).toBe('pdf')
    expect(previewKind(attachment('image/png'))).toBe('image')
    expect(previewKind(attachment('IMAGE/JPEG; name=x.jpg'))).toBe('image')
  })

  it('falls back to the extension for undeclared types', () => {
    expect(previewKind(attachment('application/octet-stream', 'invoice.PDF'))).toBe('pdf')
    expect(previewKind(attachment('application/octet-stream', 'scan.jpeg'))).toBe('image')
    expect(previewKind(attachment('application/octet-stream', 'archive.zip'))).toBeNull()
  })

  it('never previews scriptable or unknown formats', () => {
    expect(previewKind(attachment('image/svg+xml', 'logo.svg'))).toBeNull()
    expect(previewKind(attachment('text/html', 'page.html'))).toBeNull()
    // A misdeclared extension must not override an explicit content type.
    expect(previewKind(attachment('text/html', 'page.png'))).toBeNull()
  })
})

describe('previewMimeType', () => {
  it('pins the blob type to the format that gets rendered', () => {
    expect(previewMimeType(attachment('application/pdf'), 'pdf')).toBe('application/pdf')
    expect(previewMimeType(attachment('application/octet-stream', 'x.pdf'), 'pdf')).toBe('application/pdf')
    expect(previewMimeType(attachment('image/webp'), 'image')).toBe('image/webp')
    expect(previewMimeType(attachment('image/jpg'), 'image')).toBe('image/jpeg')
    expect(previewMimeType(attachment('application/octet-stream', 'x.gif'), 'image')).toBe('image/gif')
  })
})

describe('attachmentFileName', () => {
  it('strips path segments and falls back for unnamed attachments', () => {
    expect(attachmentFileName(attachment('application/pdf', 'C:\\temp\\bill.pdf'), 'file')).toBe('bill.pdf')
    expect(attachmentFileName(attachment('application/pdf', null), 'file')).toBe('file')
  })
})
