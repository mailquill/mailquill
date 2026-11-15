import { useMemo } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { MessageList } from '@/widgets/MessageList'
import { ThreadDetail } from '@/widgets/ReadingPane'
import { Loader2 } from 'lucide-react'
import { useRemoteSearchMessages, useSearchMessages } from '@/shared/hooks/useMessages'
import type { Message } from '@/shared/types'

/**
 * Route `/mail/search`: results of the top-bar search. The local index answers at once; matches only the mail servers know about (bodies that were never downloaded) are merged in when the slower server search returns, and the header shows which of the two is still running. `q` is the free-text query, `thread` the opened conversation, and every other query parameter is passed through as a structured filter, so selecting a result keeps the search intact and back/forward works.
 * @returns The results list beside the reading pane.
 */
export function SearchPage() {
  const { t } = useTranslation()
  const [searchParams, setSearchParams] = useSearchParams()
  const query = searchParams.get('q') ?? ''
  // The opened thread lives in the URL alongside the search, so selecting a
  // result keeps the query and filters intact (and back/forward works).
  const threadId = searchParams.get('thread') ?? ''

  // All query params except the free-text `q` and the opened `thread` are
  // passed through as filters, so structured filters set from the top-bar
  // funnel flow into the results.
  const filters = useMemo(() => {
    const out: Record<string, string> = {}
    searchParams.forEach((value, key) => {
      if (key !== 'q' && key !== 'thread' && value.trim()) out[key] = value
    })
    return out
  }, [searchParams])

  const { data, isLoading, isError, fetchNextPage, hasNextPage, isFetchingNextPage } = useSearchMessages(query, filters)
  const remote = useRemoteSearchMessages(query, filters)

  // Server-only matches join the local rows, newest first. The server answer
  // excludes local matches already, but only for the pages it knew about, so
  // dedupe by id as well.
  const localMessages = data?.messages
  const remoteMessages = remote.data?.messages
  const { messages, serverCount } = useMemo(() => {
    const local = localMessages ?? []
    const known = new Set(local.map((message) => message.id))
    const extra = (remoteMessages ?? []).filter((message) => !known.has(message.id))
    if (extra.length === 0) return { messages: local, serverCount: 0 }
    const merged = [...local, ...extra].sort((a, b) => b.internal_date.localeCompare(a.internal_date))
    return { messages: merged, serverCount: extra.length }
  }, [localMessages, remoteMessages])
  const matchCount = (data?.total ?? localMessages?.length ?? 0) + serverCount
  const serverFailed = remote.isError || (remote.data?.failedAccounts ?? 0) > 0

  function handleSelect(message: Message) {
    if (!message.thread_id) return
    const next = new URLSearchParams(searchParams)
    next.set('thread', message.thread_id)
    setSearchParams(next)
  }

  function closeThread() {
    const next = new URLSearchParams(searchParams)
    next.delete('thread')
    setSearchParams(next)
  }

  return (
    <section className="grid h-full min-h-0 grid-cols-[minmax(320px,460px)_1fr]">
      <div className="flex min-h-0 flex-col border-r border-border bg-card">
        <header className="border-b border-border px-4 py-3">
          <h1 className="text-lg font-semibold">{t('filter.results')}</h1>
          <p role="status" className="flex flex-wrap items-center gap-x-1.5 text-xs text-muted-foreground">
            {isLoading ? (
              <>
                <Spinner />
                {t('filter.searching')}
              </>
            ) : isError ? (
              t('filter.searchFailed')
            ) : (
              <>
                <span>{t('filter.matches', { count: matchCount })}</span>
                {serverCount > 0 && <span>· {t('filter.serverMatches', { count: serverCount })}</span>}
                {remote.isFetching && (
                  <span className="flex items-center gap-1.5">
                    · <Spinner />
                    {t('filter.searchingServer')}
                  </span>
                )}
                {!remote.isFetching && serverFailed && <span>· {t('filter.serverFailed')}</span>}
              </>
            )}
          </p>
        </header>
        <MessageList
          messages={messages}
          activeId={threadId}
          onSelect={handleSelect}
          loading={isLoading}
          onLoadMore={fetchNextPage}
          hasMore={hasNextPage}
          loadingMore={isFetchingNextPage}
        />
      </div>
      {threadId ? (
        <div className="min-w-0">
          <ThreadDetail threadId={threadId} onThreadGone={closeThread} />
        </div>
      ) : (
        <div className="flex h-full items-center justify-center bg-background p-8 text-center text-sm text-muted-foreground">
          {t('filter.selectResult')}
        </div>
      )}
    </section>
  )
}

function Spinner() {
  return <Loader2 className="size-3.5 shrink-0 motion-safe:animate-spin motion-reduce:animate-none" aria-hidden="true" />
}
