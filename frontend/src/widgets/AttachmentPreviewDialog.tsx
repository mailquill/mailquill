import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Download, X } from 'lucide-react'
import { apiGetBlob } from '@/shared/api'
import { Button } from '@/shared/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { attachmentFileName, previewMimeType, type PreviewKind } from '@/shared/lib/attachmentPreview'
import type { MessageAttachment } from '@/shared/types'

interface AttachmentPreviewDialogProps {
  attachment: MessageAttachment
  kind: PreviewKind
  onClose: () => void
}

/**
 * Shows an image or PDF attachment inline instead of forcing a download first.
 *
 * The bytes need the authenticated API call, so they are fetched once and
 * published as an object URL that both the viewer and the download button use.
 *
 * @param props - The attachment, its resolved preview kind, and a close handler.
 * @returns A modal preview of the attachment.
 */
export function AttachmentPreviewDialog({ attachment, kind, onClose }: AttachmentPreviewDialogProps) {
  const { t } = useTranslation()
  const [url, setUrl] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)
  const fileName = attachmentFileName(attachment, t('mail.unnamedAttachment'))

  useEffect(() => {
    let objectUrl: string | null = null
    let cancelled = false
    setUrl(null)
    setFailed(false)
    apiGetBlob(`/attachments/${attachment.id}`)
      .then((blob) => {
        if (cancelled) return
        // Re-type the blob: the URL must not be able to resolve to anything the
        // browser would parse as a document on our origin.
        objectUrl = URL.createObjectURL(new Blob([blob], { type: previewMimeType(attachment, kind) }))
        setUrl(objectUrl)
      })
      .catch(() => {
        if (!cancelled) setFailed(true)
      })
    return () => {
      cancelled = true
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [attachment, kind])

  return (
    <Dialog open onClose={onClose}>
      <DialogContent className="flex h-[85vh] w-[min(95vw,1100px)] max-w-none flex-col p-0">
        <DialogHeader className="mb-0 flex flex-row items-center justify-between gap-3 border-b border-border px-4 py-3">
          <DialogTitle className="min-w-0 truncate text-[15px]">{fileName}</DialogTitle>
          <div className="flex shrink-0 items-center gap-2">
            {url && (
              <a
                href={url}
                download={fileName}
                className="inline-flex h-8 items-center gap-1.5 rounded-md border border-input bg-background px-3 text-xs font-medium shadow-sm transition-colors hover:bg-accent hover:text-accent-foreground"
              >
                <Download className="size-4" aria-hidden="true" />
                {t('mail.attachmentDownload')}
              </a>
            )}
            <Button variant="ghost" size="sm" onClick={onClose} aria-label={t('action.close')}>
              <X className="size-4" aria-hidden="true" />
            </Button>
          </div>
        </DialogHeader>
        <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto bg-secondary/40 p-3">
          {failed ? (
            <p className="text-[13px] text-destructive">{t('mail.attachmentPreviewFailed')}</p>
          ) : !url ? (
            <p className="text-[13px] text-muted-foreground">{t('mail.attachmentPreviewLoading')}</p>
          ) : kind === 'image' ? (
            <img src={url} alt={fileName} className="max-h-full max-w-full object-contain" />
          ) : (
            // The blob is pinned to application/pdf, so this always opens the
            // browser's own PDF viewer, which runs the document isolated from
            // this page. A `sandbox` attribute would disable that viewer.
            <iframe src={url} title={fileName} className="size-full rounded border border-border bg-card" />
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
