import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPatch, apiDelete } from '@/shared/api'
import type { Message, SendMessageInput } from '@/shared/types'

export function useUnifiedInbox(cursor?: string) {
  return useQuery({
    queryKey: ['unified', cursor],
    queryFn: () =>
      apiGet<{ messages?: ApiMessage[]; items?: ApiMessage[]; next_cursor: string | null }>(
        `/mailbox/unified${cursor ? `?cursor=${cursor}` : ''}`,
      ).then(normalizeMessagePage),
  })
}

export function useFolderMessages(accountId: string, folder: string, cursor?: string) {
  return useQuery({
    queryKey: ['folder-messages', accountId, folder, cursor],
    queryFn: () =>
      apiGet<{ messages?: ApiMessage[]; items?: ApiMessage[]; next_cursor: string | null }>(
        `/accounts/${accountId}/folders/${encodeURIComponent(folder)}/messages${cursor ? `?cursor=${cursor}` : ''}`,
      ).then(normalizeMessagePage),
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

export function useMarkRead() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, is_read }: { id: string; is_read: boolean }) =>
      apiPatch(`/messages/${id}/read`, { is_read }),
    onSuccess: (_data, { id }) => {
      qc.invalidateQueries({ queryKey: ['message', id] })
      qc.invalidateQueries({ queryKey: ['unified'] })
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
    },
  })
}

export function useArchiveMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPost(`/messages/${id}/archive`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['unified'] }),
  })
}

export function useDeleteMessage() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/messages/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['unified'] }),
  })
}

export function useArchiveThread() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (threadId: string) => apiPost(`/threads/${threadId}/archive`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['unified'] }),
  })
}

export function useDeleteThread() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (threadId: string) => apiPost(`/threads/${threadId}/delete`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['unified'] }),
  })
}

export function useMarkThreadRead() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ threadId, is_read }: { threadId: string; is_read: boolean }) =>
      apiPatch(`/threads/${threadId}/read`, { is_read }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['unified'] }),
  })
}

export function useSearchMessages(query: string, filters?: Record<string, string>) {
  const params = new URLSearchParams({ q: query, ...filters })
  return useQuery({
    queryKey: ['search', query, filters],
    queryFn: () =>
      apiGet<{ messages?: ApiMessage[]; items?: ApiMessage[]; next_cursor: string | null }>(`/search?${params}`)
        .then(normalizeMessagePage),
    enabled: query.length > 1,
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
  next_cursor: string | null
}): { messages: Message[]; next_cursor: string | null } {
  const source = page.messages ?? page.items ?? []

  return {
    messages: source.map((message) => ({
      ...message,
      id: message.id ?? message.message_id ?? '',
    })),
    next_cursor: page.next_cursor,
  }
}
