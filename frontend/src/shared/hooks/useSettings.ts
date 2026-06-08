import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPatch } from '@/shared/api'
import type { Settings } from '@/shared/types'

export function useSettings() {
  return useQuery({
    queryKey: ['settings'],
    queryFn: () => apiGet<Settings>('/settings'),
  })
}

export function useUpdateSettings() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (settings: Partial<Settings>) => apiPatch<Settings>('/settings', settings),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['settings'] }),
  })
}
