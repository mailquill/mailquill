import { useNavigate, useParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { MessageList } from '@/widgets/MessageList'
import { ThreadDetail } from '@/widgets/ReadingPane'
import { useFolderMessages } from '@/shared/hooks/useMessages'
import { LIST_WIDTH, useUiPrefs } from '@/shared/hooks/useUiPrefs'
import { PaneResizer } from '@/shared/components/PaneResizer'
import type { Message } from '@/shared/types'

export function MailFolderPage() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { accountId = '', folder = '', threadId = '' } = useParams()
  const folderName = decodeURIComponent(folder)
  const { data, isLoading, fetchNextPage, hasNextPage, isFetchingNextPage } = useFolderMessages(accountId, folderName)
  const messages = data?.messages ?? []
  const listWidth = useUiPrefs((s) => s.listWidth)
  const setListWidth = useUiPrefs((s) => s.setListWidth)

  function handleSelect(message: Message) {
    navigate(`/mail/${accountId}/${encodeURIComponent(folderName)}/${message.thread_id ?? message.id}`)
  }

  return (
    <section className="flex h-full min-h-0">
      <div className="flex min-h-0 shrink-0 flex-col border-r border-border bg-card" style={{ width: listWidth }}>
        <header className="flex items-center justify-between border-b border-border px-4 py-3.5">
          <div className="min-w-0">
            <h1 className="truncate text-[16px] font-bold tracking-tight">{folderName}</h1>
            <p className="text-xs text-muted-foreground">
              {t('mail.messageCount', { count: data?.total ?? messages.length })}
            </p>
          </div>
        </header>
        <MessageList
          messages={messages}
          activeId={threadId}
          onSelect={handleSelect}
          loading={isLoading}
          onLoadMore={fetchNextPage}
          hasMore={hasNextPage}
          loadingMore={isFetchingNextPage}
          total={data?.total}
          scope={{ accountId, folder: folderName }}
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
        <ThreadDetail
          threadId={threadId}
          onThreadGone={() => navigate(`/mail/${accountId}/${encodeURIComponent(folderName)}`)}
        />
      </div>
    </section>
  )
}
