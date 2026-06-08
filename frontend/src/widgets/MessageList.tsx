import { cn } from '@/shared/lib/utils'
import { formatDate, parseFromAddr, listIdToName } from '@/shared/lib/format'
import { Avatar } from '@/shared/components/ui/avatar'
import { Badge } from '@/shared/components/ui/badge'
import { Star } from 'lucide-react'
import type { Message } from '@/shared/types'

interface MessageRowProps {
  message: Message
  isActive: boolean
  onClick: () => void
}

export function MessageRow({ message, isActive, onClick }: MessageRowProps) {
  const { name } = parseFromAddr(message.from_addr)
  const listName = listIdToName(message.list_id)
  const date = formatDate(message.internal_date)
  const participants = message.thread_participants
    ?.slice(0, 3)
    .map((participant) => parseFromAddr(participant).name)
    .join(', ')
  const senderLabel = participants || name

  return (
    <button
      onClick={onClick}
      className={cn(
        'flex w-full items-start gap-3 border-b border-border px-4 py-3 text-left transition-colors',
        isActive ? 'bg-primary/5' : 'hover:bg-accent/50',
        !message.is_read && 'bg-muted/30',
      )}
    >
      <Avatar name={name} size={36} className="mt-0.5 shrink-0" />

      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-2">
          <span className={cn('truncate text-sm', !message.is_read && 'font-semibold')}>
            {senderLabel}
          </span>
          <span className="shrink-0 text-xs text-muted-foreground">{date}</span>
        </div>

        <div className="flex items-center gap-1">
          <span className={cn('truncate text-sm', !message.is_read ? 'font-medium' : 'text-muted-foreground')}>
            {message.subject || '(no subject)'}
          </span>
          {message.thread_size && message.thread_size > 1 && (
            <span className="shrink-0 text-xs text-muted-foreground">
              [{message.thread_size}]
            </span>
          )}
        </div>

        <div className="flex items-center gap-1.5 mt-0.5">
          {listName && (
            <Badge variant="outline" className="text-xs py-0 px-1.5">
              {listName}
            </Badge>
          )}
          {message.thread_unread && message.thread_unread > 0 && (
            <Badge className="text-xs py-0 px-1.5">{message.thread_unread}</Badge>
          )}
          <span className="truncate text-xs text-muted-foreground">{message.snippet}</span>
          {message.is_flagged && <Star className="ml-auto h-3 w-3 shrink-0 fill-yellow-400 text-yellow-400" />}
        </div>
      </div>
    </button>
  )
}

interface MessageListProps {
  messages: Message[]
  activeId?: string
  onSelect: (msg: Message) => void
  onRefresh?: () => void
  loading?: boolean
}

export function MessageList({ messages, activeId, onSelect, loading }: MessageListProps) {
  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Loading messages…
      </div>
    )
  }

  if (!messages.length) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        No messages
      </div>
    )
  }

  return (
    <div className="flex flex-col overflow-y-auto">
      {messages.map((msg) => (
        <MessageRow
          key={msg.id}
          message={msg}
          isActive={msg.id === activeId}
          onClick={() => onSelect(msg)}
        />
      ))}
    </div>
  )
}
