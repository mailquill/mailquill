import { useMemo } from 'react'
import { useQuery, useQueries, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiPatch, apiDelete } from '@/shared/api'
import { accountColor, resolveAccountColor } from '@/shared/lib/avatar'
import type { Account, AccountAlias, DiscoveredContactBook, Folder, SyncStatus } from '@/shared/types'

/**
 * All mail accounts of the signed-in user from `GET /accounts`, cached under
 * `['accounts']` in the user's chosen order.
 * @returns A query for the `Account[]`.
 */
export function useAccounts() {
  return useQuery({
    queryKey: ['accounts'],
    queryFn: () => apiGet<Account[]>('/accounts'),
  })
}

/**
 * Resolve an account id to its display colour (user-chosen, else hash-derived).
 * For components that only hold an account id, not the full account.
 * @returns A memoised lookup from account id to CSS colour; unknown ids fall
 *   back to the hash-derived colour.
 */
export function useAccountColorLookup(): (accountId: string) => string {
  const { data: accounts = [] } = useAccounts()
  return useMemo(() => {
    const byId = new Map(accounts.map((a) => [a.id, resolveAccountColor(a)]))
    return (accountId: string) => byId.get(accountId) ?? accountColor(accountId)
  }, [accounts])
}

/**
 * Folder tree of one account from `GET /accounts/:id/folders`, cached under
 * `['folders', accountId]`. Unread counts in the sidebar read from it, so the
 * message mutations invalidate it.
 * @param accountId - Account whose folders to load; disabled while empty.
 * @returns A query for the `Folder[]`.
 */
export function useFolders(accountId: string) {
  return useQuery({
    queryKey: ['folders', accountId],
    queryFn: () => apiGet<Folder[]>(`/accounts/${accountId}/folders`),
    enabled: !!accountId,
  })
}

/**
 * Current IMAP sync state of one account from `GET /accounts/:id/sync-status`,
 * cached under `['sync-status', accountId]`. Later changes arrive through the
 * SSE `sync` event, which writes into the same cache entry.
 * @param accountId - Account to watch; disabled while empty.
 * @returns A query for the `SyncStatus`.
 */
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
 * @returns `syncing` when any account is mid-sync, plus the `synced` and
 *   `total` message counts summed over the accounts currently syncing.
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
 * @returns One entry per account, in account order, with its status once loaded.
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

/**
 * Sender aliases of one account from `GET /accounts/:id/aliases`, cached under
 * `['aliases', accountId]`, for the From picker in compose.
 * @param accountId - Account whose aliases to load; disabled while empty.
 * @returns A query for the `AccountAlias[]`.
 */
export function useAliases(accountId: string) {
  return useQuery({
    queryKey: ['aliases', accountId],
    queryFn: () => apiGet<AccountAlias[]>(`/accounts/${accountId}/aliases`),
    enabled: !!accountId,
  })
}

/**
 * Persist a new account order through `PUT /accounts/order`. The `['accounts']`
 * cache is reordered optimistically, restored on failure, and refetched once
 * the request settles.
 * @returns A mutation taking the full list of account ids in their new order.
 */
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

/**
 * Remove an account and its synced data through `DELETE /accounts/:id`, then
 * refetch `['accounts']`.
 * @returns A mutation taking the account id.
 */
export function useDeleteAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/accounts/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['accounts'] }),
  })
}

/**
 * Add a mail account through `POST /accounts`, then refetch `['accounts']`.
 * @returns A mutation taking the setup wizard's payload and resolving to the new `Account`.
 */
export function useCreateAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: unknown) => apiPost<Account>('/accounts', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['accounts'] }),
  })
}

/**
 * Change an account's settings or credentials through `PUT /accounts/:id`,
 * then refetch `['accounts']`.
 * @returns A mutation taking `{ id, data }` where `data` is the settings payload.
 */
export function useUpdateAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: unknown }) => apiPut(`/accounts/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['accounts'] }),
  })
}

/**
 * Include or exclude one folder from sync through
 * `PATCH /accounts/:id/folders/:path/sync`. The flag flips optimistically in
 * the `['folders', accountId]` cache, is restored on failure, and the folders
 * are refetched once the request settles.
 * @param accountId - Account the folder belongs to.
 * @returns A mutation taking `{ folderPath, syncEnabled }`.
 */
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

/**
 * All/none quick selection: set `sync_enabled` on every folder of the account
 * through `PUT /accounts/:id/folders/sync`. Applied optimistically to the
 * `['folders', accountId]` cache, restored on failure, refetched once settled.
 * @param accountId - Account whose folders to update.
 * @returns A mutation taking the target `syncEnabled` flag.
 */
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

/**
 * Ask the server to start a sync cycle for one account through
 * `POST /accounts/:id/sync`. Progress is not awaited here; it arrives through
 * the SSE `sync` event into the `['sync-status', id]` cache.
 * @returns A mutation taking the account id.
 */
export function useTriggerSync() {
  return useMutation({
    mutationFn: (accountId: string) => apiPost(`/accounts/${accountId}/sync`),
  })
}

/**
 * Turn on contact sync for a mail account's address book through
 * `POST /accounts/:id/contacts/enable`, then refetch `['accounts']` and
 * `['contact-accounts']`.
 * @returns A mutation taking the account id.
 */
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

/**
 * Turn off contact sync for a mail account through
 * `POST /accounts/:id/contacts/disable`, optionally keeping the contacts it
 * already downloaded, then refetch accounts, contact accounts and contacts.
 * @returns A mutation taking `{ accountId, keepDownloadedContacts }`.
 */
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

/**
 * Probe a mail account's CardDAV server for address books through
 * `POST /accounts/:id/contacts/discover`, optionally pre-selecting books and
 * accepting an invalid TLS certificate. Touches no cache; the setup dialog
 * reads the result directly.
 * @returns A mutation taking `{ accountId, selectedBookRemoteIds?,
 *   acceptInvalidTls?, tlsDecision? }` and resolving to the contact source id
 *   with the discovered books.
 */
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
