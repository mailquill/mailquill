import { useMutation, useQueryClient } from '@tanstack/react-query'
import { apiPost } from '@/shared/api'

interface DavSyncResult {
  contacts: number
  events: number
  errors: string[]
}

/**
 * Trigger CardDAV/CalDAV sync for one mail account through
 * `POST /accounts/:id/sync-dav`, then refetch contacts, calendars and events.
 * @returns A mutation taking the account id and resolving to how many contacts
 *   and events were synced plus any per-collection errors.
 */
export function useSyncDav() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (accountId: string) => apiPost<DavSyncResult>(`/accounts/${accountId}/sync-dav`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['contacts'] })
      qc.invalidateQueries({ queryKey: ['calendars'] })
      qc.invalidateQueries({ queryKey: ['events'] })
    },
  })
}
