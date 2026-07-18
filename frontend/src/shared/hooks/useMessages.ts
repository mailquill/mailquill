import {
  useInfiniteQuery,
  useQuery,
  useMutation,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query'
import { apiGet, apiPost, apiPatch, apiDelete } from '@/shared/api'
import type { Message, SendMessageInput } from '@/shared/types'
import type { UnifiedCounts, UnifiedView } from '@/shared/lib/unifiedViews'

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

export function useThread(threadId: string) {
  return useQuery({
    queryKey: ['thread', threadId],
    queryFn: () => apiGet<{ messages: Message[] }>(`/threads/${threadId}`),
    enabled: !!threadId,
  })
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

// Raw cache shape of the infinite mail-list queries (before `select` flattens).
type ListPage = { messages: Message[]; total?: number; next_cursor: string | null }
type InfiniteList = { pages: ListPage[]; pageParams: unknown[] }

// Unified views whose sidebar badge is an *unread* count. `starred` is special:
// its badge counts flagged messages, not unread ones.
const UNREAD_VIEWS: UnifiedView[] = ['inbox', 'sent', 'drafts', 'archive', 'spam', 'trash']

function unifiedViewOfKey(key: unknown): UnifiedView | null {
  const k = key as unknown[]
  return k[0] === 'unified' ? (k[1] as UnifiedView) : null
}

// Optimistically drop a message's contribution to the cross-account sidebar
// badges when it leaves a view (delete). We only touch counts the backend
// confirms synchronously — the *source* unread drop and the `starred` (flagged)
// drop — so the follow-up refetch agrees and nothing flickers. The destination
// rise (e.g. Trash) is intentionally left to the next sync, because the backend
// can't reflect it until the queued IMAP move lands.
function adjustCountsForRemoval(qc: QueryClient, id: string): () => void {
  const prev = qc.getQueryData<UnifiedCounts>(['unified-counts'])
  if (!prev) return () => {}
  const next = { ...prev }
  for (const [key, data] of qc.getQueriesData<InfiniteList>({ queryKey: ['unified'] })) {
    const view = unifiedViewOfKey(key)
    if (!view || !data) continue
    const m = data.pages.flatMap((p) => p.messages).find((x) => x.id === id)
    if (!m) continue
    if (view === 'starred') next.starred = Math.max(0, next.starred - 1)
    else if (UNREAD_VIEWS.includes(view) && !m.is_read) next[view] = Math.max(0, next[view] - 1)
  }
  qc.setQueryData(['unified-counts'], next)
  return () => qc.setQueryData(['unified-counts'], prev)
}

// Optimistically flip a message's read state in every cached list and adjust the
// affected unread badges. `mark_read` refreshes the same counts server-side, so
// the reconciling refetch matches and there's no flicker.
function optimisticMarkRead(qc: QueryClient, id: string, isRead: boolean): () => void {
  const prevCounts = qc.getQueryData<UnifiedCounts>(['unified-counts'])
  const snaps = [
    ...qc.getQueriesData<InfiniteList>({ queryKey: ['unified'] }),
    ...qc.getQueriesData<InfiniteList>({ queryKey: ['folder-messages'] }),
  ]
  // Count deltas first, from the pre-flip read state.
  if (prevCounts) {
    const next = { ...prevCounts }
    for (const [key, data] of snaps) {
      const view = unifiedViewOfKey(key)
      if (!view || view === 'starred' || !UNREAD_VIEWS.includes(view) || !data) continue
      const m = data.pages.flatMap((p) => p.messages).find((x) => x.id === id)
      if (!m || m.is_read === isRead) continue
      next[view] = Math.max(0, next[view] + (isRead ? -1 : 1))
    }
    qc.setQueryData(['unified-counts'], next)
  }
  // Then flip is_read in the list caches so rows update immediately.
  for (const [key, data] of snaps) {
    if (!data) continue
    qc.setQueryData<InfiniteList>(key, {
      ...data,
      pages: data.pages.map((p) => ({
        ...p,
        messages: p.messages.map((m) => (m.id === id ? { ...m, is_read: isRead } : m)),
      })),
    })
  }
  return () => {
    if (prevCounts) qc.setQueryData(['unified-counts'], prevCounts)
    for (const [key, data] of snaps) qc.setQueryData(key, data)
  }
}

// Drop every row matching `match` from all cached mail lists (unified +
// per-folder) right away, so delete/archive/move feel instant instead of
// waiting for the server roundtrip and a full refetch. Returns a restore()
// that puts the snapshots back if the mutation fails.
async function removeFromMailLists(
  qc: QueryClient,
  match: (m: Message) => boolean,
): Promise<() => void> {
  await Promise.all([
    qc.cancelQueries({ queryKey: ['unified'] }),
    qc.cancelQueries({ queryKey: ['folder-messages'] }),
    qc.cancelQueries({ queryKey: ['unified-counts'] }),
  ])
  const snapshots = [
    ...qc.getQueriesData<InfiniteList>({ queryKey: ['unified'] }),
    ...qc.getQueriesData<InfiniteList>({ queryKey: ['folder-messages'] }),
  ]
  for (const [key, data] of snapshots) {
    if (!data) continue
    qc.setQueryData<InfiniteList>(key, {
      ...data,
      pages: data.pages.map((page) => {
        // `total` counts messages, but each removed row is a thread; decrement
        // by the thread's message count so the header stays accurate.
        const removed = page.messages
          .filter(match)
          .reduce((sum, m) => sum + (m.thread_size ?? 1), 0)
        return {
          ...page,
          messages: page.messages.filter((m) => !match(m)),
          total: page.total != null ? Math.max(0, page.total - removed) : page.total,
        }
      }),
    })
  }
  return () => {
    for (const [key, data] of snapshots) qc.setQueryData(key, data)
  }
}

export function useMarkRead() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, is_read }: { id: string; is_read: boolean }) =>
      apiPatch(`/messages/${id}/read`, { is_read }),
    onMutate: ({ id, is_read }) => optimisticMarkRead(qc, id, is_read),
    onError: (_e, _vars, restore) => restore?.(),
    onSettled: (_data, _err, { id }) => {
      qc.invalidateQueries({ queryKey: ['message', id] })
      invalidateMailLists(qc)
    },
  })
}

export function useToggleFlag() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, is_flagged }: { id: string; is_flagged: boolean }) =>
      apiPatch(`/messages/${id}/flag`, { is_flagged }),
    onSuccess: (_data, { id }) => {
      qc.invalidateQueries({ queryKey: ['message', id] })
      invalidateMailLists(qc)
    },
  })
}

export function useArchiveMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/archive`),
    onMutate: (id) => removeFromMailLists(qc, (m) => m.id === id),
    onError: (_e, _id, restore) => restore?.(),
    onSettled: () => invalidateMailLists(qc),
  })
}

export function useMoveMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, folder_id }: { id: string; folder_id: string }) =>
      apiPost(`/messages/${id}/move`, { folder_id }),
    onMutate: ({ id }) => removeFromMailLists(qc, (m) => m.id === id),
    onError: (_e, _vars, restore) => restore?.(),
    onSettled: () => invalidateMailLists(qc),
  })
}

export function useNotSpamMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/not-spam`),
    onMutate: (id) => removeFromMailLists(qc, (m) => m.id === id),
    onError: (_e, _id, restore) => restore?.(),
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ['thread'] })
      invalidateMailLists(qc)
    },
  })
}

export function useReanalyseMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/reanalyse`),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ['message', id] })
      qc.invalidateQueries({ queryKey: ['thread'] })
      invalidateMailLists(qc)
    },
  })
}

export function useDeleteMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/messages/${id}`),
    onMutate: async (id) => {
      // Counts first (reads the message from cache), then drop it from the lists.
      const restoreCounts = adjustCountsForRemoval(qc, id)
      const restoreList = await removeFromMailLists(qc, (m) => m.id === id)
      return () => {
        restoreList()
        restoreCounts()
      }
    },
    onError: (_e, _id, restore) => restore?.(),
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
    onMutate: (threadId) => removeFromMailLists(qc, (m) => m.thread_id === threadId),
    onError: (_e, _id, restore) => restore?.(),
    onSettled: () => invalidateMailLists(qc),
  })
}

export function useDeleteThread() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (threadId: string) => apiPost(`/threads/${threadId}/delete`),
    onMutate: (threadId) => removeFromMailLists(qc, (m) => m.thread_id === threadId),
    onError: (_e, _id, restore) => restore?.(),
    onSettled: () => invalidateMailLists(qc),
  })
}

export function useMarkThreadRead() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ threadId, is_read }: { threadId: string; is_read: boolean }) =>
      apiPatch(`/threads/${threadId}/read`, { is_read }),
    onSuccess: (_data, { threadId }) => {
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
