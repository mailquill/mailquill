import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSearchParams } from 'react-router-dom'
import { ChevronLeft, ChevronRight, ExternalLink, KeyRound, Plus, Trash2 } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { apiGet } from '@/shared/api'
import { startOAuthRedirect } from '@/shared/lib/oauth'
import { DavSyncButton } from '@/widgets/DavSyncButton'
import { RecipientChips } from '@/features/compose'
import { CaldavErrorAlert } from '@/features/caldav-errors'
import { useAccounts } from '@/shared/hooks/useAccounts'
import {
  useCalendarAccounts,
  useCalendars,
  useCreateCalendarAccount,
  useCreateCalendar,
  useCreateEvent,
  useDeleteCalendarAccount,
  useUpdateEvent,
  useDeleteEvent,
  useEvents,
} from '@/shared/hooks/useCalendar'
import { useModuleNav } from '@/shared/hooks/useModuleNav'
import type { CalendarEvent } from '@/shared/types'

type ViewMode = 'month' | 'week' | 'day' | 'agenda'

const HOUR_PX = 48
const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December']

function startOfDay(d: Date) {
  const x = new Date(d)
  x.setHours(0, 0, 0, 0)
  return x
}
function addDays(d: Date, n: number) {
  const x = new Date(d)
  x.setDate(x.getDate() + n)
  return x
}
function startOfWeekMon(d: Date) {
  const x = startOfDay(d)
  const day = (x.getDay() + 6) % 7 // Mon=0
  return addDays(x, -day)
}
function sameDay(a: Date, b: Date) {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()
}
function iso(d: Date) {
  return d.toISOString()
}

export function CalendarPage() {
  const { t } = useTranslation()
  const [view, setView] = useState<ViewMode>('month')
  const [cursor, setCursor] = useState(() => new Date())
  const { hiddenCalendars } = useModuleNav()
  const [searchParams, setSearchParams] = useSearchParams()
  const addOpen = searchParams.get('new') === '1'
  const [editing, setEditing] = useState<CalendarEvent | null>(null)
  const closeDialog = () => {
    setEditing(null)
    if (addOpen) setSearchParams({}, { replace: true })
  }

  const range = useMemo(() => visibleRange(view, cursor), [view, cursor])
  const { data: events = [] } = useEvents(iso(range.from), iso(range.to))
  const visibleEvents = events.filter((e) => !hiddenCalendars.includes(e.calendar_id))

  function shift(dir: number) {
    if (view === 'month') setCursor((c) => new Date(c.getFullYear(), c.getMonth() + dir, 1))
    else if (view === 'week') setCursor((c) => addDays(c, dir * 7))
    else setCursor((c) => addDays(c, dir))
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <header className="flex shrink-0 items-center gap-3 border-b border-border bg-card px-5 py-3">
        <Button variant="outline" size="sm" onClick={() => setCursor(new Date())}>
          {t('action.today')}
        </Button>
        <div className="flex items-center">
          <button onClick={() => shift(-1)} className="flex size-8 items-center justify-center rounded-md text-secondary-foreground hover:bg-secondary">
            <ChevronLeft className="size-4" />
          </button>
          <button onClick={() => shift(1)} className="flex size-8 items-center justify-center rounded-md text-secondary-foreground hover:bg-secondary">
            <ChevronRight className="size-4" />
          </button>
        </div>
        <h1 className="text-[17px] font-bold tracking-tight">{titleFor(view, cursor)}</h1>
        <div className="ml-auto flex items-center gap-2">
          <div className="inline-flex gap-0.5 rounded-lg bg-secondary p-0.5">
            {(['month', 'week', 'day', 'agenda'] as ViewMode[]).map((m) => (
              <button
                key={m}
                onClick={() => setView(m)}
                className={cn(
                  'h-7 rounded-md px-3 text-[12.5px] font-semibold transition-colors',
                  view === m ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-secondary-foreground',
                )}
              >
                {t(`calendar.${m}`)}
              </button>
            ))}
          </div>
          <DavSyncButton />
          <Button size="sm" onClick={() => setSearchParams({ new: '1' })}>
            <Plus className="size-4" />
            {t('calendar.newEvent')}
          </Button>
        </div>
      </header>
      <CalendarAccountsStrip />

      <div className="min-h-0 flex-1 overflow-hidden">
        {view === 'month' && <MonthView cursor={cursor} events={visibleEvents} onSelectEvent={setEditing} />}
        {view === 'agenda' && <AgendaView events={visibleEvents} onSelectEvent={setEditing} />}
        {view !== 'month' && view !== 'agenda' && (
          <TimeGrid view={view} cursor={cursor} events={visibleEvents} onSelectEvent={setEditing} />
        )}
      </div>

      <EventDialog open={addOpen || editing !== null} onClose={closeDialog} defaultDate={cursor} event={editing} />
    </div>
  )
}

function CalendarAccountsStrip() {
  const { t } = useTranslation()
  const { data: accounts = [] } = useCalendarAccounts()
  const { data: emailAccounts = [] } = useAccounts()
  const createAccount = useCreateCalendarAccount()
  const deleteAccount = useDeleteCalendarAccount()
  const [open, setOpen] = useState(false)
  const [form, setForm] = useState({
    display_name: '',
    type: 'caldav' as 'caldav' | 'graph' | 'google' | 'openxchange',
    base_url: '',
    auth_scheme: 'basic' as 'basic' | 'oauth2',
    username: '',
    password: '',
    access_token: '',
    email_account_id: '',
    accept_invalid_tls: false,
  })
  const gmailAccounts = emailAccounts.filter((account) => account.provider_kind === 'gmail_api')

  function set<K extends keyof typeof form>(key: K, value: (typeof form)[K]) {
    createAccount.reset()
    setForm((f) => ({ ...f, [key]: value }))
  }

  function submit() {
    createAccount.mutate(
      {
        display_name: form.display_name || form.type,
        type: form.type,
        base_url: form.base_url || null,
        auth_scheme: form.auth_scheme,
        username: form.username || null,
        password: form.password || null,
        access_token: form.access_token || null,
        accept_invalid_tls: form.accept_invalid_tls,
      },
      { onSuccess: () => setOpen(false) },
    )
  }

  return (
    <div className="shrink-0 border-b border-border bg-background px-5 py-2.5">
      <div className="flex flex-wrap items-center gap-2">
        {accounts.map((account) => (
          <span key={account.id} className="inline-flex h-8 items-center gap-2 rounded-md border border-border bg-card px-2.5 text-[12px] font-semibold">
            {account.display_name}
            <span className={cn('size-2 rounded-full', account.sync_status === 'error' ? 'bg-red-500' : 'bg-[#16a34a]')} />
            <button title={t('action.delete')} onClick={() => deleteAccount.mutate(account.id)} className="text-muted-foreground hover:text-destructive">
              <Trash2 className="size-3.5" />
            </button>
          </span>
        ))}
        <Button variant="outline" size="sm" onClick={() => setOpen((v) => !v)}>
          <Plus className="size-4" />
          {t('calendar.connectAccount')}
        </Button>
      </div>
      {open && (
        <div className="mt-3 grid gap-2 md:grid-cols-[160px_160px_minmax(180px,1fr)_140px_140px_140px_auto]">
          {form.type !== 'google' && (
            <Input placeholder={t('settings.displayName')} value={form.display_name} onChange={(e) => set('display_name', e.currentTarget.value)} />
          )}
          <Select
            value={form.type}
            onChange={(e) => {
              const type = e.currentTarget.value as typeof form.type
              set('type', type)
              if (type === 'google') set('auth_scheme', 'oauth2')
            }}
          >
            <option value="caldav">CalDAV</option>
            <option value="graph">Exchange</option>
            <option value="google">Google</option>
            <option value="openxchange">Open-Xchange</option>
          </Select>
          {form.type === 'google' ? (
            <>
              <Select
                value={form.email_account_id}
                onChange={(e) => set('email_account_id', e.currentTarget.value)}
                aria-label={t('calendar.googleAccount')}
              >
                <option value="">{t('calendar.newGoogleAccount')}</option>
                {gmailAccounts.map((account) => (
                  <option key={account.id} value={account.id}>
                    {account.display_name || account.primary_email}
                  </option>
                ))}
              </Select>
              <Button
                onClick={() =>
                  startOAuthRedirect('google', form.email_account_id || undefined, 'calendar')
                }
              >
                {t('calendar.connectGoogle')}
              </Button>
            </>
          ) : (
            <>
              <Input
                placeholder={form.type === 'caldav' ? t('calendar.baseUrlOptional') : t('calendar.baseUrl')}
                value={form.base_url}
                onChange={(e) => set('base_url', e.currentTarget.value)}
              />
              <Select value={form.auth_scheme} onChange={(e) => set('auth_scheme', e.currentTarget.value as typeof form.auth_scheme)}>
                <option value="basic">Basic</option>
                <option value="oauth2">OAuth2</option>
              </Select>
              <Input placeholder={form.type === 'caldav' ? t('calendar.usernameEmail') : t('calendar.username')} value={form.username} onChange={(e) => set('username', e.currentTarget.value)} />
              <Input placeholder={form.auth_scheme === 'oauth2' ? t('calendar.accessToken') : t('calendar.password')} type="password" value={form.auth_scheme === 'oauth2' ? form.access_token : form.password} onChange={(e) => form.auth_scheme === 'oauth2' ? set('access_token', e.currentTarget.value) : set('password', e.currentTarget.value)} />
              <Button onClick={submit} disabled={createAccount.isPending}>{t('action.save')}</Button>
            </>
          )}
          {form.type === 'caldav' && (
            <label className="md:col-span-full flex items-start gap-2 text-[12px] text-muted-foreground">
              <input
                type="checkbox"
                checked={form.accept_invalid_tls}
                onChange={(e) => set('accept_invalid_tls', e.currentTarget.checked)}
                className="mt-0.5 size-4 accent-primary"
              />
              <span>{t('calendar.acceptInvalidTls')}</span>
            </label>
          )}
          {form.type === 'caldav' && createAccount.error && (
            <div className="md:col-span-full">
              <CaldavErrorAlert error={createAccount.error} />
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function MonthView({
  cursor,
  events,
  onSelectEvent,
}: {
  cursor: Date
  events: CalendarEvent[]
  onSelectEvent: (e: CalendarEvent) => void
}) {
  const { t } = useTranslation()
  const monthStart = new Date(cursor.getFullYear(), cursor.getMonth(), 1)
  const gridStart = startOfWeekMon(monthStart)
  const days = Array.from({ length: 42 }, (_, i) => addDays(gridStart, i))
  const today = new Date()

  return (
    <div className="flex h-full flex-col">
      <div className="grid shrink-0 grid-cols-7 border-b border-border">
        {WEEKDAYS.map((d) => (
          <div key={d} className="px-2 py-1.5 text-[11px] font-bold uppercase tracking-wide text-muted-foreground">
            {d}
          </div>
        ))}
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-7 grid-rows-6">
        {days.map((day, i) => {
          const inMonth = day.getMonth() === cursor.getMonth()
          const dayEvents = events
            .filter((e) => sameDay(new Date(e.starts_at), day))
            .sort((a, b) => a.starts_at.localeCompare(b.starts_at))
          return (
            <div key={i} className={cn('min-h-0 overflow-hidden border-b border-r border-border p-1', !inMonth && 'bg-secondary/30')}>
              <div
                className={cn(
                  'mb-1 flex size-6 items-center justify-center rounded-full text-[12px] font-semibold',
                  sameDay(day, today) ? 'bg-primary text-white' : inMonth ? 'text-foreground' : 'text-muted-foreground',
                )}
              >
                {day.getDate()}
              </div>
              <div className="space-y-0.5">
                {dayEvents.slice(0, 3).map((e) => (
                  <button
                    key={e.id}
                    onClick={() => onSelectEvent(e)}
                    className="block w-full truncate rounded px-1.5 py-0.5 text-left text-[11px] font-medium text-white hover:brightness-110"
                    style={{ backgroundColor: e.color }}
                    title={e.title}
                  >
                    {e.title}
                  </button>
                ))}
                {dayEvents.length > 3 && (
                  <div className="px-1.5 text-[11px] font-medium text-muted-foreground">
                    {t('calendar.more', { count: dayEvents.length - 3 })}
                  </div>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

function TimeGrid({
  view,
  cursor,
  events,
  onSelectEvent,
}: {
  view: ViewMode
  cursor: Date
  events: CalendarEvent[]
  onSelectEvent: (e: CalendarEvent) => void
}) {
  const { t } = useTranslation()
  const days = view === 'day' ? [startOfDay(cursor)] : Array.from({ length: 7 }, (_, i) => addDays(startOfWeekMon(cursor), i))
  const hours = Array.from({ length: 24 }, (_, h) => h)
  const today = new Date()
  const allDayEvents = events.filter((e) => e.all_day && days.some((day) => sameDay(new Date(e.starts_at), day)))

  return (
    <div className="flex h-full flex-col">
      {/* day headers */}
      <div className="flex shrink-0 border-b border-border pr-2">
        <div className="w-14 shrink-0" />
        {days.map((day) => (
          <div key={day.toISOString()} className="flex-1 px-2 py-2 text-center">
            <div className="text-[11px] font-bold uppercase tracking-wide text-muted-foreground">{WEEKDAYS[(day.getDay() + 6) % 7]}</div>
            <div
              className={cn(
                'mx-auto mt-1 flex size-7 items-center justify-center rounded-full text-[13px] font-semibold',
                sameDay(day, today) ? 'bg-primary text-white' : 'text-foreground',
              )}
            >
              {day.getDate()}
            </div>
          </div>
        ))}
      </div>
      {allDayEvents.length > 0 && (
        <div className="grid shrink-0 grid-cols-[56px_1fr] border-b border-border bg-secondary/30">
          <div className="px-2 py-2 text-right text-[10.5px] font-semibold text-muted-foreground">{t('calendar.allDay')}</div>
          <div className="flex flex-wrap gap-1 p-1.5">
            {allDayEvents.map((e) => (
              <button key={e.id} onClick={() => onSelectEvent(e)} className="rounded px-2 py-0.5 text-[11px] font-semibold text-white" style={{ backgroundColor: e.color }}>
                {e.title}
              </button>
            ))}
          </div>
        </div>
      )}
      {/* scrollable hour grid */}
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="flex">
          <div className="w-14 shrink-0">
            {hours.map((h) => (
              <div key={h} className="relative text-right" style={{ height: HOUR_PX }}>
                <span className="absolute -top-2 right-2 text-[10.5px] text-muted-foreground">{h === 0 ? '' : `${h}:00`}</span>
              </div>
            ))}
          </div>
          {days.map((day) => {
            const dayEvents = events.filter((e) => sameDay(new Date(e.starts_at), day) && !e.all_day)
            return (
              <div key={day.toISOString()} className="relative flex-1 border-l border-border">
                {hours.map((h) => (
                  <div key={h} className="border-b border-border/60" style={{ height: HOUR_PX }} />
                ))}
                {dayEvents.map((e) => {
                  const start = new Date(e.starts_at)
                  const end = new Date(e.ends_at)
                  const top = (start.getHours() + start.getMinutes() / 60) * HOUR_PX
                  const height = Math.max(22, ((end.getTime() - start.getTime()) / 3_600_000) * HOUR_PX)
                  return (
                    <button
                      key={e.id}
                      onClick={() => onSelectEvent(e)}
                      className="absolute left-1 right-1 overflow-hidden rounded-md px-1.5 py-1 text-left text-[11px] font-medium text-white shadow-sm hover:brightness-110"
                      style={{ top, height, backgroundColor: e.color }}
                      title={e.title}
                    >
                      <div className="truncate font-semibold">{e.title}</div>
                      {e.location && <div className="truncate opacity-90">{e.location}</div>}
                    </button>
                  )
                })}
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

function AgendaView({ events, onSelectEvent }: { events: CalendarEvent[]; onSelectEvent: (e: CalendarEvent) => void }) {
  return (
    <div className="h-full overflow-y-auto p-4">
      <div className="mx-auto flex max-w-[860px] flex-col gap-2">
        {events
          .slice()
          .sort((a, b) => a.starts_at.localeCompare(b.starts_at))
          .map((event) => (
            <button key={event.id} onClick={() => onSelectEvent(event)} className="grid grid-cols-[132px_1fr_auto] items-center gap-3 rounded-md border border-border bg-card px-3 py-2.5 text-left hover:bg-secondary/50">
              <span className="text-[12px] font-semibold text-muted-foreground">{new Date(event.starts_at).toLocaleString()}</span>
              <span>
                <span className="block text-[13.5px] font-bold">{event.title}</span>
                <span className="block text-[12px] text-muted-foreground">{event.location || event.organizer_email || ''}</span>
              </span>
              <span className="size-3 rounded-full" style={{ backgroundColor: event.color }} />
            </button>
          ))}
      </div>
    </div>
  )
}

function hhmm(d: Date): string {
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

function EventDialog({
  open,
  onClose,
  defaultDate,
  event,
}: {
  open: boolean
  onClose: () => void
  defaultDate: Date
  event: CalendarEvent | null
}) {
  const { t } = useTranslation()
  const { data: calendars = [] } = useCalendars()
  const createCalendar = useCreateCalendar()
  const createEvent = useCreateEvent()
  const updateEvent = useUpdateEvent()
  const deleteEvent = useDeleteEvent()
  const isEdit = event !== null
  const [form, setForm] = useState(() => emptyForm(defaultDate))

  // Reset the form whenever the dialog opens or the target event changes.
  useEffect(() => {
    if (!open) return
    if (event) {
      const s = new Date(event.starts_at)
      const e = new Date(event.ends_at)
      // Resetting a dialog draft is intentional when the selected remote event changes.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setForm({
        title: event.title,
        calendar_id: event.calendar_id,
        date: s.toISOString().slice(0, 10),
        start: hhmm(s),
        end: hhmm(e),
        location: event.location ?? '',
        description: event.description ?? '',
        recurrence: rruleFreq(event.rrule),
        recurrence_until: '',
        attendees: attendeesText(event.attendees),
        organizer_email: event.organizer_email ?? '',
        recurring_edit_scope: 'this',
        all_day: event.all_day,
      })
    } else {
      setForm(emptyForm(defaultDate))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, event])

  const selectedCalendar = form.calendar_id || calendars[0]?.id || ''

  function set<K extends keyof typeof form>(key: K, value: (typeof form)[K]) {
    setForm((f) => ({ ...f, [key]: value }))
  }

  async function submit() {
    if (!form.title.trim()) return
    const starts = form.all_day ? `${form.date}T00:00:00.000Z` : new Date(`${form.date}T${form.start}`).toISOString()
    const ends = form.all_day ? `${form.date}T23:59:59.000Z` : new Date(`${form.date}T${form.end}`).toISOString()
    const rrule = form.recurrence === 'none' ? null : `FREQ=${form.recurrence.toUpperCase()}${form.recurrence_until ? `;UNTIL=${form.recurrence_until.replaceAll('-', '')}T235959Z` : ''}`
    const attendees = form.attendees
      .split(/[,\n;]/)
      .map((email) => email.trim())
      .filter(Boolean)
      .map((email) => ({ email, partstat: 'NEEDS-ACTION' }))
    const data = {
      title: form.title,
      description: form.description || null,
      location: form.location || null,
      starts_at: starts,
      ends_at: ends,
      all_day: form.all_day,
      rrule,
      attendees,
      organizer_email: form.organizer_email || null,
      recurring_edit_scope: form.recurring_edit_scope as 'this' | 'following' | 'all',
    }
    if (isEdit && event) {
      updateEvent.mutate({ id: event.id, data }, { onSuccess: onClose })
      return
    }
    let calendarId = selectedCalendar
    if (!calendarId) {
      const created = await createCalendar.mutateAsync({ name: 'My calendar', color: '#2563EB' })
      calendarId = created.id
    }
    createEvent.mutate({ calendar_id: calendarId, ...data }, { onSuccess: onClose })
  }

  function remove() {
    if (event) deleteEvent.mutate(event.id, { onSuccess: onClose })
  }

  const busy = createEvent.isPending || updateEvent.isPending

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(480px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>{isEdit ? t('calendar.editEvent') : t('calendar.newEvent')}</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <Field label={t('calendar.title')}>
            <Input value={form.title} onChange={(e) => set('title', e.currentTarget.value)} />
          </Field>
          {calendars.length > 0 && (
            <Field label={t('calendar.calendar')}>
              <Select value={selectedCalendar} onChange={(e) => set('calendar_id', e.currentTarget.value)}>
                {calendars.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          <Field label={t('calendar.date')}>
            <Input type="date" value={form.date} onChange={(e) => set('date', e.currentTarget.value)} />
          </Field>
          {!form.all_day && (
            <div className="grid grid-cols-2 gap-3">
              <Field label={t('calendar.start')}>
                <Input type="time" value={form.start} onChange={(e) => set('start', e.currentTarget.value)} />
              </Field>
              <Field label={t('calendar.end')}>
                <Input type="time" value={form.end} onChange={(e) => set('end', e.currentTarget.value)} />
              </Field>
            </div>
          )}
          <Field label={t('calendar.location')}>
            <Input value={form.location} onChange={(e) => set('location', e.currentTarget.value)} />
          </Field>
          <Field label={t('calendar.description')}>
            <Input value={form.description} onChange={(e) => set('description', e.currentTarget.value)} />
          </Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label={t('calendar.recurrence')}>
              <Select value={form.recurrence} onChange={(e) => set('recurrence', e.currentTarget.value)}>
                <option value="none">{t('calendar.recurNone')}</option>
                <option value="daily">{t('calendar.recurDaily')}</option>
                <option value="weekly">{t('calendar.recurWeekly')}</option>
                <option value="monthly">{t('calendar.recurMonthly')}</option>
                <option value="yearly">{t('calendar.recurYearly')}</option>
              </Select>
            </Field>
            <Field label={t('calendar.recurrenceUntil')}>
              <Input type="date" value={form.recurrence_until} onChange={(e) => set('recurrence_until', e.currentTarget.value)} />
            </Field>
          </div>
          {isEdit && event?.rrule && (
            <Field label={t('calendar.recurringScope')}>
              <Select value={form.recurring_edit_scope} onChange={(e) => set('recurring_edit_scope', e.currentTarget.value)}>
                <option value="this">{t('calendar.thisEvent')}</option>
                <option value="following">{t('calendar.thisAndFollowing')}</option>
                <option value="all">{t('calendar.allEvents')}</option>
              </Select>
            </Field>
          )}
          <Field label={t('calendar.attendees')}>
            <div className="flex items-start gap-2">
              <div className="min-w-0 flex-1">
                <RecipientChips
                  label={t('calendar.attendees')}
                  value={splitAttendees(form.attendees)}
                  onChange={(next) => set('attendees', next.join(', '))}
                  mailboxId={calendars.find((calendar) => calendar.id === form.calendar_id)?.account_id ?? undefined}
                />
              </div>
              <Button variant="outline" onClick={() => discoverAttendees(form.attendees)} title={t('calendar.discoverKeys')}>
                <KeyRound className="size-4" />
              </Button>
            </div>
          </Field>
          <Field label={t('calendar.organizer')}>
            <Input value={form.organizer_email} onChange={(e) => set('organizer_email', e.currentTarget.value)} />
          </Field>
          <label className="flex items-center gap-2 text-[13px]">
            <input type="checkbox" checked={form.all_day} onChange={(e) => set('all_day', e.currentTarget.checked)} className="size-4 accent-primary" />
            {t('calendar.allDay')}
          </label>
          {event && (
            <div className="rounded-md border border-border bg-secondary/30 p-3 text-[12.5px] text-secondary-foreground">
              <div className="font-bold">{event.organizer_name || event.organizer_email || t('calendar.noOrganizer')}</div>
              {event.ms_busystatus && <div>{t('calendar.busyStatus')}: {event.ms_busystatus}</div>}
              {event.rrule && <div>{t('calendar.recurrence')}: {event.rrule}</div>}
              {event.ms_teams_url && (
                <a className="mt-2 inline-flex items-center gap-1 text-[#2563eb] font-semibold" href={event.ms_teams_url} target="_blank" rel="noreferrer">
                  <ExternalLink className="size-3.5" />
                  {t('calendar.joinTeams')}
                </a>
              )}
            </div>
          )}
        </div>
        <div className="mt-4 flex items-center gap-2">
          {isEdit && (
            <Button
              variant="ghost"
              onClick={remove}
              disabled={deleteEvent.isPending}
              className="text-red-600 hover:text-red-700"
            >
              <Trash2 className="size-4" />
              {t('action.delete')}
            </Button>
          )}
          <div className="ml-auto flex gap-2">
            <Button variant="ghost" onClick={onClose}>
              {t('action.cancel')}
            </Button>
            <Button onClick={submit} disabled={busy || !form.title.trim()}>
              {busy ? t('settings.saving') : isEdit ? t('calendar.save') : t('calendar.addEvent')}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function emptyForm(defaultDate: Date) {
  return {
    title: '',
    calendar_id: '',
    date: defaultDate.toISOString().slice(0, 10),
    start: '09:00',
    end: '10:00',
    location: '',
    description: '',
    recurrence: 'none',
    recurrence_until: '',
    attendees: '',
    organizer_email: '',
    recurring_edit_scope: 'this',
    all_day: false,
  }
}

function attendeesText(raw?: string | null) {
  if (!raw) return ''
  try {
    const values = JSON.parse(raw)
    return Array.isArray(values) ? values.map((a) => a.email).filter(Boolean).join(', ') : ''
  } catch {
    return ''
  }
}

function splitAttendees(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean)
}

function rruleFreq(rrule?: string | null) {
  const freq = rrule?.match(/FREQ=([^;]+)/i)?.[1]?.toLowerCase()
  return freq && ['daily', 'weekly', 'monthly', 'yearly'].includes(freq) ? freq : 'none'
}

function discoverAttendees(raw: string) {
  raw
    .split(/[,\n;]/)
    .map((email) => email.trim())
    .filter(Boolean)
    .forEach((email) => {
      apiGet(`/keys/discover?email=${encodeURIComponent(email)}`).catch(() => {})
    })
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label className="text-[12px] font-semibold">{label}</Label>
      {children}
    </div>
  )
}

function visibleRange(view: ViewMode, cursor: Date): { from: Date; to: Date } {
  if (view === 'agenda') return { from: startOfDay(cursor), to: addDays(startOfDay(cursor), 60) }
  if (view === 'day') return { from: startOfDay(cursor), to: addDays(startOfDay(cursor), 1) }
  if (view === 'week') {
    const from = startOfWeekMon(cursor)
    return { from, to: addDays(from, 7) }
  }
  const monthStart = new Date(cursor.getFullYear(), cursor.getMonth(), 1)
  const from = startOfWeekMon(monthStart)
  return { from, to: addDays(from, 42) }
}

function titleFor(view: ViewMode, cursor: Date): string {
  if (view === 'day') return `${WEEKDAYS[(cursor.getDay() + 6) % 7]}, ${MONTHS[cursor.getMonth()]} ${cursor.getDate()}, ${cursor.getFullYear()}`
  if (view === 'week') {
    const start = startOfWeekMon(cursor)
    const end = addDays(start, 6)
    return `${MONTHS[start.getMonth()].slice(0, 3)} ${start.getDate()} – ${MONTHS[end.getMonth()].slice(0, 3)} ${end.getDate()}, ${end.getFullYear()}`
  }
  return `${MONTHS[cursor.getMonth()]} ${cursor.getFullYear()}`
}
