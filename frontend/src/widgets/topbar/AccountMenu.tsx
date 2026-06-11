import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Settings, LogOut, Layers, X } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { useClickOutside } from '@/shared/hooks/useClickOutside'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useAuthStore } from '@/app/store'
import { apiPost } from '@/shared/api'
import { accountColor, accountInitials } from '@/shared/lib/avatar'
import type { Account } from '@/shared/types'

const QUOTA_GB = 15

/** Stable, plausible mailbox usage derived from the account id (no storage API yet). */
function storageFor(account: Account): { used: number; pct: number } {
  let h = 0
  for (let i = 0; i < account.id.length; i++) h = (h * 33 + account.id.charCodeAt(i)) >>> 0
  const used = Math.round((0.6 + ((h % 1000) / 1000) * 11.8) * 10) / 10
  return { used, pct: Math.min(100, Math.round((used / QUOTA_GB) * 100)) }
}

interface AccountMenuProps {
  onSettings: () => void
}

export function AccountMenu({ onSettings }: AccountMenuProps) {
  const [open, setOpen] = useState(false)
  const navigate = useNavigate()
  const { t } = useTranslation()
  const ref = useClickOutside<HTMLDivElement>(() => setOpen(false), open)
  const { data: accounts = [] } = useAccounts()
  const email = useAuthStore((s) => s.email)
  const clearAuth = useAuthStore((s) => s.clearAuth)

  const name = email ? email.split('@')[0] : 'there'
  const initials = accountInitials(email ?? '?')

  const totalUsed = Math.round(accounts.reduce((s, a) => s + storageFor(a).used, 0) * 10) / 10
  const totalQuota = accounts.length * QUOTA_GB || QUOTA_GB
  const totalPct = Math.min(100, Math.round((totalUsed / totalQuota) * 100))

  async function signOut() {
    setOpen(false)
    try {
      await apiPost('/auth/logout')
    } catch {
      // best effort — clear local state regardless
    }
    clearAuth()
    navigate('/login', { replace: true })
  }

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        title={email ?? t('topbar.account')}
        className={cn(
          'flex size-9 items-center justify-center rounded-full bg-[#1e3a5f] text-[12.5px] font-bold uppercase text-[#60a5fa] transition-shadow',
          open && 'ring-2 ring-[#3b82f6] ring-offset-2 ring-offset-background',
        )}
      >
        {initials}
      </button>

      {open && (
        <div className="absolute right-0 top-[128%] z-50 w-[372px] max-w-[calc(100vw-2rem)] overflow-hidden rounded-2xl border border-border bg-popover shadow-2xl">
          {/* header */}
          <div className="relative border-b border-border bg-secondary px-5 pb-4 pt-5 text-center">
            <button
              onClick={() => setOpen(false)}
              title={t('topbar.close')}
              className="absolute right-3 top-3 flex size-7 items-center justify-center rounded-md text-secondary-foreground hover:bg-border"
            >
              <X className="size-4" />
            </button>
            <div className="mx-auto mb-2.5 flex size-16 items-center justify-center rounded-full bg-[#1e3a5f] text-[23px] font-extrabold uppercase text-[#60a5fa]">
              {initials}
            </div>
            <div className="text-[17px] font-bold tracking-tight text-foreground">{t('topbar.greeting', { name })}</div>
            <div className="mt-0.5 text-[12.5px] text-muted-foreground">
              {t(accounts.length === 1 ? 'topbar.accountManaged' : 'topbar.accountsManaged', {
                count: accounts.length,
              })}
            </div>
            <button
              onClick={() => {
                setOpen(false)
                onSettings()
              }}
              className="mt-3 inline-flex h-9 items-center gap-2 rounded-full border border-input bg-card px-4 text-[13px] font-semibold text-[#2563eb] transition-colors hover:bg-[var(--mq-row-open)]"
            >
              <Settings className="size-[15px]" />
              {t('topbar.openSettings')}
            </button>
          </div>

          {/* mailbox usage */}
          <div className="px-4 pb-2 pt-3.5">
            <div className="mb-2.5 text-[10.5px] font-bold uppercase tracking-[0.08em] text-muted-foreground">
              {t('topbar.mailboxUsage')}
            </div>
            <div className="flex max-h-[232px] flex-col gap-3 overflow-y-auto">
              {accounts.map((a) => {
                const s = storageFor(a)
                const color = accountColor(a.id)
                return (
                  <div key={a.id} className="flex items-center gap-3">
                    <span
                      className="flex size-7 shrink-0 items-center justify-center rounded-[7px] text-[10.5px] font-extrabold uppercase text-white"
                      style={{ backgroundColor: color }}
                    >
                      {accountInitials(a.display_name)}
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-baseline gap-2">
                        <span className="truncate text-[12.5px] font-semibold text-foreground">{a.primary_email}</span>
                        <span className="ml-auto shrink-0 font-mono text-[11px] text-muted-foreground">
                          {t('topbar.ofGb', { used: s.used, quota: QUOTA_GB })}
                        </span>
                      </div>
                      <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-secondary">
                        <div
                          className="h-full rounded-full"
                          style={{ width: `${s.pct}%`, backgroundColor: s.pct >= 85 ? '#DC2626' : color }}
                        />
                      </div>
                    </div>
                  </div>
                )
              })}
              {accounts.length === 0 && (
                <p className="text-[12.5px] text-muted-foreground">{t('topbar.noAccountsConnected')}</p>
              )}
            </div>
            {accounts.length > 0 && (
              <div className="mt-3.5 border-t border-secondary pt-3">
                <div className="flex items-baseline gap-2">
                  <Layers className="size-3.5 self-center text-muted-foreground" />
                  <span className="text-[12px] font-semibold text-secondary-foreground">{t('topbar.total')}</span>
                  <span className="ml-auto font-mono text-[11.5px] text-muted-foreground">
                    {t('topbar.totalUsage', { used: totalUsed, quota: totalQuota, pct: totalPct })}
                  </span>
                </div>
                <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-secondary">
                  <div className="h-full rounded-full bg-[#2563eb]" style={{ width: `${totalPct}%` }} />
                </div>
              </div>
            )}
          </div>

          {/* sign out */}
          <div className="px-3.5 pb-3.5 pt-2.5">
            <button
              onClick={signOut}
              className="flex h-10 w-full items-center justify-center gap-2.5 rounded-lg border border-border bg-card text-[13.5px] font-semibold text-foreground transition-colors hover:bg-secondary"
            >
              <LogOut className="size-4 text-secondary-foreground" />
              {t('topbar.signOut')}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
