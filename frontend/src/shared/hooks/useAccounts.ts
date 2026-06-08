import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
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
