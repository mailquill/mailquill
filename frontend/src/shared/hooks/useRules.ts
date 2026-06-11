import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/shared/api'
import type { Rule, RuleInput } from '@/shared/types'

export function useRules() {
  return useQuery({
    queryKey: ['rules'],
    queryFn: () => apiGet<Rule[]>('/rules'),
  })
}

export function useCreateRule() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: RuleInput) => apiPost<Rule>('/rules', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['rules'] }),
  })
}

export function useUpdateRule() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: RuleInput }) => apiPut<Rule>(`/rules/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['rules'] }),
  })
}

export function useDeleteRule() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/rules/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['rules'] }),
  })
}

/** Compile + upload the active Sieve script for an account via ManageSieve. */
export function useApplySieve() {
  return useMutation({
    mutationFn: (accountId: string) => apiPost<{ uploaded: boolean; rules: number }>(`/accounts/${accountId}/apply-sieve`),
  })
}
