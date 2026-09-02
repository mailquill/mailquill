import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Download, FileText, Image as ImageIcon, Paperclip } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { apiGetBlob } from '@/shared/api'
import { attachmentFileName, previewKind, type PreviewKind } from '@/shared/lib/attachmentPreview'
import { AttachmentPreviewDialog } from '@/widgets/AttachmentPreviewDialog'
import type { MessageAttachment } from '@/shared/types'

interface Preview {
  attachment: MessageAttachment
  kind: PreviewKind
}

/**
 * Lists a message's attachments, opening images and PDFs inline and leaving
 * every other format to a download.
 *
 * @param props - The attachments to list.
 * @returns The attachment strip of a message card.
 */
export function AttachmentList({ attachments }: { attachments: MessageAttachment[] }) {
  const { t, i18n } = useTranslation()
  const [downloadingId, setDownloadingId] = useState<string | null>(null)
  const [failedId, setFailedId] = useState<string | null>(null)
  const [preview, setPreview] = useState<Preview | null>(null)

  const downloadAttachment = async (attachment: MessageAttachment) => {
    setDownloadingId(attachment.id)
    setFailedId(null)
    try {
      const blob = await apiGetBlob(`/attachments/${attachment.id}`)
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = attachmentFileName(attachment, t('mail.unnamedAttachment'))
      document.body.appendChild(link)
      link.click()
      link.remove()
      window.setTimeout(() => URL.revokeObjectURL(url), 1000)
    } catch {
      setFailedId(attachment.id)
    } finally {
      setDownloadingId(null)
    }
  }

  return (
    <section className="mb-3.5 rounded-md border border-border bg-secondary/30 px-3 py-2.5">
      <div className="mb-2 flex items-center gap-1.5 text-[12.5px] font-bold text-secondary-foreground">
        <Paperclip className="size-4" aria-hidden="true" />
        {t('mail.attachments', { count: attachments.length })}
      </div>
      <div className="flex flex-wrap gap-2">
        {attachments.map((attachment) => {
          const filename = attachmentFileName(attachment, t('mail.unnamedAttachment'))
          const kind = previewKind(attachment)
          const failed = failedId === attachment.id
          const Icon = kind === 'image' ? ImageIcon : FileText
          return (
            <div
              key={attachment.id}
              className="group flex min-w-0 max-w-full items-center rounded-md border border-border bg-card shadow-sm"
            >
              <button
                type="button"
                onClick={() => (kind ? setPreview({ attachment, kind }) : downloadAttachment(attachment))}
                disabled={downloadingId === attachment.id}
                title={kind ? t('mail.attachmentPreview', { name: filename }) : undefined}
                className="flex min-w-0 items-center gap-2 rounded-l-md px-2.5 py-2 text-left transition-colors hover:bg-secondary disabled:opacity-60"
              >
                <span className="flex size-8 shrink-0 items-center justify-center rounded bg-secondary text-muted-foreground group-hover:text-foreground">
                  <Icon className="size-4" aria-hidden="true" />
                </span>
                <span className="min-w-0">
                  <span className="block max-w-72 truncate text-[12.5px] font-semibold text-foreground">
                    {filename}
                  </span>
                  <span className={cn('block text-[11px]', failed ? 'text-destructive' : 'text-muted-foreground')}>
                    {failed
                      ? t('mail.attachmentDownloadFailed')
                      : formatAttachmentSize(attachment.size_bytes, i18n.language) || attachment.content_type}
                  </span>
                </span>
              </button>
              <button
                type="button"
                onClick={() => downloadAttachment(attachment)}
                disabled={downloadingId === attachment.id}
                aria-label={t('mail.attachmentDownload')}
                title={t('mail.attachmentDownload')}
                className="self-stretch rounded-r-md border-l border-border px-2.5 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-60"
              >
                <Download className="size-4" aria-hidden="true" />
              </button>
            </div>
          )
        })}
      </div>
      {preview && (
        <AttachmentPreviewDialog
          attachment={preview.attachment}
          kind={preview.kind}
          onClose={() => setPreview(null)}
        />
      )}
    </section>
  )
}

function formatAttachmentSize(size: number | null, locale: string): string | null {
  if (size == null) return null
  const units = ['B', 'KB', 'MB', 'GB']
  let value = size
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: unit === 0 ? 0 : 1 }).format(value)} ${units[unit]}`
}
