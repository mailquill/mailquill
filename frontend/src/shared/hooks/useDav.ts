import { useMutation, useQueryClient } from '@tanstack/react-query'
import { apiPost } from '@/shared/api'

interface DavSyncResult {
  contacts: number
  events: number
  errors: string[]
}

/** Trigger CardDAV/CalDAV sync for one account, refreshing contacts + calendar data. */
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
