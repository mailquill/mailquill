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

export function useUnifiedCounts() {
  return useQuery({
    queryKey: ['unified-counts'],
    queryFn: () => apiGet<UnifiedCounts>('/mailbox/unified/counts'),
  })
}

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

export function useMessage(id: string) {
  return useQuery({
    queryKey: ['message', id],
    queryFn: () => apiGet<Message>(`/messages/${id}`),
    enabled: !!id,
  })
}

/** Warm the detail cache before a message is opened in the reading pane. */
export function prefetchMessage(queryClient: QueryClient, id: string) {
  return queryClient.prefetchQuery({
    queryKey: ['message', id],
    queryFn: () => apiGet<Message>(`/messages/${id}`),
    staleTime: 30_000,
  })
}

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
 * so no optimistic list removal, unlike {@link useNotSpamMessage}.
 */
export function useNotSpamInPlace() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/not-spam`),
    onSettled: (_data, _err, id) => invalidateMessageViews(qc, id),
  })
}

export function useReanalyseMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/reanalyse`),
    onSuccess: (_data, id) => invalidateMessageViews(qc, id),
  })
}

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

export interface BulkScope {
  view?: string
  accountId?: string
  folder?: string
}

// Server-side "select all": acts on a whole view/folder without the client
// enumerating ids. Use for the "select all N conversations" path.
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

export function useSendMessage() {
  return useMutation({
    mutationFn: (message: SendMessageInput) =>
      apiPost<{ send_id: string; status: 'queued' }>('/send', message),
    // The SSE "send" event refreshes mail lists once provider delivery has
    // actually finished. The mutation only acknowledges queue acceptance.
  })
}

/** Persist a provider-neutral local draft and refresh draft lists. */
export function useSaveDraft() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (draft: SaveDraftInput) => apiPost<{ id: string }>('/drafts', draft),
    onSuccess: () => invalidateMailLists(qc),
  })
}

type ApiMessage = Message & { message_id?: string }

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
