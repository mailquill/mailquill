import { useMemo } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { MessageList } from '@/widgets/MessageList'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Select } from '@/shared/components/ui/select'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useSearchMessages } from '@/shared/hooks/useMessages'
import type { Message } from '@/shared/types'

export function SearchPage() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [searchParams, setSearchParams] = useSearchParams()
  const query = searchParams.get('q') ?? ''
  const { data: accounts = [] } = useAccounts()

  // All query params except the free-text `q` are passed through as filters,
  // so structured filters set from the top-bar funnel flow into the results.
  const filters = useMemo(() => {
    const out: Record<string, string> = {}
    searchParams.forEach((value, key) => {
      if (key !== 'q' && value.trim()) out[key] = value
    })
    return out
  }, [searchParams])

  const { data, isLoading } = useSearchMessages(query, filters)
  const messages = data?.messages ?? []

  const from = searchParams.get('from') ?? ''
  const after = searchParams.get('after') ?? ''
  const before = searchParams.get('before') ?? ''
  const accountId = searchParams.get('account_id') ?? ''
  const readState = searchParams.get('is_read') ?? ''

  function setParam(key: string, value: string) {
    const next = new URLSearchParams(searchParams)
    if (value.trim()) next.set(key, value)
    else next.delete(key)
    setSearchParams(next, { replace: true })
  }

  function clearFilters() {
    const next = new URLSearchParams()
    if (query) next.set('q', query)
    setSearchParams(next)
  }

  function handleSelect(message: Message) {
    if (message.thread_id) {
      navigate(`/mail/${message.account_id}/${encodeURIComponent(message.folder_id)}/${message.thread_id}`)
    }
  }

  const hasFilters = Object.keys(filters).length > 0

  return (
    <section className="grid h-full min-h-0 grid-cols-[minmax(320px,460px)_1fr]">
      <div className="flex min-h-0 flex-col border-r border-border bg-card">
        <header className="border-b border-border px-4 py-3">
          <h1 className="text-lg font-semibold">{t('filter.results')}</h1>
          <p className="text-xs text-muted-foreground">{t('filter.matches', { count: messages.length })}</p>
        </header>
        <div className="flex flex-wrap items-end gap-2 border-b border-border px-4 py-3">
          <Input className="w-36" value={from} onChange={(event) => setParam('from', event.currentTarget.value)} placeholder={t('filter.from')} />
          <Input className="w-36" type="date" value={after} onChange={(event) => setParam('after', event.currentTarget.value)} aria-label={t('filter.after')} />
          <Input className="w-36" type="date" value={before} onChange={(event) => setParam('before', event.currentTarget.value)} aria-label={t('filter.before')} />
          <Select className="w-40" value={accountId} onChange={(event) => setParam('account_id', event.currentTarget.value)} aria-label={t('settings.accounts')}>
            <option value="">{t('filter.allAccounts')}</option>
            {accounts.map((account) => (
              <option key={account.id} value={account.id}>{account.display_name}</option>
            ))}
          </Select>
          <Select className="w-32" value={readState} onChange={(event) => setParam('is_read', event.currentTarget.value)} aria-label={t('filter.readState')}>
            <option value="">{t('filter.anyRead')}</option>
            <option value="false">{t('filter.unread')}</option>
            <option value="true">{t('filter.read')}</option>
          </Select>
          {hasFilters && (
            <Button type="button" size="sm" variant="ghost" onClick={clearFilters}>
              {t('filter.clear')}
            </Button>
          )}
        </div>
        <MessageList messages={messages} onSelect={handleSelect} loading={isLoading} />
      </div>
      <div className="flex h-full items-center justify-center bg-background p-8 text-center text-sm text-muted-foreground">
        {t('filter.selectResult')}
      </div>
    </section>
  )
}
