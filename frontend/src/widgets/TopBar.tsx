import { MailSearch } from '@/widgets/MailSearch'
import { ThemeMenu } from '@/widgets/topbar/ThemeMenu'
import { LicensesMenu } from '@/widgets/topbar/LicensesMenu'
import { SyncStatusMenu } from '@/widgets/topbar/SyncStatusMenu'
import { NotificationMenu } from '@/widgets/topbar/NotificationMenu'
import { AccountMenu } from '@/widgets/topbar/AccountMenu'

interface TopBarProps {
  onSettings: () => void
}

export function TopBar({ onSettings }: TopBarProps) {
  return (
    <header className="flex h-16 shrink-0 items-center gap-3 border-b border-border bg-background px-4">
      <MailSearch />
      <div className="flex shrink-0 items-center gap-2">
        <ThemeMenu />
        <LicensesMenu />
        <SyncStatusMenu />
        <NotificationMenu />
        <AccountMenu onSettings={onSettings} />
      </div>
    </header>
  )
}
