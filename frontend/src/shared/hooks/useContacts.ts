import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/shared/api'
import type { Contact, ContactAccount, ContactBook, ContactGroupSummary, ContactPage, NewContact, RecipientSuggestion } from '@/shared/types'

/**
 * Filtered contact list from `GET /contacts`, cached under
 * `['contacts', accountId, q, mailboxId, bookId, groupId]` (empty strings for
 * unset filters) so the `['contacts']` prefix covers every variant. Only the
 * page's `items` are exposed.
 * @param accountId - Restrict to one contact source.
 * @param q - Free-text filter on name and address.
 * @param mailboxId - Restrict to contacts synced from one mail account.
 * @param bookId - Restrict to one address book.
 * @param groupId - Restrict to members of one contact group.
 * @returns A query whose data is the `Contact[]` of the first page.
 */
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

/**
 * Standalone CardDAV contact accounts from `GET /contact-accounts`, cached
 * under `['contact-accounts']`.
 * @returns A query for the `ContactAccount[]`.
 */
export function useContactAccounts() {
  return useQuery({
    queryKey: ['contact-accounts'],
    queryFn: () => apiGet<ContactAccount[]>('/contact-accounts'),
  })
}

/**
 * Type-ahead contact search through `GET /contacts/search`, cached under
 * `['contacts-search', q, mailboxId]`. Runs only from two characters on.
 * @param q - Name or email fragment entered by the user.
 * @param mailboxId - Optional mail account used to rank its contacts first.
 * @returns A query for the matching `Contact[]`.
 */
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

/**
 * Connect a CardDAV account through `POST /contact-accounts`, then refetch
 * contact accounts and contacts.
 * @returns A mutation taking the connection form payload and resolving to the
 *   new `ContactAccount`.
 */
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

/**
 * Address books of one contact account from `GET /contact-accounts/:id/books`,
 * cached under `['contact-books', accountId]`.
 * @param accountId - Contact account to list; the query stays disabled while unset.
 * @returns A query for the `ContactBook[]`.
 */
export function useContactBooks(accountId?: string) {
  return useQuery({
    queryKey: ['contact-books', accountId ?? ''],
    queryFn: () => apiGet<ContactBook[]>(`/contact-accounts/${accountId}/books`),
    enabled: Boolean(accountId),
  })
}

/**
 * Disconnect a contact account through `DELETE /contact-accounts/:id`, then
 * refetch contact accounts and contacts.
 * @returns A mutation taking the contact account id.
 */
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

/**
 * Run a sync for one contact account now through `POST /contact-accounts/:id/sync`,
 * then refetch contact accounts and every contacts query.
 * @returns A mutation taking the contact account id.
 */
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

/**
 * Create a contact through `POST /contacts`, then refetch every contacts query.
 * @returns A mutation taking a `NewContact` and resolving to the stored `Contact`.
 */
export function useCreateContact() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: NewContact) => apiPost<Contact>('/contacts', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['contacts'] }),
  })
}

/**
 * Update a contact through `PUT /contacts/:id`, then refetch every contacts query.
 * @returns A mutation taking `{ id, data }` with the changed fields and
 *   resolving to the updated `Contact`.
 */
export function useUpdateContact() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<NewContact> }) => apiPut<Contact>(`/contacts/${id}`, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['contacts'] }),
  })
}

/**
 * Delete a contact through `DELETE /contacts/:id`, then refetch every contacts query.
 * @returns A mutation taking the contact id.
 */
export function useDeleteContact() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/contacts/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['contacts'] }),
  })
}
