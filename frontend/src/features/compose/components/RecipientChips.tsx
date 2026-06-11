import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { X, AlertCircle } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { accountColor, accountInitials } from '@/shared/lib/avatar'
import { parseFromAddr } from '@/shared/lib/format'
import { isValidEmail } from '@/shared/lib/email'
import { useUiPrefs } from '@/shared/hooks/useUiPrefs'

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
          const valid = isValidEmail(addr)
          const { name, email } = parseFromAddr(addr)
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
          <input
            autoFocus={autoFocus}
            value={draft}
            onChange={(e) => setDraft(e.currentTarget.value)}
            onKeyDown={onKeyDown}
            onFocus={() => setFocused(true)}
            onBlur={() => {
              setFocused(false)
              if (draft.trim()) commit(draft)
            }}
            placeholder={value.length ? '' : t('compose.placeholder')}
            className="min-w-[140px] flex-1 bg-transparent py-0.5 text-[13px] text-foreground outline-none placeholder:text-muted-foreground"
          />
        )}
      </div>
      {focused && !atLimit && <p className="text-[11px] text-muted-foreground">{t('compose.hint')}</p>}
      {atLimit && <p className="text-[11px] text-muted-foreground">{t('compose.limitReached', { n: limit })}</p>}
    </div>
  )
}
