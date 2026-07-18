import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from '@/shared/lib/utils'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useCreateCalendar, useCaldavDiscover, type CaldavDiscoverResponse } from '@/shared/hooks/useCalendar'
import { useSyncDav } from '@/shared/hooks/useDav'
import { CaldavErrorAlert } from '@/features/caldav-errors'
import { ApiError } from '@/shared/api'
import { TlsCertificateDecisionDialog } from '@/shared/components'

type CalKind = 'local' | 'caldav'

export function AddCalendarDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation()
  const { data: accounts = [] } = useAccounts()
  const createCalendar = useCreateCalendar()
  const {
    data: discoverData,
    error: discoverError,
    isPending: discoverPending,
    mutate: discoverCalendars,
    reset: resetDiscover,
  } = useCaldavDiscover()
  const syncDav = useSyncDav()

  const [name, setName] = useState('')
  const [kind, setKind] = useState<CalKind>('local')
  const [accountId, setAccountId] = useState('')
  const [url, setUrl] = useState('')
  const [tlsDecision, setTlsDecision] = useState<'accept' | 'accept_always'>()
  const [selectedDiscoveredUrl, setSelectedDiscoveredUrl] = useState('')
  const discoveredAccountIdRef = useRef('')

  const selectedAccount = accountId || accounts[0]?.id || ''
  const discoveredCalendars = discoverData?.calendars ?? []
  const selectedDiscoveredCalendar = discoveredCalendars.find((calendar) => calendar.url === selectedDiscoveredUrl)

  const applyDiscoveredCalendars = useCallback(
    (response: CaldavDiscoverResponse) => {
      const calendar = response.calendars[0]
      setSelectedDiscoveredUrl(calendar?.url ?? '')
      setUrl(calendar?.url ?? response.url)
      if (calendar && !name.trim()) setName(calendar.name)
    },
    [name],
  )

  useEffect(() => {
    if (!open || kind !== 'caldav' || !selectedAccount || discoveredAccountIdRef.current === selectedAccount) return

    discoveredAccountIdRef.current = selectedAccount
    discoverCalendars({ accountId: selectedAccount, tlsDecision }, { onSuccess: applyDiscoveredCalendars })
  }, [applyDiscoveredCalendars, discoverCalendars, kind, open, selectedAccount, tlsDecision])

  function reset() {
    setName('')
    setKind('local')
    setAccountId('')
    setUrl('')
    setTlsDecision(undefined)
    discoveredAccountIdRef.current = ''
    setSelectedDiscoveredUrl('')
    resetDiscover()
  }
  function close() {
    reset()
    onClose()
  }

  function runDiscover() {
    if (!selectedAccount) return
    discoveredAccountIdRef.current = selectedAccount
    discoverCalendars({ accountId: selectedAccount, tlsDecision }, { onSuccess: applyDiscoveredCalendars })
  }

  function selectDiscoveredCalendar(nextUrl: string) {
    const previous = selectedDiscoveredCalendar
    const next = discoveredCalendars.find((calendar) => calendar.url === nextUrl)
    setSelectedDiscoveredUrl(nextUrl)
    setUrl(nextUrl)
    if (next && (!name.trim() || name === previous?.name)) setName(next.name)
  }

  function updateManualUrl(nextUrl: string) {
    setSelectedDiscoveredUrl('')
    setUrl(nextUrl)
  }

  async function submit() {
    if (!name.trim()) return
    if (kind === 'local') {
      createCalendar.mutate({ name, color: '#2563EB' }, { onSuccess: close })
      return
    }
    if (!selectedAccount || !url.trim()) return
    await createCalendar.mutateAsync({
      name,
      color: normalizeCalendarColor(selectedDiscoveredCalendar?.color) ?? '#2563EB',
      account_id: selectedAccount,
      dav_url: url.trim(),
    })
    // Pull events for the new collection right away.
    syncDav.mutate(selectedAccount)
    close()
  }

  const canSubmit = name.trim() && (kind === 'local' || (selectedAccount && url.trim()))
  const tlsCertificateError =
    discoverError instanceof ApiError && discoverError.code === 'caldav_tls_certificate_invalid'

  return (
    <>
      <TlsCertificateDecisionDialog
        open={open && tlsCertificateError}
        pending={discoverPending}
        onDecision={(decision) => {
          if (decision === 'deny') {
            resetDiscover()
            return
          }
          setTlsDecision(decision)
          discoverCalendars(
            { accountId: selectedAccount, tlsDecision: decision },
            { onSuccess: applyDiscoveredCalendars },
          )
        }}
      />
      <Dialog open={open && !tlsCertificateError} onClose={close}>
      <DialogContent className="w-[min(480px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>{t('calendar.addCalendar')}</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <Field id="calendar-name" label={t('calendar.name')}>
            <Input id="calendar-name" value={name} onChange={(e) => setName(e.currentTarget.value)} autoFocus />
          </Field>

          <Field label={t('calendar.calendarType')}>
            <div className="inline-flex gap-0.5 rounded-lg bg-secondary p-0.5">
              {(['local', 'caldav'] as CalKind[]).map((k) => (
                <button
                  key={k}
                  type="button"
                  onClick={() => setKind(k)}
                  className={cn(
                    'h-8 flex-1 rounded-md px-3 text-[12.5px] font-semibold transition-colors',
                    kind === k ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-secondary-foreground',
                  )}
                >
                  {t(k === 'local' ? 'calendar.typeLocal' : 'calendar.typeCaldav')}
                </button>
              ))}
            </div>
          </Field>

          {kind === 'caldav' && (
            <>
              <Field id="calendar-account" label={t('calendar.account')}>
                <Select
                  id="calendar-account"
                  value={selectedAccount}
                  onChange={(e) => {
                    setAccountId(e.currentTarget.value)
                    setTlsDecision(undefined)
                    discoveredAccountIdRef.current = ''
                    setSelectedDiscoveredUrl('')
                    setUrl('')
                    resetDiscover()
                  }}
                >
                  {accounts.map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.display_name || a.primary_email}
                    </option>
                  ))}
                </Select>
              </Field>
              {discoveredCalendars.length > 0 && (
                <Field id="calendar-discovered" label={t('calendar.discoveredCalendars')}>
                  <Select
                    id="calendar-discovered"
                    value={selectedDiscoveredUrl}
                    onChange={(e) => selectDiscoveredCalendar(e.currentTarget.value)}
                  >
                    {discoveredCalendars.map((calendar) => (
                      <option key={calendar.url} value={calendar.url}>
                        {calendar.name}
                      </option>
                    ))}
                  </Select>
                </Field>
              )}
              <Field id="calendar-caldav-url" label={t('calendar.caldavUrl')}>
                <div className="flex gap-2">
                  <Input
                    id="calendar-caldav-url"
                    value={url}
                    onChange={(e) => updateManualUrl(e.currentTarget.value)}
                    placeholder="https://…/calendars/user/default/"
                    className="flex-1"
                  />
                  <Button type="button" variant="outline" onClick={runDiscover} disabled={!selectedAccount || discoverPending}>
                    {discoverPending ? t('action.syncing') : t('calendar.discover')}
                  </Button>
                </div>
                {discoverError && !tlsCertificateError && (
                  <div className="mt-3">
                    <CaldavErrorAlert error={discoverError} />
                  </div>
                )}
              </Field>
            </>
          )}
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={close}>
            {t('action.cancel')}
          </Button>
          <Button type="button" onClick={submit} disabled={!canSubmit || createCalendar.isPending}>
            {createCalendar.isPending ? t('settings.saving') : t('calendar.createCalendar')}
          </Button>
        </div>
      </DialogContent>
      </Dialog>
    </>
  )
}

function normalizeCalendarColor(color?: string | null) {
  const match = color?.match(/^#[0-9a-fA-F]{6}/)
  return match?.[0]
}

function Field({ id, label, children }: { id?: string; label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={id} className="text-[12px] font-semibold">
        {label}
      </Label>
      {children}
    </div>
  )
}
