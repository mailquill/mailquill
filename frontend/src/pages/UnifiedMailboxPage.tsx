import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { MailOpen } from 'lucide-react'
import { MessageList } from '@/widgets/MessageList'
import { Button } from '@/shared/components/ui/button'
import { useAccounts, useTriggerSync } from '@/shared/hooks/useAccounts'
import { useUnifiedInbox } from '@/shared/hooks/useMessages'
import type { Message } from '@/shared/types'

export function UnifiedMailboxPage() {
  const navigate = useNavigate()
  const [selectedMessage, setSelectedMessage] = useState<Message | null>(null)
  const { data, isLoading, refetch, isFetching } = useUnifiedInbox()
  const { data: accounts = [] } = useAccounts()
  const triggerSync = useTriggerSync()
  const messages = data?.messages ?? []

  function handleSelect(message: Message) {
    setSelectedMessage(message)
    if (message.thread_id) {
      navigate(`/mail/${message.account_id}/${encodeURIComponent(message.folder_id)}/${message.thread_id}`)
    }
  }

  function handleRefresh() {
    if (!accounts.length) {
      refetch()
      return
    }

    accounts.forEach((account) => {
      triggerSync.mutate(account.id, {
        onSettled: () => {
          refetch()
        },
      })
    })
  }

  return (
    <section className="grid h-full min-h-0 grid-cols-[minmax(320px,420px)_1fr]">
      <div className="flex min-h-0 flex-col border-r border-border bg-card">
        <header className="flex items-center justify-between border-b border-border px-4 py-3">
          <div>
            <h1 className="text-lg font-semibold">Unified inbox</h1>
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
          activeId={selectedMessage?.id}
          onSelect={handleSelect}
          loading={isLoading}
        />
      </div>
      <EmptyDetail />
    </section>
  )
}

function EmptyDetail() {
  return (
    <div className="flex h-full items-center justify-center bg-background p-8 text-center">
      <div className="flex max-w-sm flex-col items-center gap-3 text-muted-foreground">
        <MailOpen className="size-10" aria-hidden="true" />
        <h2 className="text-base font-medium text-foreground">Select a conversation</h2>
        <p className="text-sm">Open a message from the list to read the thread.</p>
      </div>
    </div>
  )
}
