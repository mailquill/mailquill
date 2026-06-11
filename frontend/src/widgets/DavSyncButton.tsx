import { useTranslation } from 'react-i18next'
import { RefreshCw } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { Button } from '@/shared/components/ui/button'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useSyncDav } from '@/shared/hooks/useDav'

/** Triggers CardDAV/CalDAV sync across all connected accounts. */
export function DavSyncButton() {
  const { t } = useTranslation()
  const { data: accounts = [] } = useAccounts()
  const sync = useSyncDav()

  function run() {
    accounts.forEach((account) => sync.mutate(account.id))
  }

  return (
    <Button variant="outline" size="sm" onClick={run} disabled={sync.isPending || !accounts.length}>
      <RefreshCw className={cn('size-4', sync.isPending && 'animate-spin')} />
      {sync.isPending ? t('action.syncing') : t('action.sync')}
    </Button>
  )
}
