import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiDelete, apiGet, apiPost, apiPut } from '@/shared/api'

export interface PgpKey {
  id: string
  fingerprint: string
  uid: string
  public_key_armored: string
  is_primary: boolean
  created_at: string
}

export interface PgpKeyBlob {
  id: string
  private_key_encrypted_blob: string
}

export interface ContactKey {
  id: string
  email: string
  public_key_data: string
  source: string
  fingerprint: string
  fetched_at: string
}

export interface DiscoveryResponse {
  found: boolean
  key: ContactKey | null
  local: boolean
  wkd_enabled: boolean
  keyserver_enabled: boolean
}

export interface CreatePgpKeyInput {
  fingerprint: string
  uid: string
  public_key_armored: string
  private_key_encrypted_blob: string
  is_primary?: boolean
}

export function usePgpKeys() {
  return useQuery({
    queryKey: ['pgp-keys'],
    queryFn: () => apiGet<PgpKey[]>('/pgp-keys'),
  })
}

export function useCreatePgpKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: CreatePgpKeyInput) => apiPost<PgpKey>('/pgp-keys', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pgp-keys'] }),
  })
}

export function useDeletePgpKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/pgp-keys/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pgp-keys'] }),
  })
}

export function useSetPrimaryPgpKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPut<PgpKey>(`/pgp-keys/${id}/primary`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pgp-keys'] }),
  })
}

export function usePgpKeyBlob(id?: string) {
  return useQuery({
    queryKey: ['pgp-key-blob', id],
    queryFn: () => apiGet<PgpKeyBlob>(`/pgp-keys/${id}/blob`),
    enabled: Boolean(id),
  })
}

export function useCreateContactKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: { email: string; public_key_data: string; fingerprint?: string }) =>
      apiPost<ContactKey>('/contact-keys', data),
    onSuccess: (_data, variables) => {
      qc.invalidateQueries({ queryKey: ['contact-keys', variables.email.toLowerCase()] })
      qc.invalidateQueries({ queryKey: ['key-discovery'] })
    },
  })
}

export function useContactKeys(email?: string) {
  const normalized = email?.trim().toLowerCase()
  return useQuery({
    queryKey: ['contact-keys', normalized],
    queryFn: () => apiGet<ContactKey[]>(`/contact-keys?email=${encodeURIComponent(normalized ?? '')}`),
    enabled: Boolean(normalized),
  })
}

export function useDiscoverKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (email: string) =>
      apiGet<DiscoveryResponse>(`/keys/discover?email=${encodeURIComponent(email.trim().toLowerCase())}`),
    onSuccess: (data, email) => {
      qc.setQueryData(['key-discovery', email.trim().toLowerCase()], data)
      if (data.key) qc.invalidateQueries({ queryKey: ['contact-keys', email.trim().toLowerCase()] })
    },
  })
}
