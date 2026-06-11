import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSearchParams } from 'react-router-dom'
import { ChevronLeft, ChevronRight, Plus } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { DavSyncButton } from '@/widgets/DavSyncButton'
import {
  useCalendars,
  useCreateCalendar,
  useCreateEvent,
  useEvents,
} from '@/shared/hooks/useCalendar'
import { useModuleNav } from '@/shared/hooks/useModuleNav'
import type { CalendarEvent } from '@/shared/types'

type ViewMode = 'month' | 'week' | 'day'

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
  const closeAdd = () => setSearchParams({}, { replace: true })

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
            {(['month', 'week', 'day'] as ViewMode[]).map((m) => (
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

      <div className="min-h-0 flex-1 overflow-hidden">
        {view === 'month' && <MonthView cursor={cursor} events={visibleEvents} />}
        {view !== 'month' && <TimeGrid view={view} cursor={cursor} events={visibleEvents} />}
      </div>

      <NewEventDialog open={addOpen} onClose={closeAdd} defaultDate={cursor} />
    </div>
  )
}

function MonthView({ cursor, events }: { cursor: Date; events: CalendarEvent[] }) {
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
                  <div
                    key={e.id}
                    className="truncate rounded px-1.5 py-0.5 text-[11px] font-medium text-white"
                    style={{ backgroundColor: e.color }}
                    title={e.title}
                  >
                    {e.title}
                  </div>
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

function TimeGrid({ view, cursor, events }: { view: ViewMode; cursor: Date; events: CalendarEvent[] }) {
  const days = view === 'day' ? [startOfDay(cursor)] : Array.from({ length: 7 }, (_, i) => addDays(startOfWeekMon(cursor), i))
  const hours = Array.from({ length: 24 }, (_, h) => h)
  const today = new Date()

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
                    <div
                      key={e.id}
                      className="absolute left-1 right-1 overflow-hidden rounded-md px-1.5 py-1 text-[11px] font-medium text-white shadow-sm"
                      style={{ top, height, backgroundColor: e.color }}
                      title={e.title}
                    >
                      <div className="truncate font-semibold">{e.title}</div>
                      {e.location && <div className="truncate opacity-90">{e.location}</div>}
                    </div>
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

function NewEventDialog({ open, onClose, defaultDate }: { open: boolean; onClose: () => void; defaultDate: Date }) {
  const { t } = useTranslation()
  const { data: calendars = [] } = useCalendars()
  const createCalendar = useCreateCalendar()
  const createEvent = useCreateEvent()
  const dateStr = defaultDate.toISOString().slice(0, 10)
  const [form, setForm] = useState({ title: '', calendar_id: '', date: dateStr, start: '09:00', end: '10:00', location: '', all_day: false })
  // Default to the first calendar without an effect; the explicit choice wins.
  const selectedCalendar = form.calendar_id || calendars[0]?.id || ''

  function set<K extends keyof typeof form>(key: K, value: (typeof form)[K]) {
    setForm((f) => ({ ...f, [key]: value }))
  }

  async function submit() {
    if (!form.title.trim()) return
    let calendarId = selectedCalendar
    if (!calendarId) {
      const created = await createCalendar.mutateAsync({ name: 'My calendar', color: '#2563EB' })
      calendarId = created.id
    }
    const starts = form.all_day ? `${form.date}T00:00:00.000Z` : new Date(`${form.date}T${form.start}`).toISOString()
    const ends = form.all_day ? `${form.date}T23:59:59.000Z` : new Date(`${form.date}T${form.end}`).toISOString()
    createEvent.mutate(
      { calendar_id: calendarId, title: form.title, location: form.location || null, starts_at: starts, ends_at: ends, all_day: form.all_day },
      { onSuccess: () => { setForm((f) => ({ ...f, title: '', location: '' })); onClose() } },
    )
  }

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(480px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>{t('calendar.newEvent')}</DialogTitle>
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
          <label className="flex items-center gap-2 text-[13px]">
            <input type="checkbox" checked={form.all_day} onChange={(e) => set('all_day', e.currentTarget.checked)} className="size-4 accent-primary" />
            {t('calendar.allDay')}
          </label>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            {t('action.cancel')}
          </Button>
          <Button onClick={submit} disabled={createEvent.isPending || !form.title.trim()}>
            {createEvent.isPending ? t('settings.saving') : t('calendar.addEvent')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
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
