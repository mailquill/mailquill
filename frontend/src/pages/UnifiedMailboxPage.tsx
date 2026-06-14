import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { MessageList } from '@/widgets/MessageList'
import { ReadingPaneEmpty, ThreadDetail } from '@/widgets/ReadingPane'
import { useUnifiedInbox, useUnifiedCounts } from '@/shared/hooks/useMessages'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { LIST_WIDTH, useUiPrefs } from '@/shared/hooks/useUiPrefs'
import { PaneResizer } from '@/shared/components/PaneResizer'
import { UNIFIED_LABEL_KEY, isUnifiedView } from '@/shared/lib/unifiedViews'
import type { Message } from '@/shared/types'

export function UnifiedMailboxPage() {
  const { t } = useTranslation()
  const { view: viewParam } = useParams()
  const view = isUnifiedView(viewParam) ? viewParam : 'inbox'
  const [selectedMessage, setSelectedMessage] = useState<Message | null>(null)
  // Clear the open thread when switching unified views (adjust state on change).
  const [prevView, setPrevView] = useState(view)
  if (view !== prevView) {
    setPrevView(view)
    setSelectedMessage(null)
  }
  const { data, isLoading, fetchNextPage, hasNextPage, isFetchingNextPage } = useUnifiedInbox(view)
  const { data: counts } = useUnifiedCounts()
  const { data: accounts } = useAccounts()
  const messages = data?.messages ?? []
  const accountCount = accounts?.length ?? 0
  const unread = counts?.[view] ?? 0
  const listWidth = useUiPrefs((s) => s.listWidth)
  const setListWidth = useUiPrefs((s) => s.setListWidth)

  // Open the thread inline in the reading pane; keep the unified list and the
  // clicked row highlighted rather than navigating to a folder route.
  function handleSelect(message: Message) {
    setSelectedMessage(message)
  }

  const selectedThreadId = selectedMessage?.thread_id ?? selectedMessage?.id ?? ''

  return (
    <section className="flex h-full min-h-0">
      <div className="flex min-h-0 shrink-0 flex-col border-r border-border bg-card" style={{ width: listWidth }}>
        <header className="flex items-center justify-between border-b border-border px-4 py-3">
          <div>
            <h1 className="text-lg font-semibold">
              {t(UNIFIED_LABEL_KEY[view])} ({t('mail.allAccounts')})
            </h1>
            <p className="text-xs text-muted-foreground">
              {t('mail.accountsUnread', { accounts: accountCount, unread })}
            </p>
          </div>
        </header>
        <MessageList
          messages={messages}
          activeId={selectedMessage?.id}
          onSelect={handleSelect}
          loading={isLoading}
          onLoadMore={fetchNextPage}
          hasMore={hasNextPage}
          loadingMore={isFetchingNextPage}
          total={data?.total}
          scope={{ view }}
        />
      </div>
      <PaneResizer
        width={listWidth}
        min={LIST_WIDTH.min}
        max={LIST_WIDTH.max}
        onChange={setListWidth}
        label="Resize message list"
      />
      <div className="min-w-0 flex-1">
        {selectedThreadId ? (
          <ThreadDetail threadId={selectedThreadId} onThreadGone={() => setSelectedMessage(null)} />
        ) : (
          <ReadingPaneEmpty />
        )}
      </div>
    </section>
  )
}
