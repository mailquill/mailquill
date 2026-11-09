import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiDelete, apiGet, apiPatch, apiPost } from '@/shared/api'
import type { AllowedImageSender, BrandEntry, PublicConfig, Settings } from '@/shared/types'

/**
 * Server feature flags that apply before login (registration open, remote
 * image proxy) from `GET /config/public`, cached under `['public-config']`.
 * Refetched on every mount and window focus so a toggled flag shows up
 * without a reload.
 * @returns A query for the `PublicConfig`.
 */
export function usePublicConfig() {
  return useQuery({
    queryKey: ['public-config'],
    queryFn: () => apiGet<PublicConfig>('/config/public'),
    staleTime: 30_000,
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })
}

/**
 * The user's account-wide preferences from `GET /settings`, cached under `['settings']`.
 * @returns A query for the `Settings` record.
 */
export function useSettings() {
  return useQuery({
    queryKey: ['settings'],
    queryFn: () => apiGet<Settings>('/settings'),
  })
}

/**
 * Save changed preferences through `PATCH /settings`, then refetch `['settings']`.
 * @returns A mutation taking a `Partial<Settings>` and resolving to the full updated `Settings`.
 */
export function useUpdateSettings() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (settings: Partial<Settings>) => apiPatch<Settings>('/settings', settings),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['settings'] }),
  })
}

/**
 * Senders whose remote images load automatically, from
 * `GET /settings/image-allowlist`, cached under `['image-allowlist']`.
 * @returns A query for the `AllowedImageSender[]`.
 */
export function useImageAllowlist() {
  return useQuery({
    queryKey: ['image-allowlist'],
    queryFn: () => apiGet<AllowedImageSender[]>('/settings/image-allowlist'),
  })
}

/**
 * Always load remote images from a sender through `POST /settings/image-allowlist`,
 * then refetch `['image-allowlist']`.
 * @returns A mutation taking the sender address.
 */
export function useAddAllowedImageSender() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (sender: string) => apiPost('/settings/image-allowlist', { sender }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['image-allowlist'] }),
  })
}

/**
 * Discard every stored phishing verdict through `POST /settings/phishing/reset`
 * so messages are analysed again. Verdicts are embedded in list rows and
 * message details, so the unified, folder, message and thread queries are all
 * refetched.
 * @returns A mutation taking no variables.
 */
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

/**
 * User-maintained brand-to-domain pairs the phishing filter trusts, from
 * `GET /settings/brands`, cached under `['brand-entries']`.
 * @returns A query for the `BrandEntry[]`.
 */
export function useBrandEntries() {
  return useQuery({
    queryKey: ['brand-entries'],
    queryFn: () => apiGet<BrandEntry[]>('/settings/brands'),
  })
}

/**
 * Add a trusted brand domain through `POST /settings/brands`, then refetch `['brand-entries']`.
 * @returns A mutation taking `{ domain, brand_name }` and resolving to the stored `BrandEntry`.
 */
export function useAddBrandEntry() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (entry: { domain: string; brand_name: string }) => apiPost<BrandEntry>('/settings/brands', entry),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['brand-entries'] }),
  })
}

/**
 * Remove a trusted brand domain through `DELETE /settings/brands/:id`, then
 * refetch `['brand-entries']`.
 * @returns A mutation taking the entry id.
 */
export function useDeleteBrandEntry() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: string) => apiDelete(`/settings/brands/${id}`),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['brand-entries'] }),
  })
}

/**
 * Stop auto-loading a sender's remote images through
 * `DELETE /settings/image-allowlist/:sender`, then refetch `['image-allowlist']`.
 * @returns A mutation taking the sender address.
 */
export function useRemoveAllowedImageSender() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (sender: string) => apiDelete(`/settings/image-allowlist/${encodeURIComponent(sender)}`),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['image-allowlist'] }),
  })
}
