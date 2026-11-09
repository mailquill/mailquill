import {
  keepPreviousData,
  useInfiniteQuery,
  useQuery,
  useMutation,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query'
import { toast } from 'sonner'
import i18n from '@/shared/i18n'
import { apiGet, apiPost, apiPatch, apiDelete } from '@/shared/api'
import type { Message, SaveDraftInput, SendMessageInput } from '@/shared/types'
import type { UnifiedCounts, UnifiedView } from '@/shared/lib/unifiedViews'
import {
  optimisticMarkRead,
  optimisticMarkThreadRead,
  optimisticRemove,
  optimisticToggleFlag,
} from '@/shared/lib/mailListCache'

/**
 * Infinite, cursor-paged message list for one cross-account view (inbox,
 * starred, sent, drafts, archive, spam or trash) from `GET /mailbox/unified`.
 * Cached under `['unified', view, accountId, unread]`; the pages are flattened
 * so consumers keep reading `data.messages` and `data.total`.
 * @param view - Which unified view to list; defaults to the inbox.
 * @param accountId - Restrict the view to one account; omit for all accounts.
 * @param unread - Only list unread messages.
 * @returns An infinite query whose data is `{ messages, total }` over all loaded pages.
 */
export function useUnifiedInbox(view: UnifiedView = 'inbox', accountId?: string, unread = false) {
  return useInfiniteQuery({
    queryKey: ['unified', view, accountId ?? null, unread],
    queryFn: ({ pageParam }) => {
      const params = new URLSearchParams()
      if (view !== 'inbox') params.set('view', view)
      if (accountId) params.set('account_id', accountId)
      if (unread) params.set('unread', 'true')
      if (pageParam) params.set('cursor', pageParam)
      const qs = params.toString()
      return apiGet<{ messages?: ApiMessage[]; items?: ApiMessage[]; total?: number; next_cursor: string | null }>(
        `/mailbox/unified${qs ? `?${qs}` : ''}`,
      ).then(normalizeMessagePage)
    },
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    // Switching view, account scope or the unread filter keeps the previous
    // rows on screen until the new page arrives instead of blanking the list.
    placeholderData: keepPreviousData,
    // Flatten the pages so consumers keep reading `data.messages`.
    select: (data) => ({
      messages: data.pages.flatMap((p) => p.messages),
      total: data.pages[0]?.total,
    }),
  })
}

/**
 * Badge counts per unified view from `GET /mailbox/unified/counts`, cached
 * under `['unified-counts']`. Every read, flag, move and delete mutation in
 * this module invalidates it alongside the lists.
 * @returns A query for the `UnifiedCounts` record.
 */
export function useUnifiedCounts() {
  return useQuery({
    queryKey: ['unified-counts'],
    queryFn: () => apiGet<UnifiedCounts>('/mailbox/unified/counts'),
  })
}

/**
 * Infinite, cursor-paged list of one folder's messages from
 * `GET /accounts/:id/folders/:folder/messages`, cached under
 * `['folder-messages', accountId, folder, unread]`. Disabled until both the
 * account and the folder are known.
 * @param accountId - Owning account.
 * @param folder - Full folder path; URL-encoded for the request.
 * @param unread - Only list unread messages.
 * @returns An infinite query whose data is `{ messages, total }` over all loaded pages.
 */
export function useFolderMessages(accountId: string, folder: string, unread = false) {
  return useInfiniteQuery({
    queryKey: ['folder-messages', accountId, folder, unread],
    queryFn: ({ pageParam }) => {
      const params = new URLSearchParams()
      if (unread) params.set('unread', 'true')
      if (pageParam) params.set('cursor', pageParam)
      const qs = params.toString()
      return apiGet<{ messages?: ApiMessage[]; items?: ApiMessage[]; total?: number; next_cursor: string | null }>(
        `/accounts/${accountId}/folders/${encodeURIComponent(folder)}/messages${qs ? `?${qs}` : ''}`,
      ).then(normalizeMessagePage)
    },
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    placeholderData: keepPreviousData,
    select: (data) => ({
      messages: data.pages.flatMap((p) => p.messages),
      total: data.pages[0]?.total,
    }),
    enabled: !!(accountId && folder),
  })
}

/**
 * One message with its full body from `GET /messages/:id`, cached under
 * `['message', id]`, the same key {@link prefetchMessage} warms.
 * @param id - Message id; the query stays disabled while it is empty.
 * @returns A query for the `Message`.
 */
export function useMessage(id: string) {
  return useQuery({
    queryKey: ['message', id],
    queryFn: () => apiGet<Message>(`/messages/${id}`),
    enabled: !!id,
  })
}

/**
 * Warm the detail cache before a message is opened in the reading pane.
 * @param queryClient - The query client holding the `['message', id]` cache.
 * @param id - Message to prefetch.
 * @returns A promise that settles once the prefetch finishes; it never rejects.
 */
export function prefetchMessage(queryClient: QueryClient, id: string) {
  return queryClient.prefetchQuery({
    queryKey: ['message', id],
    queryFn: () => apiGet<Message>(`/messages/${id}`),
    staleTime: 30_000,
  })
}

/**
 * All messages of a conversation from `GET /threads/:id`, cached under
 * `['thread', threadId]`. The reading pane renders its collapsed cards from it.
 * @param threadId - Thread id; the query stays disabled while it is empty.
 * @returns A query for `{ messages }`.
 */
export function useThread(threadId: string) {
  return useQuery({
    queryKey: ['thread', threadId],
    queryFn: () => apiGet<{ messages: Message[] }>(`/threads/${threadId}`),
    enabled: !!threadId,
  })
}

// A failed list action rolls its optimistic change back, which on its own looks
// exactly like a list that refused to refresh. Say what happened instead.
function reportActionError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  toast.error(i18n.t('mail.actionFailed', { error: message }))
}

// Sidebar badges read from ['folders'] and ['unified-counts'], so every mutation
// that changes read/flag state or message location must refresh them alongside
// the message lists.
function invalidateMailLists(qc: QueryClient) {
  qc.invalidateQueries({ queryKey: ['unified'] })
  qc.invalidateQueries({ queryKey: ['folder-messages'] })
  qc.invalidateQueries({ queryKey: ['folders'] })
  qc.invalidateQueries({ queryKey: ['unified-counts'] })
}

// The reading pane renders collapsed cards from the thread query, so a flag or
// read-state change has to refresh it too or the cards show the old state.
function invalidateMessageViews(qc: QueryClient, id: string) {
  qc.invalidateQueries({ queryKey: ['message', id] })
  qc.invalidateQueries({ queryKey: ['thread'] })
  invalidateMailLists(qc)
}

/**
 * Mark one message read or unread through `PATCH /messages/:id/read`. The
 * change is applied optimistically to every cached list and the unread badges,
 * rolled back with an error toast on failure, and the message, thread, list
 * and count queries are invalidated once it settles.
 * @returns A mutation taking `{ id, is_read }`.
 */
export function useMarkRead() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, is_read }: { id: string; is_read: boolean }) =>
      apiPatch(`/messages/${id}/read`, { is_read }),
    onMutate: ({ id, is_read }) => optimisticMarkRead(qc, id, is_read),
    onError: (error, _vars, restore) => {
      restore?.()
      reportActionError(error)
    },
    onSettled: (_data, _err, { id }) => invalidateMessageViews(qc, id),
  })
}

/**
 * Star or unstar one message through `PATCH /messages/:id/flag`. The flag is
 * set optimistically in every cached list (unflagging drops the row from the
 * starred view), rolled back with an error toast on failure, and the message,
 * thread, list and count queries are invalidated once it settles.
 * @returns A mutation taking `{ id, is_flagged }`.
 */
export function useToggleFlag() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, is_flagged }: { id: string; is_flagged: boolean }) =>
      apiPatch(`/messages/${id}/flag`, { is_flagged }),
    onMutate: ({ id, is_flagged }) => optimisticToggleFlag(qc, id, is_flagged),
    onError: (error, _vars, restore) => {
      restore?.()
      reportActionError(error)
    },
    onSettled: (_data, _err, { id }) => invalidateMessageViews(qc, id),
  })
}

/**
 * Archive one message through `POST /messages/:id/archive`. The row leaves
 * every cached list at once and returns with an error toast if the request
 * fails; lists, folders and counts are invalidated once it settles.
 * @returns A mutation taking the message id.
 */
export function useArchiveMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/archive`),
    onMutate: (id) => optimisticRemove(qc, (m) => m.id === id),
    onError: (error, _id, restore) => {
      restore?.()
      reportActionError(error)
    },
    onSettled: () => invalidateMailLists(qc),
  })
}

/**
 * Move one message to another folder through `POST /messages/:id/move`. The
 * row leaves every cached list at once and returns with an error toast if the
 * request fails; lists, folders and counts are invalidated once it settles.
 * @returns A mutation taking `{ id, folder_id }`.
 */
export function useMoveMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, folder_id }: { id: string; folder_id: string }) =>
      apiPost(`/messages/${id}/move`, { folder_id }),
    onMutate: ({ id }) => optimisticRemove(qc, (m) => m.id === id),
    onError: (error, _vars, restore) => {
      restore?.()
      reportActionError(error)
    },
    onSettled: () => invalidateMailLists(qc),
  })
}

/**
 * Rescue a message from the spam view through `POST /messages/:id/not-spam`.
 * The row leaves every cached list at once (it moves back to the inbox) and
 * returns with an error toast on failure; thread, list and count queries are
 * invalidated once it settles. For the in-place variant see
 * {@link useNotSpamInPlace}.
 * @returns A mutation taking the message id.
 */
export function useNotSpamMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/not-spam`),
    onMutate: (id) => optimisticRemove(qc, (m) => m.id === id),
    onError: (error, _id, restore) => {
      restore?.()
      reportActionError(error)
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ['thread'] })
      invalidateMailLists(qc)
    },
  })
}

/**
 * "Not spam" from the phishing banner: teaches the filter (sender becomes
 * trusted, verdicts clear server-side) while the message stays where it is —
 * so no optimistic list removal, unlike {@link useNotSpamMessage}. The message,
 * thread, list and count queries are invalidated once it settles.
 * @returns A mutation taking the message id.
 */
export function useNotSpamInPlace() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/not-spam`),
    onSettled: (_data, _err, id) => invalidateMessageViews(qc, id),
  })
}

/**
 * Re-run the phishing analysis of one message through
 * `POST /messages/:id/reanalyse`, then refresh the message, thread, list and
 * count queries so the new verdict shows everywhere. No optimistic update.
 * @returns A mutation taking the message id.
 */
export function useReanalyseMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/reanalyse`),
    onSuccess: (_data, id) => invalidateMessageViews(qc, id),
  })
}

/**
 * Delete (or trash, server-side) one message through `DELETE /messages/:id`.
 * The row leaves every cached list at once and returns with an error toast on
 * failure; thread, list and count queries are invalidated once it settles.
 * @returns A mutation taking the message id.
 */
export function useDeleteMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/messages/${id}`),
    onMutate: (id) => optimisticRemove(qc, (m) => m.id === id),
    onError: (error, _id, restore) => {
      restore?.()
      reportActionError(error)
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ['thread'] })
      invalidateMailLists(qc)
    },
  })
}

/**
 * Which messages a server-side bulk action covers. Mirrors the list that is
 * open: a unified view, optionally narrowed to one account, or a single folder.
 * @property view - Unified view name, for example `inbox` or `starred`.
 * @property accountId - Restrict the action to this account.
 * @property folder - Full folder path when acting on one folder.
 */
export interface BulkScope {
  view?: string
  accountId?: string
  folder?: string
}

/**
 * Server-side "select all" through `POST /mailbox/bulk`: acts on a whole
 * view/folder without the client enumerating ids. Use for the "select all N
 * conversations" path. There is no optimistic update; a failure shows an error
 * toast and the thread, list and count queries are invalidated once it settles.
 * @returns A mutation taking `{ action, view?, accountId?, folder? }` where
 *   `action` is `read`, `archive`, `delete` or `flag`.
 */
export function useBulkAction() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      action,
      view,
      accountId,
      folder,
    }: { action: 'read' | 'archive' | 'delete' | 'flag' } & BulkScope) =>
      apiPost('/mailbox/bulk', { action, view, account_id: accountId, folder }),
    onError: reportActionError,
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ['thread'] })
      invalidateMailLists(qc)
    },
  })
}

/**
 * Archive a whole conversation through `POST /threads/:id/archive`. Every row
 * of the thread leaves the cached lists at once and returns with an error
 * toast on failure; lists, folders and counts are invalidated once it settles.
 * @returns A mutation taking the thread id.
 */
export function useArchiveThread() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (threadId: string) => apiPost(`/threads/${threadId}/archive`),
    onMutate: (threadId) => optimisticRemove(qc, (m) => m.thread_id === threadId),
    onError: (error, _id, restore) => {
      restore?.()
      reportActionError(error)
    },
    onSettled: () => invalidateMailLists(qc),
  })
}

/**
 * Delete a whole conversation through `POST /threads/:id/delete`. Every row
 * of the thread leaves the cached lists at once and returns with an error
 * toast on failure; lists, folders and counts are invalidated once it settles.
 * @returns A mutation taking the thread id.
 */
export function useDeleteThread() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (threadId: string) => apiPost(`/threads/${threadId}/delete`),
    onMutate: (threadId) => optimisticRemove(qc, (m) => m.thread_id === threadId),
    onError: (error, _id, restore) => {
      restore?.()
      reportActionError(error)
    },
    onSettled: () => invalidateMailLists(qc),
  })
}

/**
 * Mark every message of a conversation read or unread through
 * `PATCH /threads/:id/read`. The rows flip optimistically in every cached list
 * together with the unread badges, roll back with an error toast on failure,
 * and the thread, list and count queries are invalidated once it settles.
 * @returns A mutation taking `{ threadId, is_read }`.
 */
export function useMarkThreadRead() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ threadId, is_read }: { threadId: string; is_read: boolean }) =>
      apiPatch(`/threads/${threadId}/read`, { is_read }),
    onMutate: ({ threadId, is_read }) => optimisticMarkThreadRead(qc, threadId, is_read),
    onError: (error, _vars, restore) => {
      restore?.()
      reportActionError(error)
    },
    onSettled: (_data, _err, { threadId }) => {
      qc.invalidateQueries({ queryKey: ['thread', threadId] })
      invalidateMailLists(qc)
    },
  })
}

/**
 * Infinite, cursor-paged search through `GET /search`, cached under
 * `['search', query, filters]`. Runs only once the query is longer than one
 * character or at least one filter is set.
 * @param query - Free-text search terms, sent as `q`.
 * @param filters - Extra query-string parameters passed through verbatim.
 * @returns An infinite query whose data is `{ messages }` over all loaded pages.
 */
export function useSearchMessages(query: string, filters?: Record<string, string>) {
  const hasFilters = !!filters && Object.keys(filters).length > 0
  return useInfiniteQuery({
    queryKey: ['search', query, filters],
    queryFn: ({ pageParam }) => {
      const params = new URLSearchParams(query ? { q: query, ...filters } : { ...filters })
      if (pageParam) params.set('cursor', pageParam)
      return apiGet<{ messages?: ApiMessage[]; items?: ApiMessage[]; next_cursor: string | null }>(`/search?${params}`)
        .then(normalizeMessagePage)
    },
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    select: (data) => ({
      messages: data.pages.flatMap((page) => page.messages),
    }),
    enabled: query.trim().length > 1 || hasFilters,
  })
}

/**
 * Queue an outgoing message through `POST /send`. Success only means the
 * server accepted it; the SSE `send` event later reports delivery and refreshes
 * the mail lists, so this mutation touches no cache itself.
 * @returns A mutation taking a `SendMessageInput` and resolving to
 *   `{ send_id, status: 'queued' }`.
 */
export function useSendMessage() {
  return useMutation({
    mutationFn: (message: SendMessageInput) =>
      apiPost<{ send_id: string; status: 'queued' }>('/send', message),
    // The SSE "send" event refreshes mail lists once provider delivery has
    // actually finished. The mutation only acknowledges queue acceptance.
  })
}

/**
 * Persist a provider-neutral local draft through `POST /drafts` and refresh the
 * mail lists, folders and counts so the drafts view picks it up.
 * @returns A mutation taking a `SaveDraftInput` and resolving to the draft `{ id }`.
 */
export function useSaveDraft() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (draft: SaveDraftInput) => apiPost<{ id: string }>('/drafts', draft),
    onSuccess: () => invalidateMailLists(qc),
  })
}

/** Wire shape of a list row: older endpoints send `message_id` instead of `id`. */
type ApiMessage = Message & { message_id?: string }

/** Accept both `messages` and `items` page shapes and guarantee every row has an `id`. */
function normalizeMessagePage(page: {
  messages?: ApiMessage[]
  items?: ApiMessage[]
  total?: number
  next_cursor: string | null
}): { messages: Message[]; total?: number; next_cursor: string | null } {
  const source = page.messages ?? page.items ?? []

  return {
    messages: source.map((message) => ({
      ...message,
      id: message.id ?? message.message_id ?? '',
    })),
    total: page.total,
    next_cursor: page.next_cursor,
  }
}
