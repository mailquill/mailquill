import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/shared/api'
import type { Contact, ContactAccount, NewContact } from '@/shared/types'

export function useContacts(accountId?: string, q?: string) {
  const params = new URLSearchParams()
  if (accountId) params.set('account_id', accountId)
  if (q) params.set('q', q)
  const qs = params.toString()
  return useQuery({
    queryKey: ['contacts', accountId ?? '', q ?? ''],
    queryFn: () => apiGet<Contact[]>(`/contacts${qs ? `?${qs}` : ''}`),
  })
}

export function useContactAccounts() {
  return useQuery({
    queryKey: ['contact-accounts'],
    queryFn: () => apiGet<ContactAccount[]>('/contact-accounts'),
  })
}

export function useContactSearch(q: string) {
  return useQuery({
    queryKey: ['contacts-search', q],
    queryFn: () => apiGet<Contact[]>(`/contacts/search?q=${encodeURIComponent(q)}`),
    enabled: q.trim().length >= 2,
  })
}

export function useCreateContactAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: {
      display_name: string
      type: 'cardav' | 'graph' | 'google'
      base_url?: string | null
      auth_scheme?: 'basic' | 'oauth2'
      username?: string | null
      password?: string | null
      access_token?: string | null
      refresh_token?: string | null
    }) => apiPost<ContactAccount>('/contact-accounts', data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['contact-accounts'] })
      qc.invalidateQueries({ queryKey: ['contacts'] })
    },
  })
}

export function useDeleteContactAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/contact-accounts/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['contact-accounts'] })
      qc.invalidateQueries({ queryKey: ['contacts'] })
    },
  })
}

export function useSyncContactAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/contact-accounts/${id}/sync`),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ['contact-accounts'] })
      qc.invalidateQueries({ queryKey: ['contacts'] })
      qc.invalidateQueries({ queryKey: ['contacts', id] })
    },
  })
}

export function useCreateContact() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: NewContact) => apiPost<Contact>('/contacts', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['contacts'] }),
  })
}

export function useUpdateContact() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<NewContact> }) => apiPut<Contact>(`/contacts/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['contacts'] }),
  })
}

export function useDeleteContact() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/contacts/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['contacts'] }),
  })
}
