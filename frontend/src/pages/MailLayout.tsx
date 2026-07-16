import { useState } from 'react'
import { Outlet, useNavigate } from 'react-router-dom'
import { ComposeDialog } from '@/features/compose'
import type { ComposeInitialState } from '@/features/compose'
import { useOnlineStatus } from '@/shared/hooks/useOnlineStatus'
import { useSyncActivity } from '@/shared/hooks/useAccounts'
import { SIDEBAR_WIDTH, useUiPrefs } from '@/shared/hooks/useUiPrefs'
import { useMailNotifications } from '@/shared/hooks/useMailNotifications'
import { PaneResizer } from '@/shared/components/PaneResizer'
import { TopBar } from '@/widgets/TopBar'
import { Sidebar } from '@/widgets/Sidebar'

export interface MailOutletContext {
  openCompose: (state?: ComposeInitialState) => void
}

export function MailLayout() {
  const isOnline = useOnlineStatus()
  const navigate = useNavigate()
  // Watch background syncs app-wide: auto-refresh folders/messages on completion.
  useSyncActivity()
  const sidebarWidth = useUiPrefs((s) => s.sidebarWidth)
  const setSidebarWidth = useUiPrefs((s) => s.setSidebarWidth)
  // Foreground desktop notifications (SSE), alongside service-worker push.
  useMailNotifications(useUiPrefs((s) => s.notificationsEnabled))
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
          <Outlet context={{ openCompose } satisfies MailOutletContext} />
        </div>
      </main>
      <ComposeDialog
        key={composeKey}
        open={isComposeOpen}
        initialState={composeState}
        onClose={() => setIsComposeOpen(false)}
      />
    </div>
  )
}
