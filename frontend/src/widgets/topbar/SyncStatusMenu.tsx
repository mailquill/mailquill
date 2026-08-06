import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { RefreshCw, CheckCircle2, AlertCircle } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { useClickOutside } from '@/shared/hooks/useClickOutside'
import { useSyncStatuses, useTriggerSync } from '@/shared/hooks/useAccounts'
import { accountInitials, resolveAccountColor } from '@/shared/lib/avatar'
import { relativeFromNow } from '@/shared/lib/format'
import { startOAuthRedirect } from '@/shared/lib/oauth'
import type { SyncStatus } from '@/shared/types'

type Phase = 'syncing' | 'reauth' | 'error' | 'idle'

function phaseOf(status?: SyncStatus): Phase {
  if (status?.state === 'syncing') return 'syncing'
  if (status?.state === 'reauth_required') return 'reauth'
  if (status?.state === 'error') return 'error'
  return 'idle'
}

/**
 * Phase used for all visuals. A sync that has nothing queued yet (totals not
 * known) or is already complete stays visually idle, so triggering a refresh
 * doesn't reset the progress bar or flip a green status to "syncing" until
 * there is actually new mail to fetch.
 */
function visualPhase(phase: Phase, synced: number, total: number): Phase {
  if (phase === 'syncing' && synced >= total) return 'idle'
  return phase
}

/** Progress bar coloured by sync phase; full when idle, fractional while syncing. */
function ProgressBar({ phase, synced, total }: { phase: Phase; synced: number; total: number }) {
  const pct =
    phase === 'syncing' ? (total > 0 ? Math.min(100, Math.round((synced / total) * 100)) : 8) : 100
  const color =
    phase === 'error' || phase === 'reauth'
      ? 'bg-destructive'
      : phase === 'syncing'
        ? 'bg-primary'
        : 'bg-[#16a34a]'
  return (
    <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-secondary">
      <div className={cn('h-full rounded-full transition-all', color)} style={{ width: `${pct}%` }} />
    </div>
  )
}

export function SyncStatusMenu() {
  const { t, i18n } = useTranslation()
  const [open, setOpen] = useState(false)
  const ref = useClickOutside<HTMLDivElement>(() => setOpen(false), open)
  const statuses = useSyncStatuses()
  const triggerSync = useTriggerSync()

  const anySyncing = statuses.some((s) => s.status?.state === 'syncing')
  const anyReauth = statuses.some((s) => s.status?.state === 'reauth_required')
  const anyError = statuses.some((s) => s.status?.state === 'error')
  const active = statuses.filter((s) => s.status?.state === 'syncing')
  const aggSynced = active.reduce((n, s) => n + (s.status?.synced ?? 0), 0)
  const aggTotal = active.reduce((n, s) => n + (s.status?.total ?? 0), 0)
  const lastSynced = statuses
    .map((s) => s.status?.last_synced_at)
    .filter((d): d is string => Boolean(d))
    .sort()
    .at(-1)
  const overallPhase = visualPhase(
    anyReauth ? 'reauth' : anySyncing ? 'syncing' : anyError ? 'error' : 'idle',
    aggSynced,
    aggTotal,
  )

  function refreshAll() {
    statuses
      .filter(({ status }) => status?.state !== 'reauth_required')
      .forEach(({ account }) => triggerSync.mutate(account.id))
  }

  function statusLabel(phase: Phase): string {
    if (phase === 'syncing') return t('syncMenu.syncing')
    if (phase === 'reauth') return t('syncMenu.reauthRequired')
    if (phase === 'error') return t('syncMenu.error')
    return t('syncMenu.noNewMessages')
  }

  function statusColor(phase: Phase): string {
    if (phase === 'syncing') return 'text-primary'
    if (phase === 'error' || phase === 'reauth') return 'text-destructive'
    return 'text-[#16a34a]'
  }

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        aria-label={overallPhase === 'reauth' ? t('syncMenu.reauthRequired') : t('syncMenu.title')}
        title={overallPhase === 'reauth' ? t('syncMenu.reauthRequired') : t('syncMenu.title')}
        className={cn(
          'relative flex size-9 items-center justify-center rounded-lg border border-border text-secondary-foreground transition-colors hover:bg-secondary',
          open ? 'bg-secondary' : 'bg-card',
        )}
      >
        <RefreshCw
          className={cn(
            'size-4',
            overallPhase === 'syncing' && 'animate-spin text-primary',
            (overallPhase === 'error' || overallPhase === 'reauth') && 'text-destructive',
          )}
        />
        {overallPhase === 'reauth' && (
          <span className="absolute right-1 top-1 size-2 rounded-full bg-destructive" />
        )}
      </button>

      {open && (
        <div className="absolute right-0 top-[120%] z-50 flex max-h-[min(560px,80vh)] w-[400px] flex-col overflow-hidden rounded-xl border border-border bg-popover shadow-2xl">
          {/* header */}
          <div className="flex shrink-0 items-center gap-2 border-b border-secondary px-4 pb-3 pt-3.5">
            <span className="text-[15px] font-bold tracking-tight text-foreground">{t('syncMenu.title')}</span>
            <button
              onClick={refreshAll}
              disabled={triggerSync.isPending || anySyncing}
              className="ml-auto inline-flex items-center gap-1.5 text-[12px] font-semibold text-[#2563eb] disabled:text-muted-foreground"
            >
              <RefreshCw className={cn('size-3.5', (triggerSync.isPending || anySyncing) && 'animate-spin')} />
              {t('syncMenu.refreshNow')}
            </button>
          </div>

          {/* all accounts */}
          <div className="border-b border-secondary px-4 py-3.5">
            <div className="flex items-start gap-2.5">
              {overallPhase === 'syncing' ? (
                <RefreshCw className="mt-0.5 size-5 shrink-0 animate-spin text-primary" />
              ) : overallPhase === 'error' || overallPhase === 'reauth' ? (
                <AlertCircle className="mt-0.5 size-5 shrink-0 text-destructive" />
              ) : (
                <CheckCircle2 className="mt-0.5 size-5 shrink-0 text-[#16a34a]" />
              )}
              <div className="min-w-0 flex-1">
                <div className="text-[14px] font-bold text-foreground">{t('syncMenu.allAccounts')}</div>
                <div className="text-[12px] text-muted-foreground">
                  {lastSynced
                    ? t('syncMenu.lastUpdated', { time: relativeFromNow(lastSynced, i18n.language) })
                    : t('syncMenu.never')}
                </div>
                <ProgressBar phase={overallPhase} synced={aggSynced} total={aggTotal} />
              </div>
            </div>
          </div>

          {/* per account */}
          <div className="flex-1 overflow-y-auto px-4 py-3">
            <div className="mb-2 text-[10.5px] font-bold uppercase tracking-[0.08em] text-muted-foreground">
              {t('syncMenu.perAccount')}
            </div>
            <div className="flex flex-col gap-3.5">
              {statuses.map(({ account, status }) => {
                const synced = status?.synced ?? 0
                const total = status?.total ?? 0
                const phase = visualPhase(phaseOf(status), synced, total)
                const color = resolveAccountColor(account)
                const reauthProvider = oauthProviderFromStatus(status)
                return (
                  <div key={account.id} className="flex items-start gap-3">
                    <span
                      className="flex size-8 shrink-0 items-center justify-center rounded-full text-[11px] font-extrabold uppercase text-white"
                      style={{ backgroundColor: color }}
                    >
                      {accountInitials(account.display_name)}
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-baseline gap-2">
                        <span className="truncate text-[13px] font-semibold text-foreground">
                          {account.primary_email}
                        </span>
                        <span className={cn('ml-auto shrink-0 text-[11.5px] font-semibold', statusColor(phase))}>
                          {statusLabel(phase)}
                        </span>
                      </div>
                      <ProgressBar phase={phase} synced={synced} total={total} />
                      <div className="mt-1 text-[11px] text-muted-foreground">
                        {phase === 'reauth' ? (
                          <span className="flex items-center justify-between gap-2">
                            <span>{t('syncMenu.reauthDescription')}</span>
                            {reauthProvider && (
                              <button
                                className="shrink-0 font-semibold text-[#2563eb] hover:underline"
                                onClick={() => startOAuthRedirect(reauthProvider, account.id)}
                              >
                                {t('syncMenu.reconnect')}
                              </button>
                            )}
                          </span>
                        ) : (
                          t('syncMenu.messages', { synced, total })
                        )}
                      </div>
                    </div>
                  </div>
                )
              })}
              {statuses.length === 0 && (
                <p className="text-[12.5px] text-muted-foreground">{t('sidebar.noAccounts')}</p>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function oauthProviderFromStatus(status?: SyncStatus): 'google' | 'microsoft' | null {
  if (status?.state !== 'reauth_required') return null
  const provider = status.error?.split(':', 2)[1]
  return provider === 'google' || provider === 'microsoft' ? provider : null
}
