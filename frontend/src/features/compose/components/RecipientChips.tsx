import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { X, AlertCircle } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { accountColor, accountInitials } from '@/shared/lib/avatar'
import { parseFromAddr } from '@/shared/lib/format'
import { isValidEmail } from '@/shared/lib/email'
import { useUiPrefs } from '@/shared/hooks/useUiPrefs'
import { useContactSearch } from '@/shared/hooks/useContacts'

interface RecipientChipsProps {
  label: string
  value: string[]
  onChange: (next: string[]) => void
  accessory?: React.ReactNode
  autoFocus?: boolean
}

export function RecipientChips({ label, value, onChange, accessory, autoFocus }: RecipientChipsProps) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState('')
  const [focused, setFocused] = useState(false)
  const limit = useUiPrefs((s) => s.maxRecipients)
  const atLimit = value.length >= limit
  const { data: suggestions = [] } = useContactSearch(focused ? draft : '')

  function commit(raw: string) {
    const parts = raw
      .split(/[,;\s]+/)
      .map((p) => p.trim())
      .filter(Boolean)
    if (!parts.length) return
    const next = [...value]
    for (const p of parts) {
      if (next.length >= limit) break
      if (!next.includes(p)) next.push(p)
    }
    onChange(next)
    setDraft('')
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (['Enter', ',', ';', ' ', 'Tab'].includes(event.key)) {
      if (draft.trim()) {
        event.preventDefault()
        commit(draft)
      } else if (event.key === 'Tab') {
        // allow tab-out when empty
      }
    } else if (event.key === 'Backspace' && !draft && value.length) {
      // pull the last chip back into the input for editing
      event.preventDefault()
      onChange(value.slice(0, -1))
      setDraft(value[value.length - 1])
    }
  }

  function selectSuggestion(displayName: string, email: string) {
    const recipient = displayName ? `${displayName} <${email}>` : email
    if (!value.includes(recipient)) onChange([...value, recipient])
    setDraft('')
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2">
        <label className="text-[11px] font-bold uppercase tracking-wide text-muted-foreground">{label}</label>
        {accessory}
        {focused && (
          <span className="ml-auto text-[11px] text-muted-foreground">
            {value.length} / {limit}
          </span>
        )}
      </div>
      <div
        className={cn(
          'flex flex-wrap items-center gap-1.5 rounded-md border bg-card px-2 py-1.5 transition-colors',
          focused ? 'border-[#3b82f6] ring-2 ring-[#3b82f6]/15' : 'border-input',
        )}
        onClick={(e) => (e.currentTarget.querySelector('input') as HTMLInputElement | null)?.focus()}
      >
        {value.map((addr) => {
          const { name, email } = parseFromAddr(addr)
          const valid = isValidEmail(email)
          return (
            <span
              key={addr}
              className={cn(
                'inline-flex items-center gap-1.5 rounded-full py-0.5 pl-0.5 pr-1.5 text-[12.5px]',
                valid
                  ? 'bg-[var(--mq-row-open)] text-[#1d4ed8]'
                  : 'bg-destructive/10 text-destructive',
              )}
            >
              {valid ? (
                <span
                  className="flex size-5 items-center justify-center rounded-full text-[9px] font-bold uppercase text-white"
                  style={{ backgroundColor: accountColor(email) }}
                >
                  {accountInitials(name)}
                </span>
              ) : (
                <AlertCircle className="size-4" />
              )}
              <span className="max-w-[220px] truncate">{addr}</span>
              <button
                type="button"
                aria-label={`Remove ${addr}`}
                onClick={(e) => {
                  e.stopPropagation()
                  onChange(value.filter((v) => v !== addr))
                }}
                className="opacity-60 hover:opacity-100"
              >
                <X className="size-3" />
              </button>
            </span>
          )
        })}
        {!atLimit && (
          <span className="relative min-w-[180px] flex-1">
            <input
              autoFocus={autoFocus}
              value={draft}
              onChange={(e) => setDraft(e.currentTarget.value)}
              onKeyDown={onKeyDown}
              onFocus={() => setFocused(true)}
              onBlur={() => {
                window.setTimeout(() => {
                  setFocused(false)
                  if (draft.trim()) commit(draft)
                }, 120)
              }}
              placeholder={value.length ? '' : t('compose.placeholder')}
              className="w-full bg-transparent py-0.5 text-[13px] text-foreground outline-none placeholder:text-muted-foreground"
            />
            {focused && draft.trim().length >= 2 && suggestions.length > 0 && (
              <span className="absolute left-0 top-7 z-50 flex w-[320px] max-w-[80vw] flex-col overflow-hidden rounded-md border border-border bg-popover shadow-lg">
                {suggestions.flatMap((contact) =>
                  contact.emails.slice(0, 2).map((email) => (
                    <button
                      key={`${contact.id}-${email.value}`}
                      type="button"
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => selectSuggestion(contact.display_name ?? '', email.value)}
                      className="flex items-center gap-2 px-3 py-2 text-left text-sm transition-colors hover:bg-accent"
                    >
                      <span
                        className="flex size-7 shrink-0 items-center justify-center rounded-full text-[10px] font-bold uppercase text-white"
                        style={{ backgroundColor: accountColor(contact.id) }}
                      >
                        {accountInitials(contact.display_name ?? email.value)}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-medium text-foreground">
                          {contact.display_name ?? email.value}
                        </span>
                        <span className="block truncate font-mono text-xs text-muted-foreground">{email.value}</span>
                      </span>
                    </button>
                  )),
                )}
              </span>
            )}
          </span>
        )}
      </div>
      {focused && !atLimit && <p className="text-[11px] text-muted-foreground">{t('compose.hint')}</p>}
      {atLimit && <p className="text-[11px] text-muted-foreground">{t('compose.limitReached', { n: limit })}</p>}
    </div>
  )
}
