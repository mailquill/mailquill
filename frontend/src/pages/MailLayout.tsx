import { lazy, Suspense, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { Outlet, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import type { ComposeInitialState } from '@/features/compose'
import { useOnlineStatus } from '@/shared/hooks/useOnlineStatus'
import { useSyncActivity } from '@/shared/hooks/useAccounts'
import { SIDEBAR_WIDTH, useUiPrefs } from '@/shared/hooks/useUiPrefs'
import { useMailNotifications } from '@/shared/hooks/useMailNotifications'
import { useSendProgressToasts } from '@/shared/hooks/useSendProgressToasts'
import { ToastRegion } from '@/shared/components'
import { PaneResizer } from '@/shared/components/PaneResizer'
import { TopBar } from '@/widgets/TopBar'
import { Sidebar } from '@/widgets/Sidebar'

const ComposeDialog = lazy(() => import('@/features/compose').then((module) => ({ default: module.ComposeDialog })))

export interface MailOutletContext {
  openCompose: (state?: ComposeInitialState) => void
}

function PageLoadingFallback() {
  const { t } = useTranslation()

  return (
    <div className="flex h-full items-center justify-center text-muted-foreground" role="status" aria-live="polite">
      <RefreshCw className="size-5 motion-safe:animate-spin motion-reduce:animate-none" aria-hidden="true" />
      <span className="sr-only">{t('messages.loading')}</span>
    </div>
  )
}

export function MailLayout() {
  const isOnline = useOnlineStatus()
  const navigate = useNavigate()
  // Watch background syncs app-wide: auto-refresh folders/messages on completion.
  useSyncActivity()
  const sidebarWidth = useUiPrefs((s) => s.sidebarWidth)
  const setSidebarWidth = useUiPrefs((s) => s.setSidebarWidth)
  const { showSendQueued, showSendStatus } = useSendProgressToasts()
  // Foreground desktop notifications and asynchronous send results share SSE.
  useMailNotifications(useUiPrefs((s) => s.notificationsEnabled), showSendStatus)
  const [isComposeOpen, setIsComposeOpen] = useState(false)
  const [composeState, setComposeState] = useState<ComposeInitialState>({ mode: 'new' })
  const [composeKey, setComposeKey] = useState(0)

  function openCompose(state: ComposeInitialState = { mode: 'new' }) {
    setComposeState(state)
    setComposeKey((current) => current + 1)
    setIsComposeOpen(true)
  }

  return (
    <div className="flex h-screen min-h-0 bg-background text-foreground">
      <Sidebar onCompose={() => openCompose()} />
      <PaneResizer
        width={sidebarWidth}
        min={SIDEBAR_WIDTH.min}
        max={SIDEBAR_WIDTH.max}
        onChange={setSidebarWidth}
        label="Resize sidebar"
      />
      <main className="flex min-w-0 flex-1 flex-col">
        {!isOnline ? (
          <div className="border-b border-border bg-muted px-4 py-2 text-sm font-medium text-muted-foreground">
            Offline
          </div>
        ) : null}
        <TopBar onSettings={() => navigate('/mail/settings')} />
        <div className="min-h-0 flex-1">
          <Suspense fallback={<PageLoadingFallback />}>
            <Outlet context={{ openCompose } satisfies MailOutletContext} />
          </Suspense>
        </div>
      </main>
      {isComposeOpen ? (
        <Suspense fallback={null}>
          <ComposeDialog
            key={composeKey}
            open
            initialState={composeState}
            onClose={() => setIsComposeOpen(false)}
            onSendQueued={showSendQueued}
          />
        </Suspense>
      ) : null}
      <ToastRegion />
    </div>
  )
}
