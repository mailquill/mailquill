import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Bell, Check, X } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { useClickOutside } from '@/shared/hooks/useClickOutside'
import { useUnifiedInbox } from '@/shared/hooks/useMessages'
import { useAccountColorLookup } from '@/shared/hooks/useAccounts'
import { accountInitials } from '@/shared/lib/avatar'
import { parseFromAddr } from '@/shared/lib/format'
import type { Message } from '@/shared/types'

function timeAgo(dateStr: string | null | undefined): string {
  if (!dateStr) return ''
  const diff = Date.now() - new Date(dateStr).getTime()
  const m = Math.floor(diff / 60000)
  if (m < 1) return 'now'
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h`
  const d = Math.floor(h / 24)
  if (d < 7) return `${d}d`
  return new Date(dateStr).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

interface Notification {
  id: string
  message: Message
  title: string
  subject: string
  snippet: string
  date: string
  accent: string
  initials: string
}

function buildNotifications(
  messages: Message[],
  colorFor: (accountId: string) => string,
): Notification[] {
  return messages
    .filter((m) => !m.is_read)
    .map((m) => {
      const { name } = parseFromAddr(m.from_addr)
      return {
        id: m.id,
        message: m,
        title: name,
        subject: m.subject || '(no subject)',
        snippet: m.snippet,
        date: m.internal_date,
        accent: colorFor(m.account_id),
        initials: accountInitials(name),
      }
    })
    .sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime())
}

export function NotificationMenu() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [tab, setTab] = useState<'all' | 'unread'>('all')
  const [readIds, setReadIds] = useState<Set<string>>(() => new Set())
  const [dismissed, setDismissed] = useState<Set<string>>(() => new Set())
  const ref = useClickOutside<HTMLDivElement>(() => setOpen(false), open)

  const { data } = useUnifiedInbox()
  const colorFor = useAccountColorLookup()
  const all = useMemo(
    () => buildNotifications(data?.messages ?? [], colorFor).filter((n) => !dismissed.has(n.id)),
    [data?.messages, dismissed, colorFor],
  )
  const unreadCount = all.filter((n) => !readIds.has(n.id)).length
  const shown = tab === 'unread' ? all.filter((n) => !readIds.has(n.id)) : all

  function openNotification(n: Notification) {
    setReadIds((s) => new Set(s).add(n.id))
    const m = n.message
    // The folder route segment is the folder's full path, not its id.
    if (m.folder_path) {
      navigate(`/mail/${m.account_id}/${encodeURIComponent(m.folder_path)}/${m.thread_id ?? m.id}`)
    } else {
      navigate('/mail/unified')
    }
    setOpen(false)
  }

  function dismiss(id: string, event: React.MouseEvent) {
    event.stopPropagation()
    setDismissed((s) => new Set(s).add(id))
  }

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        title={t('topbar.notifications')}
        className={cn(
          'relative flex size-9 items-center justify-center rounded-lg border border-border text-secondary-foreground transition-colors hover:bg-secondary',
          open ? 'bg-secondary' : 'bg-card',
        )}
      >
        <Bell className="size-4" />
        {unreadCount > 0 && (
          <span className="absolute -right-1.5 -top-1.5 flex h-[17px] min-w-[17px] items-center justify-center rounded-full border-2 border-card bg-primary px-1 text-[10px] font-extrabold text-white">
            {unreadCount}
          </span>
        )}
      </button>

      {open && (
        <div className="absolute right-0 top-[120%] z-50 flex max-h-[min(560px,80vh)] w-96 flex-col overflow-hidden rounded-xl border border-border bg-popover shadow-2xl">
          <div className="flex shrink-0 items-center gap-2.5 border-b border-secondary px-4 pb-3 pt-3.5">
            <span className="text-[15px] font-bold tracking-tight text-foreground">{t('topbar.notifications')}</span>
            {unreadCount > 0 && (
              <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-bold text-primary">
                {t('topbar.newCount', { count: unreadCount })}
              </span>
            )}
            <button
              onClick={() => setReadIds(new Set(all.map((n) => n.id)))}
              disabled={unreadCount === 0}
              className="ml-auto text-[12px] font-semibold text-[#2563eb] disabled:text-muted-foreground"
            >
              {t('topbar.markAllRead')}
            </button>
          </div>

          <div className="flex shrink-0 gap-1 border-b border-secondary px-3 py-2.5">
            {(['all', 'unread'] as const).map((value) => {
              const on = tab === value
              return (
                <button
                  key={value}
                  onClick={() => setTab(value)}
                  className={cn(
                    'rounded-md px-3 py-1 text-[12.5px] font-semibold transition-colors',
                    on ? 'bg-[var(--mq-row-open)] text-[#1d4ed8]' : 'text-secondary-foreground hover:bg-secondary',
                  )}
                >
                  {value === 'all' ? t('topbar.tabAll') : t('topbar.tabUnread')}
                  {value === 'unread' && unreadCount > 0 ? ` · ${unreadCount}` : ''}
                </button>
              )
            })}
          </div>

          <div className="flex-1 overflow-y-auto">
            {shown.length === 0 ? (
              <div className="px-6 py-12 text-center text-muted-foreground">
                <div className="mx-auto mb-3.5 flex size-13 items-center justify-center rounded-full bg-secondary">
                  <Check className="size-6 text-muted-foreground" />
                </div>
                <div className="text-[13.5px] font-semibold text-secondary-foreground">
                  {tab === 'unread' ? t('topbar.allCaughtUp') : t('topbar.noNotifications')}
                </div>
                <div className="mt-0.5 text-xs">
                  {tab === 'unread' ? t('topbar.readEverything') : t('topbar.newActivityHint')}
                </div>
              </div>
            ) : (
              shown.map((n) => {
                const unread = !readIds.has(n.id)
                return (
                  <div
                    key={n.id}
                    onClick={() => openNotification(n)}
                    className={cn(
                      'group relative flex cursor-pointer gap-3 border-b border-secondary py-3 pl-3 pr-3.5 transition-colors hover:bg-secondary',
                      unread && 'bg-[#fff7f2] dark:bg-[#1e1a17]',
                    )}
                  >
                    {unread && <span className="absolute bottom-2.5 left-0 top-2.5 w-[3px] rounded-r-sm bg-primary" />}
                    <span
                      className="flex size-9 shrink-0 items-center justify-center rounded-full text-[12.5px] font-bold text-white"
                      style={{ backgroundColor: n.accent }}
                    >
                      {n.initials}
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-baseline gap-2">
                        <span
                          className={cn(
                            'truncate text-[13px] text-foreground',
                            unread ? 'font-bold' : 'font-semibold',
                          )}
                        >
                          {n.title}
                        </span>
                        <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">{timeAgo(n.date)}</span>
                      </div>
                      <div className="mt-0.5 truncate text-[12.5px] font-medium text-secondary-foreground">
                        {n.subject}
                      </div>
                      <div className="mt-0.5 line-clamp-2 text-[12px] leading-snug text-muted-foreground">
                        {n.snippet}
                      </div>
                    </div>
                    <button
                      onClick={(event) => dismiss(n.id, event)}
                      title={t('topbar.dismiss')}
                      className="absolute right-2 top-2 flex size-5 items-center justify-center rounded opacity-0 transition-opacity hover:bg-border group-hover:opacity-100"
                    >
                      <X className="size-3 text-muted-foreground" />
                    </button>
                  </div>
                )
              })
            )}
          </div>
        </div>
      )}
    </div>
  )
}
