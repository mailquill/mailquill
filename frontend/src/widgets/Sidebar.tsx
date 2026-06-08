import { useState } from 'react'
import type { ElementType } from 'react'
import { Link, useParams } from 'react-router-dom'
import { Inbox, Send, Archive, Trash2, ChevronRight, RefreshCw, Settings, Plus, FolderIcon, Pencil } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { formatDate } from '@/shared/lib/format'
import { Badge } from '@/shared/components/ui/badge'
import { Button } from '@/shared/components/ui/button'
import { useAccounts, useFolders, useSyncStatus } from '@/shared/hooks/useAccounts'
import type { Folder } from '@/shared/types'

const FOLDER_ICONS: Record<string, ElementType> = {
  INBOX: Inbox,
  SENT: Send,
  ARCHIVE: Archive,
  TRASH: Trash2,
  DRAFTS: FolderIcon,
  SPAM: FolderIcon,
  CUSTOM: FolderIcon,
}

function FolderItem({ folder, accountId }: { folder: Folder; accountId: string }) {
  const { accountId: paramAccount, folder: paramFolder } = useParams()
  const isActive = paramAccount === accountId && paramFolder === folder.full_path
  const Icon = FOLDER_ICONS[folder.folder_type] ?? FolderIcon

  return (
    <Link
      to={`/mail/${accountId}/${encodeURIComponent(folder.full_path)}`}
      className={cn(
        'flex items-center gap-2 rounded-md px-3 py-1.5 text-sm transition-colors',
        isActive
          ? 'bg-primary/10 text-primary font-medium'
          : 'text-muted-foreground hover:bg-accent hover:text-foreground',
      )}
    >
      <Icon className="h-4 w-4 shrink-0" />
      <span className="flex-1 truncate">{folder.name}</span>
      {folder.unread_count > 0 && (
        <Badge variant="secondary" className="ml-auto text-xs">
          {folder.unread_count}
        </Badge>
      )}
    </Link>
  )
}

function AccountSection({ accountId, displayName }: { accountId: string; displayName: string }) {
  const [expanded, setExpanded] = useState(true)
  const { data: folders } = useFolders(accountId)
  const { data: syncStatus } = useSyncStatus(accountId)

  const totalUnread = folders?.reduce((s, f) => s + f.unread_count, 0) ?? 0
  const lastSynced = syncStatus?.last_synced_at ? formatDate(syncStatus.last_synced_at) : 'Never'
  const syncState = syncStatus?.state ?? 'idle'

  return (
    <div className="space-y-0.5">
      <button
        onClick={() => setExpanded((e) => !e)}
        className="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-accent/60"
      >
        <ChevronRight className={cn('mt-1 h-3 w-3 shrink-0 transition-transform', expanded && 'rotate-90')} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-xs font-semibold uppercase tracking-wider text-foreground">
            {displayName}
          </span>
          <span className="mt-1 flex items-center gap-1.5 text-[11px] text-muted-foreground">
            <Badge variant={syncState === 'error' ? 'destructive' : 'secondary'} className="px-1.5 py-0">
              {syncState}
            </Badge>
            <span className="truncate">Last sync {lastSynced}</span>
          </span>
        </span>
        {totalUnread > 0 && <Badge className="mt-0.5">{totalUnread}</Badge>}
        {syncStatus?.state === 'syncing' && (
          <RefreshCw className="mt-1 h-3 w-3 shrink-0 animate-spin text-muted-foreground" />
        )}
      </button>

      {expanded && (
        <div className="ml-2 space-y-0.5">
          {folders?.map((f) => (
            <FolderItem key={f.id} folder={f} accountId={accountId} />
          ))}
        </div>
      )}
    </div>
  )
}

interface SidebarProps {
  onCompose: () => void
  onAddAccount: () => void
  onSettings: () => void
}

export function Sidebar({ onCompose, onAddAccount, onSettings }: SidebarProps) {
  const { data: accounts } = useAccounts()

  return (
    <aside className="flex h-full w-64 shrink-0 flex-col border-r border-border bg-card">
      <div className="flex items-center justify-between px-4 py-3">
        <h1 className="text-base font-bold">Mailquill</h1>
        <div className="flex gap-1">
          <Button size="icon" variant="ghost" onClick={onAddAccount} title="Add account">
            <Plus className="h-4 w-4" />
          </Button>
          <Button size="icon" variant="ghost" onClick={onSettings} title="Settings">
            <Settings className="h-4 w-4" />
          </Button>
        </div>
      </div>

      <nav className="flex-1 space-y-1 overflow-y-auto px-2 py-2">
        <Button type="button" className="mb-2 w-full justify-start" onClick={onCompose}>
          <Pencil className="h-4 w-4" aria-hidden="true" />
          Compose
        </Button>

        <Link
          to="/mail/unified"
          className="flex items-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium hover:bg-accent"
        >
          <Inbox className="h-4 w-4" />
          Unified Inbox
        </Link>

        <div className="my-2 border-t border-border" />

        {accounts?.map((acct) => (
          <AccountSection key={acct.id} accountId={acct.id} displayName={acct.display_name} />
        ))}

        {!accounts?.length && (
          <p className="px-3 py-2 text-xs text-muted-foreground">
            No accounts. Add one to get started.
          </p>
        )}
      </nav>
    </aside>
  )
}
