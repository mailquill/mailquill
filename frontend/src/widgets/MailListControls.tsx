import { useTranslation } from 'react-i18next'
import { cn } from '@/shared/lib/utils'

export type MailFilter = 'all' | 'unread'

/**
 * Title-row controls for a message list: the Alle/Ungelesen quick filter. State
 * lives in the page so the filter can drive a server-side query.
 */
export function MailListControls({
  filter,
  onFilterChange,
  unreadCount = 0,
}: {
  filter: MailFilter
  onFilterChange: (f: MailFilter) => void
  unreadCount?: number
}) {
  const { t } = useTranslation()
  return (
    <div className="flex shrink-0 items-center rounded-md border border-secondary p-0.5 text-[12px] font-semibold">
      <button
        type="button"
        onClick={() => onFilterChange('all')}
        className={cn(
          'rounded px-2 py-0.5 transition-colors',
          filter === 'all' ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground',
        )}
      >
        {t('ml.all')}
      </button>
      <button
        type="button"
        onClick={() => onFilterChange('unread')}
        className={cn(
          'flex items-center gap-1 rounded px-2 py-0.5 transition-colors',
          filter === 'unread' ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground',
        )}
      >
        {t('ml.unread')}
        {unreadCount > 0 && (
          <span className="rounded-full bg-primary px-1.5 text-[10px] leading-4 text-primary-foreground">
            {unreadCount}
          </span>
        )}
      </button>
    </div>
  )
}
