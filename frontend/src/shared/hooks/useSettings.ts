import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiDelete, apiGet, apiPatch, apiPost } from '@/shared/api'
import type { AllowedImageSender, Settings } from '@/shared/types'

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

export function useImageAllowlist() {
  return useQuery({
    queryKey: ['image-allowlist'],
    queryFn: () => apiGet<AllowedImageSender[]>('/settings/image-allowlist'),
  })
}

export function useAddAllowedImageSender() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (sender: string) => apiPost('/settings/image-allowlist', { sender }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['image-allowlist'] }),
  })
}

export function useResetPhishingAnalysis() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => apiPost('/settings/phishing/reset'),
    onSuccess: () => {
      // Verdicts are embedded in list rows and message details.
      queryClient.invalidateQueries({ queryKey: ['unified'] })
      queryClient.invalidateQueries({ queryKey: ['folder-messages'] })
      queryClient.invalidateQueries({ queryKey: ['message'] })
      queryClient.invalidateQueries({ queryKey: ['thread'] })
    },
  })
}

export function useRemoveAllowedImageSender() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (sender: string) => apiDelete(`/settings/image-allowlist/${encodeURIComponent(sender)}`),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['image-allowlist'] }),
  })
}
