import { useMemo } from 'react'
import { useQuery, useQueries, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiPatch, apiDelete } from '@/shared/api'
import { accountColor, resolveAccountColor } from '@/shared/lib/avatar'
import type { Account, AccountAlias, DiscoveredContactBook, Folder, SyncStatus } from '@/shared/types'

export function useAccounts() {
  return useQuery({
    queryKey: ['accounts'],
    queryFn: () => apiGet<Account[]>('/accounts'),
  })
}

/**
 * Resolve an account id to its display colour (user-chosen, else hash-derived).
 * For components that only hold an account id, not the full account.
 */
export function useAccountColorLookup(): (accountId: string) => string {
  const { data: accounts = [] } = useAccounts()
  return useMemo(() => {
    const byId = new Map(accounts.map((a) => [a.id, resolveAccountColor(a)]))
    return (accountId: string) => byId.get(accountId) ?? accountColor(accountId)
  }, [accounts])
}

export function useFolders(accountId: string) {
  return useQuery({
    queryKey: ['folders', accountId],
    queryFn: () => apiGet<Folder[]>(`/accounts/${accountId}/folders`),
    enabled: !!accountId,
  })
}

export function useSyncStatus(accountId: string) {
  return useQuery({
    queryKey: ['sync-status', accountId],
    queryFn: () => apiGet<SyncStatus>(`/accounts/${accountId}/sync-status`),
    enabled: !!accountId,
  })
}

/**
 * Aggregate sync activity across all accounts. Initial status is fetched once;
 * subsequent changes arrive through the application SSE stream and update the
 * same query cache.
 */
export function useSyncActivity() {
  const { data: accounts = [] } = useAccounts()

  const results = useQueries({
    queries: accounts.map((a) => ({
      queryKey: ['sync-status', a.id],
      queryFn: () => apiGet<SyncStatus>(`/accounts/${a.id}/sync-status`),
    })),
  })

  const statuses = results
    .map((r) => r.data)
    .filter((s): s is SyncStatus => Boolean(s))
  const syncing = statuses.some((s) => s.state === 'syncing')
  // Aggregate progress across accounts currently syncing.
  const active = statuses.filter((s) => s.state === 'syncing')
  const synced = active.reduce((n, s) => n + (s.synced ?? 0), 0)
  const total = active.reduce((n, s) => n + (s.total ?? 0), 0)
  return { syncing, synced, total }
}

/**
 * Per-account sync status for the refresh-status menu. Shares the
 * `['sync-status', id]` cache with {@link useSyncActivity}, so no extra polling.
 */
export function useSyncStatuses(): { account: Account; status?: SyncStatus }[] {
  const { data: accounts = [] } = useAccounts()
  const results = useQueries({
    queries: accounts.map((a) => ({
      queryKey: ['sync-status', a.id],
      queryFn: () => apiGet<SyncStatus>(`/accounts/${a.id}/sync-status`),
    })),
  })
  return accounts.map((account, i) => ({ account, status: results[i]?.data }))
}

export function useAliases(accountId: string) {
  return useQuery({
    queryKey: ['aliases', accountId],
    queryFn: () => apiGet<AccountAlias[]>(`/accounts/${accountId}/aliases`),
    enabled: !!accountId,
  })
}

export function useReorderAccounts() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (accountIds: string[]) => apiPut('/accounts/order', { account_ids: accountIds }),
    onMutate: async (accountIds) => {
      await qc.cancelQueries({ queryKey: ['accounts'] })
      const prev = qc.getQueryData<Account[]>(['accounts'])
      qc.setQueryData<Account[]>(['accounts'], (old) => {
        if (!old) return old
        const byId = new Map(old.map((a) => [a.id, a]))
        return accountIds
          .map((id) => byId.get(id))
          .filter((a): a is Account => Boolean(a))
      })
      return prev
    },
    onError: (_e, _ids, prev) => {
      if (prev) qc.setQueryData(['accounts'], prev)
    },
    onSettled: () => qc.invalidateQueries({ queryKey: ['accounts'] }),
  })
}

export function useDeleteAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/accounts/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['accounts'] }),
  })
}

export function useCreateAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: unknown) => apiPost<Account>('/accounts', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['accounts'] }),
  })
}

export function useUpdateAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: unknown }) => apiPut(`/accounts/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['accounts'] }),
  })
}

export function useSetFolderSync(accountId: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ folderPath, syncEnabled }: { folderPath: string; syncEnabled: boolean }) =>
      apiPatch(`/accounts/${accountId}/folders/${encodeURIComponent(folderPath)}/sync`, {
        sync_enabled: syncEnabled,
      }),
    onMutate: async ({ folderPath, syncEnabled }) => {
      await qc.cancelQueries({ queryKey: ['folders', accountId] })
      const prev = qc.getQueryData<Folder[]>(['folders', accountId])
      qc.setQueryData<Folder[]>(['folders', accountId], (old) =>
        old?.map((f) => (f.full_path === folderPath ? { ...f, sync_enabled: syncEnabled } : f)),
      )
      return prev
    },
    onError: (_e, _vars, prev) => {
      if (prev) qc.setQueryData(['folders', accountId], prev)
    },
    onSettled: () => qc.invalidateQueries({ queryKey: ['folders', accountId] }),
  })
}

/** All/none quick selection: set sync_enabled on every folder of the account. */
export function useSetAllFoldersSync(accountId: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (syncEnabled: boolean) =>
      apiPut(`/accounts/${accountId}/folders/sync`, { sync_enabled: syncEnabled }),
    onMutate: async (syncEnabled) => {
      await qc.cancelQueries({ queryKey: ['folders', accountId] })
      const prev = qc.getQueryData<Folder[]>(['folders', accountId])
      qc.setQueryData<Folder[]>(['folders', accountId], (old) =>
        old?.map((f) => ({ ...f, sync_enabled: syncEnabled })),
      )
      return prev
    },
    onError: (_e, _vars, prev) => {
      if (prev) qc.setQueryData(['folders', accountId], prev)
    },
    onSettled: () => qc.invalidateQueries({ queryKey: ['folders', accountId] }),
  })
}

export function useTriggerSync() {
  return useMutation({
    mutationFn: (accountId: string) => apiPost(`/accounts/${accountId}/sync`),
  })
}

export function useEnableMailboxContacts() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (accountId: string) => apiPost(`/accounts/${accountId}/contacts/enable`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['accounts'] })
      qc.invalidateQueries({ queryKey: ['contact-accounts'] })
    },
  })
}

export function useDisableMailboxContacts() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ accountId, keepDownloadedContacts }: { accountId: string; keepDownloadedContacts: boolean }) =>
      apiPost(`/accounts/${accountId}/contacts/disable`, {
        keep_downloaded_contacts: keepDownloadedContacts,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['accounts'] })
      qc.invalidateQueries({ queryKey: ['contact-accounts'] })
      qc.invalidateQueries({ queryKey: ['contacts'] })
    },
  })
}

export function useDiscoverMailboxContacts() {
  return useMutation({
    mutationFn: ({ accountId, selectedBookRemoteIds, acceptInvalidTls, tlsDecision }: { accountId: string; selectedBookRemoteIds?: string[]; acceptInvalidTls?: boolean; tlsDecision?: 'accept' | 'accept_always' }) =>
      apiPost<{ source_id: string; books: DiscoveredContactBook[] }>(`/accounts/${accountId}/contacts/discover`, {
        selected_book_remote_ids: selectedBookRemoteIds,
        accept_invalid_tls: acceptInvalidTls ?? false,
        tls_decision: tlsDecision,
      }),
  })
}
