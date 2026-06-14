import { useEffect, useRef, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Search, Filter, Check, X } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { Input } from '@/shared/components/ui/input'
import { Select } from '@/shared/components/ui/select'
import { useClickOutside } from '@/shared/hooks/useClickOutside'

interface SearchFilter {
  from: string
  to: string
  subject: string
  has: string
  not: string
  within: string
  withinDate: string
  att: boolean
  unread: boolean
}

const EMPTY_FILTER: SearchFilter = {
  from: '',
  to: '',
  subject: '',
  has: '',
  not: '',
  within: '',
  withinDate: '',
  att: false,
  unread: false,
}

const WITHIN_DAYS: Record<string, number> = { '1d': 1, '3d': 3, '1w': 7, '2w': 14, '1m': 30, '6m': 180, '1y': 365 }
const WITHIN_KEYS = ['1d', '3d', '1w', '2w', '1m', '6m', '1y'] as const

function isActive(f: SearchFilter): boolean {
  return !!(f.from || f.to || f.subject || f.has || f.not || (f.within && f.withinDate) || f.att || f.unread)
}

function filterCount(f: SearchFilter): number {
  let n = 0
  ;(['from', 'to', 'subject', 'has', 'not'] as const).forEach((k) => {
    if (f[k]) n++
  })
  if (f.within && f.withinDate) n++
  if (f.att) n++
  if (f.unread) n++
  return n
}

function isoDate(d: Date): string {
  return d.toISOString().slice(0, 10)
}

/** Build the /mail/search query string from the free-text box and the filter. */
function buildParams(searchText: string, f: SearchFilter): URLSearchParams {
  const params = new URLSearchParams()
  const q = [searchText.trim(), f.has.trim()].filter(Boolean).join(' ')
  if (q) params.set('q', q)
  if (f.from.trim()) params.set('from', f.from.trim())
  if (f.to.trim()) params.set('to', f.to.trim())
  if (f.subject.trim()) params.set('subject', f.subject.trim())
  if (f.not.trim()) params.set('not', f.not.trim())
  if (f.within && f.withinDate && WITHIN_DAYS[f.within]) {
    const days = WITHIN_DAYS[f.within]
    const ref = new Date(f.withinDate)
    if (!Number.isNaN(ref.getTime())) {
      const after = new Date(ref)
      after.setDate(after.getDate() - days)
      const before = new Date(ref)
      before.setDate(before.getDate() + days)
      params.set('after', isoDate(after))
      params.set('before', `${isoDate(before)}T23:59:59Z`)
    }
  }
  if (f.att) params.set('has_attachment', 'true')
  if (f.unread) params.set('is_read', 'false')
  return params
}

export function MailSearch() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const inputRef = useRef<HTMLInputElement | null>(null)
  const [search, setSearch] = useState(searchParams.get('q') ?? '')
  const [filter, setFilter] = useState<SearchFilter>(EMPTY_FILTER)
  const [open, setOpen] = useState(false)
  const menuRef = useClickOutside<HTMLDivElement>(() => setOpen(false), open)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        inputRef.current?.focus()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const active = isActive(filter)
  const count = filterCount(filter)
  const set = <K extends keyof SearchFilter>(k: K, v: SearchFilter[K]) =>
    setFilter((f) => ({ ...f, [k]: v }))

  function runSearch() {
    const params = buildParams(search, filter)
    if ([...params.keys()].length === 0) return
    setOpen(false)
    navigate(`/mail/search?${params}`)
  }

  return (
    <div className="flex min-w-0 flex-1 items-center gap-3">
      <form
        className="relative min-w-0 flex-1 sm:max-w-xl"
        onSubmit={(e) => {
          e.preventDefault()
          runSearch()
        }}
      >
        <Search className="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground" aria-hidden="true" />
        <Input
          ref={inputRef}
          id="mail-search"
          name="mail-search"
          type="search"
          autoComplete="off"
          value={search}
          onChange={(e) => setSearch(e.currentTarget.value)}
          className="bg-secondary pl-9 pr-9"
          placeholder={t('filter.searchPlaceholder')}
          aria-label={t('filter.searchPlaceholder')}
        />
        {search ? (
          <button
            type="button"
            onClick={() => setSearch('')}
            className="absolute right-2.5 top-2.5 text-muted-foreground hover:text-foreground"
            aria-label={t('filter.clear')}
          >
            <X className="size-4" />
          </button>
        ) : (
          <kbd className="pointer-events-none absolute right-3 top-2.5 rounded border border-border px-1.5 text-[10px] text-muted-foreground">
            Ctrl K
          </kbd>
        )}
      </form>

      <div ref={menuRef} className="relative shrink-0">
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          title={t('filter.title')}
          className={cn(
            'flex h-9 items-center gap-2 rounded-lg border px-3 text-[13px] font-semibold transition-colors',
            active
              ? 'border-[#3b82f6] bg-[var(--mq-row-open)] text-[#1d4ed8]'
              : open
                ? 'border-[#3b82f6] text-secondary-foreground'
                : 'border-border text-secondary-foreground hover:bg-secondary',
          )}
        >
          <Filter className={cn('size-[15px]', active && 'fill-[#2563eb] text-[#2563eb]')} />
          <span className="hidden sm:inline">{t('filter.title')}</span>
          {count > 0 && (
            <span className="inline-flex h-[17px] min-w-[17px] items-center justify-center rounded-full bg-[#2563eb] px-1 text-[10.5px] font-bold text-white">
              {count}
            </span>
          )}
        </button>

        {open && (
          <div className="absolute right-0 top-[calc(100%+6px)] z-50 w-[min(580px,calc(100vw-2rem))] rounded-xl border border-border bg-popover p-5 shadow-xl">
            <FilterRow label={t('filter.from')}>
              <Input className="h-[34px]" name="filter-from" value={filter.from} onChange={(e) => set('from', e.currentTarget.value)} placeholder={t('filter.fromPh')} />
            </FilterRow>
            <FilterRow label={t('filter.to')}>
              <Input className="h-[34px]" name="filter-to" value={filter.to} onChange={(e) => set('to', e.currentTarget.value)} placeholder={t('filter.toPh')} />
            </FilterRow>
            <FilterRow label={t('filter.subject')}>
              <Input className="h-[34px]" name="filter-subject" value={filter.subject} onChange={(e) => set('subject', e.currentTarget.value)} />
            </FilterRow>
            <FilterRow label={t('filter.has')}>
              <Input className="h-[34px]" name="filter-has" value={filter.has} onChange={(e) => set('has', e.currentTarget.value)} />
            </FilterRow>
            <FilterRow label={t('filter.hasNot')}>
              <Input className="h-[34px]" name="filter-not" value={filter.not} onChange={(e) => set('not', e.currentTarget.value)} />
            </FilterRow>
            <FilterRow label={t('filter.date')}>
              <Select
                className="h-[34px] flex-1"
                name="filter-within"
                value={filter.within}
                onChange={(e) =>
                  setFilter((f) => ({ ...f, within: e.currentTarget.value, withinDate: f.withinDate || isoDate(new Date()) }))
                }
              >
                <option value="">{t('filter.anyTime')}</option>
                {WITHIN_KEYS.map((k) => (
                  <option key={k} value={k}>
                    {t(`filter.${k}`)}
                  </option>
                ))}
              </Select>
              <Input
                className="h-[34px] flex-1"
                name="filter-within-date"
                type="date"
                value={filter.withinDate}
                onChange={(e) => set('withinDate', e.currentTarget.value)}
              />
            </FilterRow>

            <div className="mb-1 flex gap-7 pl-[138px] pt-3">
              <CheckBox checked={filter.att} onChange={(v) => set('att', v)} label={t('filter.hasAttachment')} />
              <CheckBox checked={filter.unread} onChange={(v) => set('unread', v)} label={t('filter.unreadOnly')} />
            </div>

            <div className="mt-3.5 flex items-center gap-3 border-t border-border pt-3.5">
              {active && (
                <button
                  type="button"
                  onClick={() => setFilter(EMPTY_FILTER)}
                  className="text-[13px] font-semibold text-destructive"
                >
                  {t('filter.reset')}
                </button>
              )}
              <button
                type="button"
                onClick={runSearch}
                className="ml-auto h-9 rounded-lg bg-[#ea580c] px-6 text-[13px] font-bold text-white shadow-[0_4px_14px_rgba(234,88,12,0.3)] hover:bg-[#c2410c]"
              >
                {t('filter.search')}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

function FilterRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-2.5 flex items-center gap-3.5">
      <span className="w-[124px] shrink-0 text-right text-[13px] text-muted-foreground">{label}</span>
      <div className="flex min-w-0 flex-1 gap-2">{children}</div>
    </div>
  )
}

function CheckBox({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button type="button" onClick={() => onChange(!checked)} className="flex items-center gap-2.5 text-[13px] text-secondary-foreground">
      <span
        className={cn(
          'flex size-[18px] shrink-0 items-center justify-center rounded',
          checked ? 'bg-[#2563eb]' : 'border-[1.5px] border-input bg-card',
        )}
      >
        {checked && <Check className="size-3 text-white" strokeWidth={3} />}
      </span>
      {label}
    </button>
  )
}
