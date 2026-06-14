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

export function useUnifiedInbox(view: UnifiedView = 'inbox') {
  return useInfiniteQuery({
    queryKey: ['unified', view],
    queryFn: ({ pageParam }) => {
      const params = new URLSearchParams()
      if (view !== 'inbox') params.set('view', view)
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
    refetchInterval: 30_000,
  })
}

export function useFolderMessages(accountId: string, folder: string) {
  return useInfiniteQuery({
    queryKey: ['folder-messages', accountId, folder],
    queryFn: ({ pageParam }) =>
      apiGet<{ messages?: ApiMessage[]; items?: ApiMessage[]; total?: number; next_cursor: string | null }>(
        `/accounts/${accountId}/folders/${encodeURIComponent(folder)}/messages${pageParam ? `?cursor=${pageParam}` : ''}`,
      ).then(normalizeMessagePage),
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
    onSuccess: (_data, { id }) => {
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

export function useDeleteMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/messages/${id}`),
    onMutate: (id) => removeFromMailLists(qc, (m) => m.id === id),
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
  const params = new URLSearchParams(query ? { q: query, ...filters } : { ...filters })
  const hasFilters = !!filters && Object.keys(filters).length > 0
  return useQuery({
    queryKey: ['search', query, filters],
    queryFn: () =>
      apiGet<{ messages?: ApiMessage[]; items?: ApiMessage[]; next_cursor: string | null }>(`/search?${params}`)
        .then(normalizeMessagePage),
    enabled: query.trim().length > 1 || hasFilters,
  })
}

export function useSendMessage() {
  const qc = useQueryClient()

  return useMutation({
    mutationFn: (message: SendMessageInput) => apiPost<{ message_id: string }>('/send', message),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['unified'] })
      qc.invalidateQueries({ queryKey: ['folder-messages'] })
    },
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
