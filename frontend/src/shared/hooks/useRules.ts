import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/shared/api'
import type { Rule, RuleInput } from '@/shared/types'

/**
 * The user's mail filter rules from `GET /rules`, cached under `['rules']`.
 * @returns A query for the `Rule[]`.
 */
export function useRules() {
  return useQuery({
    queryKey: ['rules'],
    queryFn: () => apiGet<Rule[]>('/rules'),
  })
}

/**
 * Create a filter rule through `POST /rules`, then refetch `['rules']`.
 * @returns A mutation taking a `RuleInput` and resolving to the stored `Rule`.
 */
export function useCreateRule() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: RuleInput) => apiPost<Rule>('/rules', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['rules'] }),
  })
}

/**
 * Replace a filter rule through `PUT /rules/:id`, then refetch `['rules']`.
 * @returns A mutation taking `{ id, data }` and resolving to the updated `Rule`.
 */
export function useUpdateRule() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: RuleInput }) => apiPut<Rule>(`/rules/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['rules'] }),
  })
}

/**
 * Delete a filter rule through `DELETE /rules/:id`, then refetch `['rules']`.
 * @returns A mutation taking the rule id.
 */
export function useDeleteRule() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/rules/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['rules'] }),
  })
}

/**
 * Compile + upload the active Sieve script for an account via ManageSieve,
 * through `POST /accounts/:id/apply-sieve`. Touches no cache.
 * @returns A mutation taking the account id and resolving to whether the script
 *   was uploaded and how many rules it contains.
 */
export function useApplySieve() {
  return useMutation({
    mutationFn: (accountId: string) => apiPost<{ uploaded: boolean; rules: number }>(`/accounts/${accountId}/apply-sieve`),
  })
}
