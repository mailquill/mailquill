import { useState } from 'react'
import { Outlet } from 'react-router-dom'
import { AddAccountDialog } from '@/features/accounts'
import { ComposeDialog } from '@/features/compose'
import type { ComposeInitialState } from '@/features/compose'
import { SettingsDialog } from '@/features/settings'
import { useOnlineStatus } from '@/shared/hooks/useOnlineStatus'
import { SearchBar } from '@/widgets/SearchBar'
import { Sidebar } from '@/widgets/Sidebar'

export interface MailOutletContext {
  openCompose: (state?: ComposeInitialState) => void
}

export function MailLayout() {
  const isOnline = useOnlineStatus()
  const [isAddAccountOpen, setIsAddAccountOpen] = useState(false)
  const [isSettingsOpen, setIsSettingsOpen] = useState(false)
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
      <Sidebar
        onCompose={() => openCompose()}
        onAddAccount={() => setIsAddAccountOpen(true)}
        onSettings={() => setIsSettingsOpen(true)}
      />
      <main className="flex min-w-0 flex-1 flex-col">
        {!isOnline ? (
          <div className="border-b border-border bg-muted px-4 py-2 text-sm font-medium text-muted-foreground">
            Offline
          </div>
        ) : null}
        <header className="flex h-14 shrink-0 items-center border-b border-border bg-background px-4">
          <SearchBar />
        </header>
        <div className="min-h-0 flex-1">
          <Outlet context={{ openCompose } satisfies MailOutletContext} />
        </div>
      </main>
      <AddAccountDialog open={isAddAccountOpen} onClose={() => setIsAddAccountOpen(false)} />
      <SettingsDialog open={isSettingsOpen} onClose={() => setIsSettingsOpen(false)} />
      <ComposeDialog
        key={composeKey}
        open={isComposeOpen}
        initialState={composeState}
        onClose={() => setIsComposeOpen(false)}
      />
    </div>
  )
}
