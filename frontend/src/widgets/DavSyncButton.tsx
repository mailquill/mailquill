import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { RefreshCw } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { Button } from '@/shared/components/ui/button'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useSyncDav } from '@/shared/hooks/useDav'

interface SyncStatus {
  contacts: number
  events: number
  errors: string[]
}

/** Triggers CardDAV/CalDAV sync across all connected accounts and surfaces the
 * aggregated result (counts, or errors) inline. */
export function DavSyncButton() {
  const { t } = useTranslation()
  const { data: accounts = [] } = useAccounts()
  const sync = useSyncDav()
  const [status, setStatus] = useState<SyncStatus | null>(null)

  async function run() {
    setStatus(null)
    let contacts = 0
    let events = 0
    const errors: string[] = []
    for (const account of accounts) {
      try {
        const r = await sync.mutateAsync(account.id)
        contacts += r.contacts
        events += r.events
        errors.push(...r.errors.map((e) => `${account.primary_email}: ${e}`))
      } catch (e) {
        errors.push(`${account.primary_email}: ${e instanceof Error ? e.message : String(e)}`)
      }
    }
    setStatus({ contacts, events, errors })
  }

  return (
    <div className="flex items-center gap-2">
      {status && (
        <span
          className={cn(
            'max-w-[220px] truncate text-[11px]',
            status.errors.length ? 'text-red-500' : 'text-muted-foreground',
          )}
          title={status.errors.length ? status.errors.join('\n') : undefined}
        >
          {status.errors.length
            ? t('action.syncErrors', { count: status.errors.length })
            : t('action.syncOk', { events: status.events, contacts: status.contacts })}
        </span>
      )}
      <Button variant="outline" size="sm" onClick={run} disabled={sync.isPending || !accounts.length}>
        <RefreshCw className={cn('size-4', sync.isPending && 'animate-spin')} />
        {sync.isPending ? t('action.syncing') : t('action.sync')}
      </Button>
    </div>
  )
}
