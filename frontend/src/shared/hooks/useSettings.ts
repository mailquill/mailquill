import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiDelete, apiGet, apiPatch, apiPost } from '@/shared/api'
import type { AllowedImageSender, BrandEntry, PublicConfig, Settings } from '@/shared/types'

export function usePublicConfig() {
  return useQuery({
    queryKey: ['public-config'],
    queryFn: () => apiGet<PublicConfig>('/config/public'),
    staleTime: 30_000,
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })
}

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

export function useBrandEntries() {
  return useQuery({
    queryKey: ['brand-entries'],
    queryFn: () => apiGet<BrandEntry[]>('/settings/brands'),
  })
}

export function useAddBrandEntry() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (entry: { domain: string; brand_name: string }) => apiPost<BrandEntry>('/settings/brands', entry),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['brand-entries'] }),
  })
}

export function useDeleteBrandEntry() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: string) => apiDelete(`/settings/brands/${id}`),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['brand-entries'] }),
  })
}

export function useRemoveAllowedImageSender() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (sender: string) => apiDelete(`/settings/image-allowlist/${encodeURIComponent(sender)}`),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['image-allowlist'] }),
  })
}
