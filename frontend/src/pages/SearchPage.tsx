import { useMemo, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { MessageList } from '@/widgets/MessageList'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Select } from '@/shared/components/ui/select'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useSearchMessages } from '@/shared/hooks/useMessages'
import type { Message } from '@/shared/types'

export function SearchPage() {
  const navigate = useNavigate()
  const [searchParams, setSearchParams] = useSearchParams()
  const query = searchParams.get('q') ?? ''
  const [from, setFrom] = useState(searchParams.get('from') ?? '')
  const [after, setAfter] = useState(searchParams.get('after') ?? '')
  const [before, setBefore] = useState(searchParams.get('before') ?? '')
  const [accountId, setAccountId] = useState(searchParams.get('account_id') ?? '')
  const [readState, setReadState] = useState(searchParams.get('is_read') ?? '')
  const { data: accounts = [] } = useAccounts()
  const filters = useMemo(
    () => compactFilters({ from, after, before, account_id: accountId, is_read: readState }),
    [accountId, after, before, from, readState],
  )
  const { data, isLoading } = useSearchMessages(query, filters)
  const messages = data?.messages ?? []

  function applyFilters() {
    const next = new URLSearchParams({ q: query })
    Object.entries(filters).forEach(([key, value]) => next.set(key, value))
    setSearchParams(next)
  }

  function clearFilters() {
    setFrom('')
    setAfter('')
    setBefore('')
    setAccountId('')
    setReadState('')
    setSearchParams(new URLSearchParams({ q: query }))
  }

  function handleSelect(message: Message) {
    if (message.thread_id) {
      navigate(`/mail/${message.account_id}/${encodeURIComponent(message.folder_id)}/${message.thread_id}`)
    }
  }

  return (
    <section className="grid h-full min-h-0 grid-cols-[minmax(320px,460px)_1fr]">
      <div className="flex min-h-0 flex-col border-r border-border bg-card">
        <header className="border-b border-border px-4 py-3">
          <h1 className="text-lg font-semibold">Search results</h1>
          <p className="text-xs text-muted-foreground">{messages.length} matches</p>
        </header>
        <div className="flex flex-wrap items-end gap-2 border-b border-border px-4 py-3">
          <Input className="w-36" value={from} onChange={(event) => setFrom(event.currentTarget.value)} placeholder="From" />
          <Input className="w-36" type="date" value={after} onChange={(event) => setAfter(event.currentTarget.value)} aria-label="After" />
          <Input className="w-36" type="date" value={before} onChange={(event) => setBefore(event.currentTarget.value)} aria-label="Before" />
          <Select className="w-40" value={accountId} onChange={(event) => setAccountId(event.currentTarget.value)} aria-label="Account">
            <option value="">All accounts</option>
            {accounts.map((account) => (
              <option key={account.id} value={account.id}>{account.display_name}</option>
            ))}
          </Select>
          <Select className="w-32" value={readState} onChange={(event) => setReadState(event.currentTarget.value)} aria-label="Read state">
            <option value="">Any read</option>
            <option value="false">Unread</option>
            <option value="true">Read</option>
          </Select>
          <Button type="button" size="sm" onClick={applyFilters}>Apply</Button>
          <Button type="button" size="sm" variant="ghost" onClick={clearFilters}>Clear</Button>
        </div>
        <MessageList messages={messages} onSelect={handleSelect} loading={isLoading} />
      </div>
      <div className="flex h-full items-center justify-center bg-background p-8 text-center text-sm text-muted-foreground">
        Select a result to open its thread.
      </div>
    </section>
  )
}

function compactFilters(filters: Record<string, string>): Record<string, string> {
  return Object.fromEntries(Object.entries(filters).filter(([, value]) => value.trim()))
}
