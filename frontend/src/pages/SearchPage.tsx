import { useMemo } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { MessageList } from '@/widgets/MessageList'
import { useSearchMessages } from '@/shared/hooks/useMessages'
import type { Message } from '@/shared/types'

export function SearchPage() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const query = searchParams.get('q') ?? ''

  // All query params except the free-text `q` are passed through as filters,
  // so structured filters set from the top-bar funnel flow into the results.
  const filters = useMemo(() => {
    const out: Record<string, string> = {}
    searchParams.forEach((value, key) => {
      if (key !== 'q' && value.trim()) out[key] = value
    })
    return out
  }, [searchParams])

  const { data, isLoading, fetchNextPage, hasNextPage, isFetchingNextPage } = useSearchMessages(query, filters)
  const messages = data?.messages ?? []

  function handleSelect(message: Message) {
    // The folder view resolves by full_path, not the internal folder id.
    const folder = message.folder_path
    if (message.thread_id && folder) {
      navigate(`/mail/${message.account_id}/${encodeURIComponent(folder)}/${message.thread_id}`)
    }
  }

  return (
    <section className="grid h-full min-h-0 grid-cols-[minmax(320px,460px)_1fr]">
      <div className="flex min-h-0 flex-col border-r border-border bg-card">
        <header className="border-b border-border px-4 py-3">
          <h1 className="text-lg font-semibold">{t('filter.results')}</h1>
          <p className="text-xs text-muted-foreground">{t('filter.matches', { count: messages.length })}</p>
        </header>
        <MessageList
          messages={messages}
          onSelect={handleSelect}
          loading={isLoading}
          onLoadMore={fetchNextPage}
          hasMore={hasNextPage}
          loadingMore={isFetchingNextPage}
        />
      </div>
      <div className="flex h-full items-center justify-center bg-background p-8 text-center text-sm text-muted-foreground">
        {t('filter.selectResult')}
      </div>
    </section>
  )
}
