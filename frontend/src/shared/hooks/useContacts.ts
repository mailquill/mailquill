import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/shared/api'
import type { Contact, ContactAccount, ContactBook, ContactGroupSummary, ContactPage, NewContact, RecipientSuggestion } from '@/shared/types'

export function useContacts(accountId?: string, q?: string, mailboxId?: string, bookId?: string, groupId?: string) {
  const params = new URLSearchParams()
  if (accountId) params.set('account_id', accountId)
  if (q) params.set('q', q)
  if (mailboxId) params.set('mailbox_id', mailboxId)
  if (bookId) params.set('book_id', bookId)
  if (groupId) params.set('group_id', groupId)
  const qs = params.toString()
  return useQuery({
    queryKey: ['contacts', accountId ?? '', q ?? '', mailboxId ?? '', bookId ?? '', groupId ?? ''],
    queryFn: () => apiGet<ContactPage>(`/contacts${qs ? `?${qs}` : ''}`),
    select: (page) => page.items,
  })
}

/** Loads contact groups and membership counts for the active source filters.
 * @param accountId - Optional contact source identifier.
 * @param mailboxId - Optional mailbox identifier.
 * @param bookId - Optional local contact book identifier.
 * @returns A query containing resolved contact groups.
 */
export function useContactGroups(accountId?: string, mailboxId?: string, bookId?: string) {
  const params = new URLSearchParams()
  if (accountId) params.set('account_id', accountId)
  if (mailboxId) params.set('mailbox_id', mailboxId)
  if (bookId) params.set('book_id', bookId)
  const qs = params.toString()
  return useQuery({
    queryKey: ['contact-groups', accountId ?? '', mailboxId ?? '', bookId ?? ''],
    queryFn: () => apiGet<ContactGroupSummary[]>(`/contact-groups${qs ? `?${qs}` : ''}`),
  })
}

export function useContactAccounts() {
  return useQuery({
    queryKey: ['contact-accounts'],
    queryFn: () => apiGet<ContactAccount[]>('/contact-accounts'),
  })
}

export function useContactSearch(q: string, mailboxId?: string) {
  const params = new URLSearchParams({ q })
  if (mailboxId) params.set('mailbox_id', mailboxId)
  return useQuery({
    queryKey: ['contacts-search', q, mailboxId ?? ''],
    queryFn: () => apiGet<Contact[]>(`/contacts/search?${params}`),
    enabled: q.trim().length >= 2,
  })
}

/** Searches synchronized contacts and addresses seen on incoming messages.
 * @param q - Name or email fragment entered by the user.
 * @param mailboxId - Optional mailbox used to prioritize familiar senders.
 * @returns A deduplicated recipient suggestion query.
 */
export function useRecipientSuggestions(q: string, mailboxId?: string) {
  const params = new URLSearchParams({ q })
  if (mailboxId) params.set('mailbox_id', mailboxId)
  return useQuery({
    queryKey: ['recipient-suggestions', q, mailboxId ?? ''],
    queryFn: () => apiGet<RecipientSuggestion[]>(`/recipient-suggestions?${params}`),
    enabled: q.trim().length >= 1,
  })
}

export function useCreateContactAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: {
      display_name: string
      type: 'cardav'
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

export function useContactBooks(accountId?: string) {
  return useQuery({
    queryKey: ['contact-books', accountId ?? ''],
    queryFn: () => apiGet<ContactBook[]>(`/contact-accounts/${accountId}/books`),
    enabled: Boolean(accountId),
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
