import { useNavigate, useOutletContext, useParams } from 'react-router-dom'
import { Archive, Forward, MailOpen, Mail, MailCheck, Reply, Star, Trash2 } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { MessageList } from '@/widgets/MessageList'
import { Button } from '@/shared/components/ui/button'
import { Badge } from '@/shared/components/ui/badge'
import { useTriggerSync } from '@/shared/hooks/useAccounts'
import {
  useArchiveThread,
  useDeleteThread,
  useFolderMessages,
  useDeleteMessage,
  useMarkRead,
  useMarkThreadRead,
  useMessage,
  useThread,
  useToggleFlag,
} from '@/shared/hooks/useMessages'
import { formatDate, listIdToName, parseFromAddr } from '@/shared/lib/format'
import type { MailOutletContext } from './MailLayout'
import type { Message } from '@/shared/types'

export function MailFolderPage() {
  const navigate = useNavigate()
  const { accountId = '', folder = '', threadId = '' } = useParams()
  const folderName = decodeURIComponent(folder)
  const { data, isLoading, refetch, isFetching } = useFolderMessages(accountId, folderName)
  const triggerSync = useTriggerSync()
  const messages = data?.messages ?? []

  function handleSelect(message: Message) {
    navigate(`/mail/${accountId}/${encodeURIComponent(folderName)}/${message.thread_id ?? message.id}`)
  }

  function handleRefresh() {
    triggerSync.mutate(accountId, {
      onSettled: () => {
        refetch()
      },
    })
  }

  return (
    <section className="grid h-full min-h-0 grid-cols-[minmax(320px,420px)_1fr]">
      <div className="flex min-h-0 flex-col border-r border-border bg-card">
        <header className="flex items-center justify-between border-b border-border px-4 py-3">
          <div className="min-w-0">
            <h1 className="truncate text-lg font-semibold">{folderName}</h1>
            <p className="text-xs text-muted-foreground">{messages.length} conversations</p>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleRefresh}
            disabled={isFetching || triggerSync.isPending}
          >
            Refresh
          </Button>
        </header>
        <MessageList
          messages={messages}
          activeId={threadId}
          onSelect={handleSelect}
          loading={isLoading}
        />
      </div>
      <ThreadDetail threadId={threadId} />
    </section>
  )
}

interface ThreadDetailProps {
  threadId: string
}

function ThreadDetail({ threadId }: ThreadDetailProps) {
  const { data, isLoading } = useThread(threadId)
  const archiveThread = useArchiveThread()
  const deleteThread = useDeleteThread()
  const markThreadRead = useMarkThreadRead()
  const messages = useMemo(() => data?.messages ?? [], [data?.messages])
  const firstMessage = messages[0]
  const listName = listIdToName(firstMessage?.list_id)
  const defaultExpandedId = useMemo(() => {
    const unread = [...messages].reverse().find((message) => !message.is_read)
    return unread?.id ?? messages.at(-1)?.id
  }, [messages])

  if (!threadId) {
    return <EmptyDetail />
  }

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Loading thread...
      </div>
    )
  }

  if (!messages.length) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Thread not found
      </div>
    )
  }

  return (
    <article className="flex h-full min-h-0 flex-col bg-background">
      <header className="border-b border-border px-6 py-4">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 className="truncate text-xl font-semibold">{firstMessage?.subject || '(no subject)'}</h2>
            <div className="mt-2 flex items-center gap-2">
              {listName ? <Badge variant="outline">{listName}</Badge> : null}
              <span className="text-xs text-muted-foreground">{messages.length} messages</span>
            </div>
          </div>
          <div className="flex shrink-0 gap-2">
            <Button
              type="button"
              size="icon"
              variant="ghost"
              aria-label="Archive thread"
              onClick={() => archiveThread.mutate(threadId)}
              disabled={archiveThread.isPending}
            >
              <Archive className="size-4" aria-hidden="true" />
            </Button>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              aria-label="Delete thread"
              onClick={() => deleteThread.mutate(threadId)}
              disabled={deleteThread.isPending}
            >
              <Trash2 className="size-4" aria-hidden="true" />
            </Button>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              aria-label="Mark thread read"
              onClick={() => markThreadRead.mutate({ threadId, is_read: true })}
              disabled={markThreadRead.isPending}
            >
              <MailCheck className="size-4" aria-hidden="true" />
            </Button>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              aria-label="Mark thread unread"
              onClick={() => markThreadRead.mutate({ threadId, is_read: false })}
              disabled={markThreadRead.isPending}
            >
              <Mail className="size-4" aria-hidden="true" />
            </Button>
          </div>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto p-6">
        <div className="flex flex-col gap-4">
          {messages.map((message) => (
            <MessageCard key={message.id} message={message} defaultExpanded={message.id === defaultExpandedId} />
          ))}
        </div>
      </div>
    </article>
  )
}

function MessageCard({ message, defaultExpanded }: { message: Message; defaultExpanded: boolean }) {
  const { openCompose } = useOutletContext<MailOutletContext>()
  const [isExpanded, setIsExpanded] = useState(defaultExpanded)
  const sender = parseFromAddr(message.from_addr)
  const { data: detail, isLoading } = useMessage(isExpanded ? message.id : '')
  const markRead = useMarkRead()
  const toggleFlag = useToggleFlag()
  const deleteMessage = useDeleteMessage()
  const displayedMessage = detail ?? message
  const bodyLoaded = Boolean(displayedMessage.body_html || displayedMessage.body_text || displayedMessage.snippet)
  const isOfflineBodyMissing = !navigator.onLine && displayedMessage.body_available === false

  useEffect(() => {
    if (isExpanded && bodyLoaded && !displayedMessage.is_read && !markRead.isPending) {
      markRead.mutate({ id: displayedMessage.id, is_read: true })
    }
  }, [bodyLoaded, displayedMessage.id, displayedMessage.is_read, isExpanded, markRead])

  return (
    <section className="rounded-md border border-border bg-card p-4">
      <button
        type="button"
        className="flex w-full items-start justify-between gap-3 text-left"
        onClick={() => setIsExpanded((value) => !value)}
      >
        <div className="min-w-0">
          <h3 className="truncate text-sm font-semibold">{sender.name}</h3>
          <p className="truncate text-xs text-muted-foreground">{sender.email}</p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {message.folder_path ? <Badge variant="outline">{message.folder_path}</Badge> : null}
          <Badge variant="secondary">{message.is_read ? 'Read' : 'Unread'}</Badge>
          <span className="text-xs text-muted-foreground">{formatDate(message.internal_date)}</span>
        </div>
      </button>
      <div className="mt-3 flex items-center gap-1">
        <Button
          type="button"
          size="icon"
          variant="ghost"
          aria-label={displayedMessage.is_flagged ? 'Unstar message' : 'Star message'}
          onClick={() => toggleFlag.mutate({ id: displayedMessage.id, is_flagged: !displayedMessage.is_flagged })}
          disabled={toggleFlag.isPending}
        >
          <Star
            className={displayedMessage.is_flagged ? 'size-4 fill-current' : 'size-4'}
            aria-hidden="true"
          />
        </Button>
        <Button
          type="button"
          size="icon"
          variant="ghost"
          aria-label="Reply"
          onClick={() => openCompose({ mode: 'reply', sourceMessage: displayedMessage })}
        >
          <Reply className="size-4" aria-hidden="true" />
        </Button>
        <Button
          type="button"
          size="icon"
          variant="ghost"
          aria-label="Forward"
          onClick={() => openCompose({ mode: 'forward', sourceMessage: displayedMessage })}
        >
          <Forward className="size-4" aria-hidden="true" />
        </Button>
        <Button
          type="button"
          size="icon"
          variant="ghost"
          aria-label="Delete message"
          onClick={() => deleteMessage.mutate(displayedMessage.id)}
          disabled={deleteMessage.isPending}
        >
          <Trash2 className="size-4" aria-hidden="true" />
        </Button>
      </div>
      {isExpanded ? (
        <div className="mt-4 text-sm leading-6">
          {isLoading ? (
            <div className="flex h-24 items-center justify-center text-muted-foreground">Loading body...</div>
          ) : isOfflineBodyMissing ? (
            <div className="rounded-md border border-border bg-muted/30 p-4 text-muted-foreground">
              Body not available offline. Download when online.
            </div>
          ) : displayedMessage.body_html ? (
            <iframe
              title={displayedMessage.subject}
              sandbox=""
              srcDoc={displayedMessage.body_html}
              className="h-80 w-full rounded-md border border-border bg-background"
            />
          ) : (
            <p className="whitespace-pre-wrap">{displayedMessage.body_text ?? displayedMessage.snippet}</p>
          )}
        </div>
      ) : (
        <p className="mt-3 truncate text-sm text-muted-foreground">{message.snippet}</p>
      )}
    </section>
  )
}

function EmptyDetail() {
  return (
    <div className="flex h-full items-center justify-center bg-background p-8 text-center">
      <div className="flex max-w-sm flex-col items-center gap-3 text-muted-foreground">
        <MailOpen className="size-10" aria-hidden="true" />
        <h2 className="text-base font-medium text-foreground">Select a conversation</h2>
        <p className="text-sm">Choose a message in this folder to show its thread.</p>
      </div>
    </div>
  )
}
