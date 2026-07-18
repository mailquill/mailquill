import { useCallback, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import type { SendStatus } from '@/shared/hooks/useMailNotifications'

/**
 * Coordinate queued and terminal background-send notifications by send ID.
 * @returns Stable callbacks for queue acceptance and SSE delivery results.
 */
export function useSendProgressToasts() {
  const { t } = useTranslation()
  const sendSubjects = useRef(new Map<string, string>())
  const terminalSendIds = useRef(new Set<string>())

  const showSendQueued = useCallback(
    ({ sendId, subject }: { sendId: string; subject: string }) => {
      if (terminalSendIds.current.has(sendId)) return
      sendSubjects.current.set(sendId, subject)
      toast.loading(t('compose.sendInProgress'), {
        id: sendId,
        description: subject ? t('compose.sendInProgressDetail', { subject }) : undefined,
        duration: Number.POSITIVE_INFINITY,
      })
    },
    [t],
  )

  const showSendStatus = useCallback(
    (status: SendStatus) => {
      terminalSendIds.current.add(status.send_id)
      const subject = status.subject ?? sendSubjects.current.get(status.send_id)
      sendSubjects.current.delete(status.send_id)

      if (status.status === 'sent') {
        toast.success(t('compose.sendSucceeded'), {
          id: status.send_id,
          description: subject ? t('compose.sendSucceededDetail', { subject }) : undefined,
          duration: 5000,
        })
        return
      }

      toast.error(t('compose.sendFailed'), {
        id: status.send_id,
        description: subject
          ? t('compose.sendFailedDetail', { subject, error: status.error ?? t('compose.unknownSendError') })
          : status.error ?? t('compose.unknownSendError'),
        duration: Number.POSITIVE_INFINITY,
      })
    },
    [t],
  )

  return { showSendQueued, showSendStatus }
}
