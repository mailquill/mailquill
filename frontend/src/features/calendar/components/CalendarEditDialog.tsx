import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, Copy, ExternalLink, ShieldCheck } from 'lucide-react'
import { Button } from '@/shared/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { useUpdateCalendar } from '@/shared/hooks/useCalendar'
import type { Account, Calendar } from '@/shared/types'

interface CalendarEditDialogProps {
  open: boolean
  calendar: Calendar
  account?: Account
  onClose: () => void
}

/**
 * Edits display properties while surfacing the calendar's connection metadata.
 *
 * @param props - Calendar, owning account, visibility state, and close callback.
 * @returns An accessible calendar details dialog.
 */
export function CalendarEditDialog({ open, calendar, account, onClose }: CalendarEditDialogProps) {
  const { t, i18n } = useTranslation()
  const updateCalendar = useUpdateCalendar()
  const [name, setName] = useState(calendar.name)
  const [color, setColor] = useState(calendar.color)
  const [copied, setCopied] = useState(false)

  const remote = Boolean(calendar.dav_url)
  const typeKey =
    calendar.provider_type === 'google'
      ? 'calendar.typeGoogle'
      : calendar.provider_type === 'graph'
        ? 'calendar.typeGraph'
        : remote
          ? 'calendar.typeCaldav'
          : 'calendar.typeLocal'
  const createdAt = new Date(`${calendar.created_at.replace(' ', 'T')}Z`)
  const createdLabel = Number.isNaN(createdAt.getTime())
    ? calendar.created_at
    : createdAt.toLocaleString(i18n.language)

  function save() {
    const trimmedName = name.trim()
    if (!trimmedName) return
    updateCalendar.mutate(
      { id: calendar.id, name: trimmedName, color },
      { onSuccess: onClose },
    )
  }

  async function copyRemoteAddress() {
    if (!calendar.dav_url) return
    await navigator.clipboard.writeText(calendar.dav_url)
    setCopied(true)
  }

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(640px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>{t('calendar.editCalendar')}</DialogTitle>
        </DialogHeader>

        <div className="grid gap-5">
          <section aria-labelledby="calendar-display-heading" className="grid gap-3">
            <h3 id="calendar-display-heading" className="text-[13px] font-bold text-foreground">
              {t('calendar.display')}
            </h3>
            <div className="grid gap-3 sm:grid-cols-[1fr_120px]">
              <div className="grid gap-1.5">
                <Label htmlFor="edit-calendar-name">{t('calendar.name')}</Label>
                <Input
                  id="edit-calendar-name"
                  value={name}
                  onChange={(event) => setName(event.currentTarget.value)}
                  autoFocus
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor="edit-calendar-color">{t('sidebar.calendarColor')}</Label>
                <div className="flex h-9 items-center gap-2 rounded-md border border-input px-2">
                  <input
                    id="edit-calendar-color"
                    type="color"
                    value={color}
                    onChange={(event) => setColor(event.currentTarget.value)}
                    className="size-7 cursor-pointer rounded border-0 bg-transparent p-0"
                  />
                  <span className="font-mono text-[11px] text-muted-foreground">{color.toUpperCase()}</span>
                </div>
              </div>
            </div>
          </section>

          <section aria-labelledby="calendar-connection-heading" className="grid gap-3 border-t border-border pt-4">
            <h3 id="calendar-connection-heading" className="text-[13px] font-bold text-foreground">
              {t('calendar.connectionDetails')}
            </h3>
            <dl className="grid gap-x-4 gap-y-2 text-[12.5px] sm:grid-cols-[150px_1fr]">
              <dt className="text-muted-foreground">{t('calendar.calendarType')}</dt>
              <dd className="font-semibold text-foreground">{t(typeKey)}</dd>
              <dt className="text-muted-foreground">{t('calendar.account')}</dt>
              <dd className="min-w-0 text-foreground">
                {account ? (
                  <>
                    <span className="font-semibold">{account.display_name || account.primary_email}</span>
                    {account.display_name && (
                      <span className="ml-1 text-muted-foreground">({account.primary_email})</span>
                    )}
                  </>
                ) : (
                  t(remote ? 'calendar.connectedAccount' : 'sidebar.localCalendars')
                )}
              </dd>
              <dt className="text-muted-foreground">{t('calendar.defaultCalendar')}</dt>
              <dd className="text-foreground">{t(calendar.is_default ? 'calendar.yes' : 'calendar.no')}</dd>
              <dt className="text-muted-foreground">{t('calendar.createdAt')}</dt>
              <dd className="text-foreground">{createdLabel}</dd>
            </dl>

            {calendar.dav_url && (
              <div className="grid gap-1.5">
                <Label htmlFor="calendar-remote-address">{t('calendar.remoteAddress')}</Label>
                <div className="flex gap-2">
                  <Input
                    id="calendar-remote-address"
                    value={calendar.dav_url}
                    readOnly
                    className="min-w-0 font-mono text-[12px]"
                  />
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    onClick={() => void copyRemoteAddress()}
                    aria-label={t('calendar.copyRemoteAddress')}
                    title={t('calendar.copyRemoteAddress')}
                  >
                    {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
                  </Button>
                  {/^https?:\/\//i.test(calendar.dav_url) && (
                    <a
                      href={calendar.dav_url}
                      target="_blank"
                      rel="noopener noreferrer"
                      aria-label={t('calendar.openRemoteAddress')}
                      title={t('calendar.openRemoteAddress')}
                      className="inline-flex size-9 shrink-0 items-center justify-center rounded-md border border-input bg-background shadow-sm transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                    >
                      <ExternalLink className="size-4" />
                    </a>
                  )}
                </div>
                {account?.caldav_accept_invalid_tls && (
                  <p className="text-[12px] font-medium text-destructive">
                    {t('calendar.invalidTlsActive')}
                  </p>
                )}
              </div>
            )}
          </section>

          <section aria-labelledby="calendar-permissions-heading" className="grid gap-2 border-t border-border pt-4">
            <h3 id="calendar-permissions-heading" className="flex items-center gap-2 text-[13px] font-bold text-foreground">
              <ShieldCheck className="size-4 text-primary" aria-hidden="true" />
              {t('calendar.permissions')}
            </h3>
            <div className="rounded-md border border-border bg-secondary/40 px-3 py-2.5">
              <p className="text-[12.5px] font-semibold text-foreground">
                {t(remote ? 'calendar.remoteReadWrite' : 'calendar.localFullAccess')}
              </p>
              <p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
                {t(remote ? 'calendar.remotePermissionsHint' : 'calendar.localPermissionsHint')}
              </p>
            </div>
          </section>

          <details className="border-t border-border pt-4 text-[12px]">
            <summary className="cursor-pointer font-semibold text-muted-foreground">
              {t('calendar.technicalDetails')}
            </summary>
            <dl className="mt-2 grid gap-x-4 gap-y-2 sm:grid-cols-[150px_1fr]">
              <dt className="text-muted-foreground">{t('calendar.calendarId')}</dt>
              <dd className="break-all font-mono text-foreground">{calendar.id}</dd>
              {calendar.account_id && (
                <>
                  <dt className="text-muted-foreground">{t('calendar.accountId')}</dt>
                  <dd className="break-all font-mono text-foreground">{calendar.account_id}</dd>
                </>
              )}
            </dl>
          </details>
        </div>

        {updateCalendar.isError && (
          <p className="mt-4 text-[12.5px] text-destructive" role="alert">
            {t('calendar.updateFailed')}
          </p>
        )}
        <div className="mt-5 flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onClose}>
            {t('action.cancel')}
          </Button>
          <Button type="button" onClick={save} disabled={!name.trim() || updateCalendar.isPending}>
            {updateCalendar.isPending ? t('settings.saving') : t('action.save')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
