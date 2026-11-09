import { MailSearch } from '@/widgets/MailSearch'
import { ThemeMenu } from '@/widgets/topbar/ThemeMenu'
import { SystemInfoMenu } from '@/widgets/topbar/SystemInfoMenu'
import { SyncStatusMenu } from '@/widgets/topbar/SyncStatusMenu'
import { NotificationMenu } from '@/widgets/topbar/NotificationMenu'
import { AccountMenu } from '@/widgets/topbar/AccountMenu'

interface TopBarProps {
  onSettings: () => void
}

/**
 * Header above the mail panes: the search box plus the theme, system info, sync status, notification and account menus.
 * @param props - `onSettings` is forwarded to the account menu to open the settings page.
 * @returns The header bar.
 */
export function TopBar({ onSettings }: TopBarProps) {
  return (
    <header className="flex h-16 shrink-0 items-center gap-3 border-b border-border bg-background px-4">
      <MailSearch />
      <div className="flex shrink-0 items-center gap-2">
        <ThemeMenu />
        <SystemInfoMenu />
        <SyncStatusMenu />
        <NotificationMenu />
        <AccountMenu onSettings={onSettings} />
      </div>
    </header>
  )
}
