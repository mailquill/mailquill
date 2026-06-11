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
    onSuccess: () => invalidateMailLists(qc),
  })
}

export function useMoveMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, folder_id }: { id: string; folder_id: string }) =>
      apiPost(`/messages/${id}/move`, { folder_id }),
    onSuccess: () => invalidateMailLists(qc),
  })
}

export function useDeleteMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/messages/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['thread'] })
      invalidateMailLists(qc)
    },
  })
}

export function useArchiveThread() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (threadId: string) => apiPost(`/threads/${threadId}/archive`),
    onSuccess: () => invalidateMailLists(qc),
  })
}

export function useDeleteThread() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (threadId: string) => apiPost(`/threads/${threadId}/delete`),
    onSuccess: () => invalidateMailLists(qc),
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
