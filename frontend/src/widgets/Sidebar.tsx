import { useState } from 'react'
import type { ElementType } from 'react'
import { useTranslation } from 'react-i18next'
import { Link, useParams, useNavigate, useLocation } from 'react-router-dom'
import {
  Inbox,
  Send,
  Archive,
  Trash2,
  ChevronRight,
  ChevronDown,
  RefreshCw,
  Plus,
  Pencil,
  Star,
  FileText,
  ShieldAlert,
  Mail,
  Users,
  CalendarDays,
  Check,
} from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { formatDate } from '@/shared/lib/format'
import { accountColor, accountInitials } from '@/shared/lib/avatar'
import { useAccounts, useFolders, useSyncStatus } from '@/shared/hooks/useAccounts'
import { useMoveMessage, useUnifiedCounts } from '@/shared/hooks/useMessages'
import { useContactAccounts, useContactGroups, useContacts } from '@/shared/hooks/useContacts'
import { useCalendars, useUpdateCalendar, useDeleteCalendar } from '@/shared/hooks/useCalendar'
import { AddCalendarDialog } from '@/widgets/AddCalendarDialog'
import { CalendarEditDialog } from '@/features/calendar'
import { useModuleNav } from '@/shared/hooks/useModuleNav'
import { useUiPrefs } from '@/shared/hooks/useUiPrefs'
import { buildFolderTree, folderLeafLabel, folderIcon, type FolderNode } from '@/shared/lib/folders'
import { UNIFIED_VIEWS, isUnifiedView, type UnifiedView } from '@/shared/lib/unifiedViews'
import type { Account, Calendar, Folder } from '@/shared/types'

const UNIFIED_ICONS: Record<UnifiedView, ElementType> = {
  inbox: Inbox,
  starred: Star,
  sent: Send,
  drafts: FileText,
  archive: Archive,
  spam: ShieldAlert,
  trash: Trash2,
}

type Module = 'mail' | 'contacts' | 'calendar'

function FolderItem({
  folder,
  accountId,
  depth = 0,
  hasChildren = false,
  expanded = false,
  onToggle,
}: {
  folder: Folder
  accountId: string
  depth?: number
  hasChildren?: boolean
  expanded?: boolean
  onToggle?: () => void
}) {
  const { t } = useTranslation()
  const { accountId: paramAccount, folder: paramFolder } = useParams()
  const isActive = paramAccount === accountId && paramFolder === folder.full_path
  const Icon = folderIcon(folder)
  const label = folderLeafLabel(folder, t)
  const move = useMoveMessage()
  const [dropOver, setDropOver] = useState(false)

  return (
    <Link
      to={`/mail/${accountId}/${encodeURIComponent(folder.full_path)}`}
      style={{ marginLeft: `${24 + depth * 14}px` }}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes('application/x-mailtastic-message')) {
          e.preventDefault()
          e.dataTransfer.dropEffect = 'move'
          if (!dropOver) setDropOver(true)
        }
      }}
      onDragLeave={() => setDropOver(false)}
      onDrop={(e) => {
        const id = e.dataTransfer.getData('application/x-mailtastic-message')
        setDropOver(false)
        if (id) {
          e.preventDefault()
          move.mutate({ id, folder_id: folder.id })
        }
      }}
      className={cn(
        'relative flex items-center gap-1.5 rounded-md py-1.5 pl-2 pr-3 text-[12.5px] transition-colors',
        dropOver
          ? 'bg-primary/30 ring-1 ring-primary'
          : isActive
            ? 'bg-[#1e293b] font-semibold text-[#f8fafc]'
            : 'font-medium text-[#94a3b8] hover:bg-white/5',
      )}
    >
      {isActive && <span className="absolute bottom-[18%] left-0 top-[18%] w-[3px] rounded-r-sm bg-primary" />}
      {hasChildren ? (
        <button
          type="button"
          onClick={(e) => {
            e.preventDefault()
            e.stopPropagation()
            onToggle?.()
          }}
          aria-expanded={expanded}
          aria-label={
            expanded
              ? t('sidebar.collapseFolder', { name: label })
              : t('sidebar.expandFolder', { name: label })
          }
          className="flex size-4 shrink-0 items-center justify-center text-[#475569] hover:text-[#cbd5e1]"
        >
          {expanded ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
        </button>
      ) : (
        <span className="size-4 shrink-0" />
      )}
      {/* folderIcon returns one of a fixed set of module-level Lucide components. */}
      {/* eslint-disable-next-line react-hooks/static-components */}
      <Icon className={cn('size-4 shrink-0', isActive ? 'text-[#f8fafc]' : 'text-[#64748b]')} />
      <span className="flex-1 truncate">{label}</span>
      {folder.unread_count > 0 && (
        <span className={cn('text-[11px] font-bold', isActive ? 'text-white' : 'text-[#64748b]')}>
          {folder.unread_count}
        </span>
      )}
    </Link>
  )
}

// One tree node + its collapsible descendants. Expansion is persisted by path.
function FolderTreeNode({ node, accountId }: { node: FolderNode; accountId: string }) {
  const expanded = useUiPrefs(
    (state) => state.expandedFoldersByMailbox[accountId]?.includes(node.folder.full_path) ?? false,
  )
  const toggleFolderExpanded = useUiPrefs((state) => state.toggleFolderExpanded)
  const hasChildren = node.children.length > 0
  return (
    <>
      <FolderItem
        folder={node.folder}
        accountId={accountId}
        depth={node.depth}
        hasChildren={hasChildren}
        expanded={expanded}
        onToggle={() => toggleFolderExpanded(accountId, node.folder.full_path)}
      />
      {hasChildren &&
        expanded &&
        node.children.map((child) => (
          <FolderTreeNode key={child.folder.id} node={child} accountId={accountId} />
        ))}
    </>
  )
}

// Per-account "starred" view: flagged mail of just this mailbox. A virtual
// entry (not a real folder), so it never appears in the move-to context menu.
function AccountStarredItem({ account }: { account: Account }) {
  const { t } = useTranslation()
  const location = useLocation()
  const isActive =
    location.pathname === '/mail/unified/starred' &&
    new URLSearchParams(location.search).get('account') === account.id
  return (
    <Link
      to={`/mail/unified/starred?account=${account.id}`}
      style={{ marginLeft: '24px' }}
      className={cn(
        'relative flex items-center gap-1.5 rounded-md py-1.5 pl-2 pr-3 text-[12.5px] transition-colors',
        isActive
          ? 'bg-[#1e293b] font-semibold text-[#f8fafc]'
          : 'font-medium text-[#94a3b8] hover:bg-white/5',
      )}
    >
      {isActive && <span className="absolute bottom-[18%] left-0 top-[18%] w-[3px] rounded-r-sm bg-primary" />}
      <span className="size-4 shrink-0" />
      <Star className={cn('size-4 shrink-0', isActive ? 'text-[#f8fafc]' : 'text-[#64748b]')} />
      <span className="flex-1 truncate">{t('sidebar.unifiedStarred')}</span>
    </Link>
  )
}

function AccountSection({ account }: { account: Account }) {
  const { t } = useTranslation()
  const expanded = useUiPrefs((state) => !state.collapsedMailboxIds.includes(account.id))
  const toggleMailboxCollapsed = useUiPrefs((state) => state.toggleMailboxCollapsed)
  const { data: folders } = useFolders(account.id)
  const { data: syncStatus } = useSyncStatus(account.id)
  const color = accountColor(account.id)
  const inboxUnread =
    folders?.find((f) => f.folder_type === 'INBOX')?.unread_count ??
    folders?.reduce((s, f) => s + f.unread_count, 0) ??
    0
  const lastSynced = syncStatus?.last_synced_at
    ? formatDate(syncStatus.last_synced_at)
    : t('syncMenu.never')

  return (
    <div>
      <button
        onClick={() => toggleMailboxCollapsed(account.id)}
        title={t('sidebar.lastSync', { time: lastSynced })}
        aria-expanded={expanded}
        aria-label={
          expanded
            ? t('sidebar.collapseMailbox', { name: account.display_name })
            : t('sidebar.expandMailbox', { name: account.display_name })
        }
        className="flex w-full items-center gap-2.5 rounded-md px-3 py-1.5 text-left transition-colors hover:bg-white/5"
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0 text-[#475569]" />
        ) : (
          <ChevronRight className="size-3 shrink-0 text-[#475569]" />
        )}
        <span
          className="flex size-[22px] shrink-0 items-center justify-center rounded-[5px] text-[9.5px] font-extrabold tracking-wide text-white"
          style={{ backgroundColor: color }}
        >
          {accountInitials(account.display_name)}
        </span>
        <span className="min-w-0 flex-1 truncate text-[12.5px] font-semibold text-[#cbd5e1]">
          {account.display_name}
        </span>
        {syncStatus?.state === 'syncing' ? (
          <RefreshCw className="size-3 shrink-0 animate-spin text-[#475569]" />
        ) : (
          inboxUnread > 0 && (
            <span className="text-[11px] font-bold" style={{ color }}>
              {inboxUnread}
            </span>
          )
        )}
      </button>
      {expanded && (
        <div className="mb-1.5 mt-0.5 space-y-0.5">
          <AccountStarredItem account={account} />
          {buildFolderTree(folders ?? []).map((node) => (
            <FolderTreeNode key={node.folder.id} node={node} accountId={account.id} />
          ))}
        </div>
      )}
    </div>
  )
}

function QuillLogo() {
  return (
    <svg width="38" height="38" viewBox="0 0 40 40" fill="none" aria-hidden="true">
      <rect x="2" y="9" width="30" height="24" rx="5" fill="#1E293B" stroke="#334155" strokeWidth="1.5" />
      <path d="M4 12 L17 22 L30 12" stroke="#64748B" strokeWidth="1.8" fill="none" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M37 5 C 28 7, 23 12, 20 22 C 28 19, 33 14, 37 5 Z" fill="#EA580C" />
      <path d="M31 10 l-3.2 1.4 M28 13 l-3.2 1.4 M25 16.5 l-3.2 1.4" stroke="#FDBA8C" strokeWidth="1" strokeLinecap="round" opacity="0.85" />
      <path d="M20 22 L33.5 8.5" stroke="#9A3412" strokeWidth="1.2" strokeLinecap="round" />
      <path d="M20 22 l-3 4.4" stroke="#EA580C" strokeWidth="2.2" strokeLinecap="round" />
    </svg>
  )
}

const CAP = 'px-3 pb-1.5 pt-3 text-[10px] font-bold uppercase tracking-[0.1em] text-[#475569]'

function ModuleSwitcher({ active }: { active: Module }) {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const items: { id: Module; label: string; icon: ElementType; to: string }[] = [
    { id: 'mail', label: t('nav.mail'), icon: Mail, to: '/mail/unified' },
    { id: 'contacts', label: t('nav.contacts'), icon: Users, to: '/mail/contacts' },
    { id: 'calendar', label: t('nav.calendar'), icon: CalendarDays, to: '/mail/calendar' },
  ]
  return (
    <div className="sticky top-0 z-[5] bg-sidebar px-3.5 pb-3">
      <div className="flex gap-1 rounded-[9px] bg-[#1e293b] p-[3px]">
        {items.map(({ id, label, icon: Icon, to }) => {
          const on = active === id
          return (
            <button
              key={id}
              onClick={() => navigate(to)}
              title={label}
              className={cn(
                'flex flex-1 flex-col items-center gap-1 rounded-[7px] px-1 py-1.5 text-[10.5px] font-bold transition-colors',
                on ? 'bg-primary text-white' : 'text-[#94a3b8] hover:bg-white/5 hover:text-[#cbd5e1]',
              )}
            >
              <Icon className="size-[17px]" />
              {label}
            </button>
          )
        })}
      </div>
    </div>
  )
}

function SidebarCta({ icon: Icon, label, onClick }: { icon: ElementType; label: string; onClick: () => void }) {
  return (
    <div className="px-3.5 pb-3">
      <button
        onClick={onClick}
        className="flex w-full items-center justify-center gap-2.5 rounded-md bg-primary px-3.5 py-2.5 text-sm font-bold text-primary-foreground shadow-[0_4px_14px_rgba(234,88,12,0.35)] transition-colors hover:bg-[#c2410c]"
      >
        <Icon className="size-4" />
        {label}
      </button>
    </div>
  )
}

function SbRow({
  active,
  onClick,
  children,
}: {
  active?: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'relative flex w-full items-center gap-2.5 rounded-md px-3 py-1.5 text-left text-[13px] transition-colors',
        active ? 'bg-[#1e293b] font-semibold text-[#f8fafc]' : 'font-medium text-[#cbd5e1] hover:bg-white/5',
      )}
    >
      {active && <span className="absolute bottom-[18%] left-0 top-[18%] w-[3px] rounded-r-sm bg-primary" />}
      {children}
    </button>
  )
}

function ContactsNav() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { contactGroup, setContactGroup } = useModuleNav()
  const { data: contacts = [] } = useContacts()
  const { data: accounts = [] } = useContactAccounts()
  const { data: groups = [] } = useContactGroups()

  return (
    <>
      <SidebarCta icon={Plus} label={t('sidebar.newContact')} onClick={() => navigate('/mail/contacts?new=1')} />
      <nav className="flex-1 space-y-0.5 overflow-y-auto px-2 pb-3">
        <SbRow active={contactGroup === 'all'} onClick={() => setContactGroup('all')}>
          <Users className={cn('size-4', contactGroup === 'all' ? 'text-[#f8fafc]' : 'text-[#94a3b8]')} />
          <span className="flex-1">{t('sidebar.allContacts')}</span>
          <span className="text-[11px] font-bold text-[#475569]">{contacts.length}</span>
        </SbRow>
        {accounts.length > 0 && <div className={CAP}>{t('sidebar.accounts')}</div>}
        {accounts.map((account) => (
          <SbRow key={account.id} active={contactGroup === account.id} onClick={() => setContactGroup(account.id)}>
            <span className="size-4 text-center text-[#94a3b8]">
              {account.type === 'google' ? 'G' : account.type === 'graph' ? 'M' : 'C'}
            </span>
            <span className="flex-1 truncate">{account.display_name}</span>
            <span className="text-[11px] font-bold text-[#475569]">
              {contacts.filter((c) => c.account_id === account.id).length}
            </span>
          </SbRow>
        ))}
        {groups.length > 0 && <div className={CAP}>{t('sidebar.groups')}</div>}
        {groups.map((group) => {
          const selection = `group:${group.id}`
          return (
            <SbRow key={group.id} active={contactGroup === selection} onClick={() => setContactGroup(selection)}>
              <Users className={cn('size-4', contactGroup === selection ? 'text-[#f8fafc]' : 'text-[#94a3b8]')} />
              <span className="flex-1 truncate" title={group.name}>{group.name}</span>
              <span className="text-[11px] font-bold tabular-nums text-[#475569]" title={t('sidebar.groupMembers', { count: group.member_count })}>
                {group.member_count}
              </span>
            </SbRow>
          )
        })}
      </nav>
    </>
  )
}

function CalendarNav() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { data: calendars = [] } = useCalendars()
  const { data: accounts = [] } = useAccounts()
  const { hiddenCalendars, toggleCalendar } = useModuleNav()
  const grouping = useUiPrefs((s) => s.calendarGrouping)
  const [addOpen, setAddOpen] = useState(false)

  const row = (c: Calendar) => (
    <CalendarRow
      key={c.id}
      calendar={c}
      account={accounts.find((account) => account.id === c.account_id)}
      shown={!hiddenCalendars.includes(c.id)}
      onToggle={() => toggleCalendar(c.id)}
    />
  )

  // Group by owning account (preserving calendar order); local calendars (no
  // account) collect under their own heading.
  const groups: [string | null, Calendar[]][] = []
  for (const c of calendars) {
    const key = c.account_id ?? null
    const existing = groups.find(([k]) => k === key)
    if (existing) existing[1].push(c)
    else groups.push([key, [c]])
  }
  const accountLabel = (id: string | null) => {
    if (!id) return t('sidebar.localCalendars')
    const a = accounts.find((x) => x.id === id)
    return a?.display_name || a?.primary_email || t('sidebar.localCalendars')
  }

  return (
    <>
      <SidebarCta icon={Plus} label={t('sidebar.newEvent')} onClick={() => navigate('/mail/calendar?new=1')} />
      <nav className="flex-1 space-y-0.5 overflow-y-auto px-2 pb-3">
        <div className="flex items-center justify-between pr-2">
          <span className={CAP}>{t('sidebar.myCalendars')}</span>
          <button
            onClick={() => setAddOpen(true)}
            title={t('sidebar.newCalendar')}
            className="rounded p-1 text-[#94a3b8] transition-colors hover:bg-white/5 hover:text-white"
          >
            <Plus className="size-3.5" />
          </button>
        </div>

        {grouping === 'flat'
          ? calendars.map(row)
          : groups.map(([accId, cals]) => (
              <div key={accId ?? 'local'} className="mb-1">
                <div className="px-3 pb-1 pt-1.5 text-[10px] font-semibold uppercase tracking-wide text-[#64748b]">
                  {accountLabel(accId)}
                </div>
                {cals.map(row)}
              </div>
            ))}

        {!calendars.length && (
          <button
            onClick={() => setAddOpen(true)}
            className="mx-1 mt-1 flex w-[calc(100%-0.5rem)] items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12.5px] text-[#94a3b8] transition-colors hover:bg-white/5 hover:text-white"
          >
            <Plus className="size-4" />
            {t('sidebar.createCalendar')}
          </button>
        )}
      </nav>
      <AddCalendarDialog open={addOpen} onClose={() => setAddOpen(false)} />
    </>
  )
}

function CalendarRow({
  calendar: c,
  account,
  shown,
  onToggle,
}: {
  calendar: Calendar
  account?: Account
  shown: boolean
  onToggle: () => void
}) {
  const { t } = useTranslation()
  const updateCalendar = useUpdateCalendar()
  const deleteCalendar = useDeleteCalendar()
  const [editOpen, setEditOpen] = useState(false)

  function remove() {
    if (window.confirm(t('sidebar.deleteCalendarConfirm', { name: c.name }))) deleteCalendar.mutate(c.id)
  }

  return (
    <div className="group flex items-center rounded-md pr-1.5 transition-colors hover:bg-white/5">
      <button
        onClick={onToggle}
        className="flex min-w-0 flex-1 items-center gap-2.5 px-3 py-1.5 text-left text-[12.5px] text-[#cbd5e1]"
      >
        <span
          className="flex size-[17px] shrink-0 items-center justify-center rounded-[5px] border-2"
          style={{ borderColor: c.color, backgroundColor: shown ? c.color : 'transparent' }}
        >
          {shown && <Check className="size-3 text-white" strokeWidth={3} />}
        </span>
        <span className={cn('flex-1 truncate', !shown && 'text-[#64748b]')}>{c.name}</span>
      </button>
      <label className="shrink-0 cursor-pointer opacity-0 transition-opacity group-hover:opacity-100" title={t('sidebar.calendarColor')}>
        <span className="block size-4 rounded-full border border-white/20" style={{ backgroundColor: c.color }} />
        <input
          type="color"
          value={c.color}
          onChange={(e) => updateCalendar.mutate({ id: c.id, color: e.currentTarget.value })}
          className="sr-only"
        />
      </label>
      <button
        onClick={() => setEditOpen(true)}
        title={t('sidebar.editCalendar')}
        aria-label={t('sidebar.editCalendar')}
        className="ml-1 shrink-0 p-0.5 text-[#94a3b8] opacity-0 transition-opacity hover:text-white group-hover:opacity-100"
      >
        <Pencil className="size-3.5" />
      </button>
      <button
        onClick={remove}
        title={t('action.delete')}
        className="ml-0.5 shrink-0 p-0.5 text-[#94a3b8] opacity-0 transition-opacity hover:text-red-400 group-hover:opacity-100"
      >
        <Trash2 className="size-3.5" />
      </button>
      {editOpen && (
        <CalendarEditDialog open calendar={c} account={account} onClose={() => setEditOpen(false)} />
      )}
    </div>
  )
}

interface SidebarProps {
  onCompose: () => void
}

function UnifiedSection() {
  const { t } = useTranslation()
  const { view: viewParam, accountId } = useParams()
  const { pathname } = useLocation()
  const { data: counts } = useUnifiedCounts()
  const onUnified = !accountId && pathname.startsWith('/mail/unified')
  const activeView = isUnifiedView(viewParam) ? viewParam : 'inbox'

  return (
    <div className="px-2 pb-1">
      <div className={CAP.replace('pt-3', 'pt-1')}>{t('sidebar.unified')}</div>
      {UNIFIED_VIEWS.map(({ id, labelKey }) => {
        const Icon = UNIFIED_ICONS[id]
        const to = id === 'inbox' ? '/mail/unified' : `/mail/unified/${id}`
        const active = onUnified && activeView === id
        const count = counts?.[id] ?? 0
        return (
          <Link
            key={id}
            to={to}
            className={cn(
              'relative flex items-center gap-2.5 rounded-md px-3 py-1.5 text-[13px] transition-colors',
              active
                ? 'bg-[#1e293b] font-semibold text-[#f8fafc]'
                : 'font-medium text-[#cbd5e1] hover:bg-white/5',
            )}
          >
            {active && (
              <span className="absolute bottom-[18%] left-0 top-[18%] w-[3px] rounded-r-sm bg-primary" />
            )}
            <Icon className={cn('size-4 shrink-0', active ? 'text-[#f8fafc]' : 'text-[#94a3b8]')} />
            <span className="flex-1 truncate">{t(labelKey)}</span>
            {count > 0 &&
              (id === 'starred' ? (
                <span className="text-[11px] font-bold text-[#64748b]">{count}</span>
              ) : (
                <span className="rounded-full bg-primary px-1.5 text-[11px] font-bold leading-5 text-primary-foreground">
                  {count}
                </span>
              ))}
          </Link>
        )
      })}
    </div>
  )
}

export function Sidebar({ onCompose }: SidebarProps) {
  const { t } = useTranslation()
  const { pathname } = useLocation()
  const { data: accounts } = useAccounts()
  const sidebarWidth = useUiPrefs((s) => s.sidebarWidth)
  const module: Module = pathname.startsWith('/mail/contacts')
    ? 'contacts'
    : pathname.startsWith('/mail/calendar')
      ? 'calendar'
      : 'mail'

  return (
    <aside
      className="flex h-full shrink-0 flex-col border-r border-[#1e293b] bg-sidebar"
      style={{ width: sidebarWidth }}
    >
      <div className="flex items-center gap-2.5 px-4 pb-3 pt-4">
        <QuillLogo />
        <span className="text-base font-extrabold leading-none tracking-tight text-[#f8fafc]">Mailquill</span>
      </div>

      <ModuleSwitcher active={module} />

      {module === 'mail' && (
        <>
          <SidebarCta icon={Pencil} label={t('sidebar.compose')} onClick={onCompose} />
          <UnifiedSection />
          <nav className="flex-1 space-y-0.5 overflow-y-auto border-t border-[#1e293b] px-2 pb-3 pt-1">
            <div className={CAP}>{t('sidebar.accounts')}</div>
            {accounts?.map((account) => (
              <AccountSection key={account.id} account={account} />
            ))}
            {!accounts?.length && <p className="px-3 py-2 text-xs text-[#64748b]">{t('sidebar.noAccounts')}</p>}
          </nav>
        </>
      )}

      {module === 'contacts' && <ContactsNav />}
      {module === 'calendar' && <CalendarNav />}
    </aside>
  )
}
