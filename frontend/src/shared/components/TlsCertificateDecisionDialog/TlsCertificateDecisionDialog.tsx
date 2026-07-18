import { ShieldAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/shared/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/shared/components/ui/dialog'
import type { TlsDecision } from './tlsCertificateDecision'

interface TlsCertificateDecisionDialogProps {
  open: boolean
  host?: string
  port?: number
  fingerprint?: string
  pending?: boolean
  onDecision: (decision: TlsDecision) => void
}

const formatFingerprint = (hex: string) => hex.toUpperCase().match(/.{2}/g)?.join(':') ?? hex

/**
 * Ask for an explicit response before retrying a failed TLS connection.
 *
 * @param props - Dialog state, optional certificate details and decision callback.
 * @returns An accessible modal with one-time, persistent and deny actions.
 */
export function TlsCertificateDecisionDialog({
  open,
  host,
  port,
  fingerprint,
  pending = false,
  onDecision,
}: TlsCertificateDecisionDialogProps) {
  const { t } = useTranslation()
  const endpoint = host ? `${host}${port ? `:${port}` : ''}` : undefined

  return (
    <Dialog open={open} onClose={() => onDecision('deny')}>
      <DialogContent className="w-[min(560px,calc(100vw-2rem))] max-w-none border border-destructive/40">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ShieldAlert className="text-destructive" aria-hidden="true" />
            {t('tlsDecision.title')}
          </DialogTitle>
        </DialogHeader>
        <div role="alert" className="flex flex-col gap-3 text-sm">
          <p>{t('tlsDecision.description', { endpoint: endpoint ?? t('tlsDecision.unknownEndpoint') })}</p>
          <p className="text-muted-foreground">{t('tlsDecision.warning')}</p>
          {fingerprint ? (
            <div className="rounded-md border border-border bg-muted/30 p-3">
              <div className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
                {t('tlsDecision.fingerprint')}
              </div>
              <div className="mt-1 break-all font-mono text-xs">{formatFingerprint(fingerprint)}</div>
            </div>
          ) : null}
        </div>
        <div className="mt-5 flex flex-wrap justify-end gap-2">
          <Button type="button" variant="outline" disabled={pending} onClick={() => onDecision('accept')}>
            {t('tlsDecision.accept')}
          </Button>
          <Button type="button" variant="destructive" disabled={pending} onClick={() => onDecision('accept_always')}>
            {t('tlsDecision.acceptAlways')}
          </Button>
          <Button type="button" disabled={pending} onClick={() => onDecision('deny')} autoFocus>
            {t('tlsDecision.deny')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
