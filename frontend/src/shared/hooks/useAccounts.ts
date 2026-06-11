import { useEffect, useRef } from 'react'
import { useQuery, useQueries, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/shared/api'
import type { Account, AccountAlias, Folder, SyncStatus } from '@/shared/types'

export function useAccounts() {
  return useQuery({
    queryKey: ['accounts'],
    queryFn: () => apiGet<Account[]>('/accounts'),
  })
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
    refetchInterval: 10_000,
  })
}

/**
 * Aggregate sync activity across all accounts. Polls every account's
 * sync-status (sharing the cache with {@link useSyncStatus}) and reports
 * whether any account is currently syncing.
 *
 * Pass `{ watch: true }` on exactly one mount (e.g. the mail layout) to also
 * auto-refresh folders and message lists. The backend commits messages per
 * folder as it goes, so while a sync is running we re-fetch on a short interval
 * to stream new messages into the UI, plus once more on completion (detected by
 * a change in `last_synced_at`) to catch the final batch. Header components call
 * it without `watch` purely to render a live indicator.
 */
export function useSyncActivity({ watch = false }: { watch?: boolean } = {}) {
  const qc = useQueryClient()
  const { data: accounts = [] } = useAccounts()
  const lastSyncedRef = useRef<Record<string, string | null>>({})

  const results = useQueries({
    queries: accounts.map((a) => ({
      queryKey: ['sync-status', a.id],
      queryFn: () => apiGet<SyncStatus>(`/accounts/${a.id}/sync-status`),
      // Poll faster while this account is syncing so progress and completion
      // are picked up promptly; idle accounts poll lazily.
      refetchInterval: (query: { state: { data?: SyncStatus } }) =>
        query.state.data?.state === 'syncing' ? 3_000 : 10_000,
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
  // Stable dependency: only re-run the effect when a completion timestamp moves.
  const signature = statuses.map((s) => `${s.account_id}:${s.last_synced_at}`).join(',')

  function refreshMail() {
    qc.invalidateQueries({ queryKey: ['folders'] })
    qc.invalidateQueries({ queryKey: ['unified'] })
    qc.invalidateQueries({ queryKey: ['unified-counts'] })
    qc.invalidateQueries({ queryKey: ['folder-messages'] })
  }

  // Stream partial results in while a sync is running.
  useEffect(() => {
    if (!watch || !syncing) return
    const id = setInterval(refreshMail, 3_000)
    return () => clearInterval(id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watch, syncing, qc])

  // Final refresh when a sync completes (last_synced_at changes).
  useEffect(() => {
    if (!watch) return
    let completed = false
    for (const s of statuses) {
      const prev = lastSyncedRef.current[s.account_id]
      if (prev !== undefined && prev !== s.last_synced_at && s.last_synced_at) {
        completed = true
      }
      lastSyncedRef.current[s.account_id] = s.last_synced_at
    }
    if (completed) refreshMail()
    // statuses is derived fresh each render; signature captures the change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature, watch, qc])

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
      refetchInterval: (query: { state: { data?: SyncStatus } }) =>
        query.state.data?.state === 'syncing' ? 3_000 : 10_000,
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
    mutationFn: (data: unknown) => apiPost('/accounts', data),
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

export function useTriggerSync() {
  const qc = useQueryClient()

  return useMutation({
    mutationFn: (accountId: string) => apiPost(`/accounts/${accountId}/sync`),
    onSuccess: (_data, accountId) => {
      qc.invalidateQueries({ queryKey: ['sync-status', accountId] })
      qc.invalidateQueries({ queryKey: ['folders', accountId] })
      qc.invalidateQueries({ queryKey: ['folder-messages'] })
      qc.invalidateQueries({ queryKey: ['unified'] })
    },
  })
}
