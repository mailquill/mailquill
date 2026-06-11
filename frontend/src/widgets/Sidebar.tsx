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
  FolderIcon,
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
import { useContacts } from '@/shared/hooks/useContacts'
import { useCalendars } from '@/shared/hooks/useCalendar'
import { useModuleNav } from '@/shared/hooks/useModuleNav'
import { useUiPrefs } from '@/shared/hooks/useUiPrefs'
import { UNIFIED_VIEWS, isUnifiedView, type UnifiedView } from '@/shared/lib/unifiedViews'
import type { Account, Folder } from '@/shared/types'

const UNIFIED_ICONS: Record<UnifiedView, ElementType> = {
  inbox: Inbox,
  starred: Star,
  sent: Send,
  drafts: FileText,
  archive: Archive,
  spam: ShieldAlert,
  trash: Trash2,
}

const FOLDER_ICONS: Record<string, ElementType> = {
  INBOX: Inbox,
  STARRED: Star,
  SENT: Send,
  DRAFTS: FileText,
  ARCHIVE: Archive,
  SPAM: ShieldAlert,
  JUNK: ShieldAlert,
  TRASH: Trash2,
  CUSTOM: FolderIcon,
}

// Standard folders pinned to the top in this order; everything else follows,
// sorted alphabetically by path.
const FOLDER_ORDER: Record<string, number> = {
  INBOX: 0,
  SENT: 1,
  DRAFTS: 2,
  SPAM: 3,
  JUNK: 3,
  TRASH: 4,
}

// Localised label per standard folder type; custom folders keep their IMAP name.
const FOLDER_TYPE_LABEL: Record<string, string> = {
  INBOX: 'sidebar.unifiedInbox',
  SENT: 'sidebar.unifiedSent',
  DRAFTS: 'sidebar.unifiedDrafts',
  SPAM: 'sidebar.unifiedSpam',
  JUNK: 'sidebar.unifiedSpam',
  TRASH: 'sidebar.unifiedTrash',
  ARCHIVE: 'sidebar.unifiedArchive',
}

function folderRank(folder: Folder): number {
  return FOLDER_ORDER[folder.folder_type] ?? 5
}

function sortFolders(folders: Folder[]): Folder[] {
  return [...folders].sort(
    (a, b) => folderRank(a) - folderRank(b) || a.full_path.localeCompare(b.full_path),
  )
}

type Module = 'mail' | 'contacts' | 'calendar'

function FolderItem({ folder, accountId }: { folder: Folder; accountId: string }) {
  const { t } = useTranslation()
  const { accountId: paramAccount, folder: paramFolder } = useParams()
  const isActive = paramAccount === accountId && paramFolder === folder.full_path
  const Icon = FOLDER_ICONS[folder.folder_type] ?? FolderIcon
  const labelKey = FOLDER_TYPE_LABEL[folder.folder_type]
  const label = labelKey ? t(labelKey) : folder.name
  const move = useMoveMessage()
  const [dropOver, setDropOver] = useState(false)

  return (
    <Link
      to={`/mail/${accountId}/${encodeURIComponent(folder.full_path)}`}
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
        'relative ml-6 flex items-center gap-2.5 rounded-md py-1.5 pl-3 pr-3 text-[12.5px] transition-colors',
        dropOver
          ? 'bg-primary/30 ring-1 ring-primary'
          : isActive
            ? 'bg-[#1e293b] font-semibold text-[#f8fafc]'
            : 'font-medium text-[#94a3b8] hover:bg-white/5',
      )}
    >
      {isActive && <span className="absolute bottom-[18%] left-0 top-[18%] w-[3px] rounded-r-sm bg-primary" />}
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

function AccountSection({ account }: { account: Account }) {
  const [expanded, setExpanded] = useState(true)
  const { data: folders } = useFolders(account.id)
  const { data: syncStatus } = useSyncStatus(account.id)
  const color = accountColor(account.id)
  const inboxUnread =
    folders?.find((f) => f.folder_type === 'INBOX')?.unread_count ??
    folders?.reduce((s, f) => s + f.unread_count, 0) ??
    0
  const lastSynced = syncStatus?.last_synced_at ? formatDate(syncStatus.last_synced_at) : 'Never'

  return (
    <div>
      <button
        onClick={() => setExpanded((e) => !e)}
        title={`Last sync ${lastSynced}`}
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
          {sortFolders(folders ?? []).map((f) => (
            <FolderItem key={f.id} folder={f} accountId={account.id} />
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
  const groups = Array.from(
    new Set(contacts.map((c) => c.group_name).filter((g): g is string => Boolean(g))),
  ).sort()

  return (
    <>
      <SidebarCta icon={Plus} label={t('sidebar.newContact')} onClick={() => navigate('/mail/contacts?new=1')} />
      <nav className="flex-1 space-y-0.5 overflow-y-auto px-2 pb-3">
        <SbRow active={contactGroup === 'all'} onClick={() => setContactGroup('all')}>
          <Users className={cn('size-4', contactGroup === 'all' ? 'text-[#f8fafc]' : 'text-[#94a3b8]')} />
          <span className="flex-1">{t('sidebar.allContacts')}</span>
          <span className="text-[11px] font-bold text-[#475569]">{contacts.length}</span>
        </SbRow>
        <SbRow active={contactGroup === 'fav'} onClick={() => setContactGroup('fav')}>
          <Star className={cn('size-4', contactGroup === 'fav' ? 'text-[#f8fafc]' : 'text-[#94a3b8]')} />
          <span className="flex-1">{t('sidebar.favourites')}</span>
          <span className="text-[11px] font-bold text-[#475569]">{contacts.filter((c) => c.favorite).length}</span>
        </SbRow>
        {groups.length > 0 && <div className={CAP}>{t('sidebar.groups')}</div>}
        {groups.map((g) => (
          <SbRow key={g} active={contactGroup === g} onClick={() => setContactGroup(g)}>
            <span className="size-4 text-center text-[#94a3b8]">#</span>
            <span className="flex-1 truncate">{g}</span>
            <span className="text-[11px] font-bold text-[#475569]">
              {contacts.filter((c) => c.group_name === g).length}
            </span>
          </SbRow>
        ))}
      </nav>
    </>
  )
}

function CalendarNav() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { data: calendars = [] } = useCalendars()
  const { hiddenCalendars, toggleCalendar } = useModuleNav()

  return (
    <>
      <SidebarCta icon={Plus} label={t('sidebar.newEvent')} onClick={() => navigate('/mail/calendar?new=1')} />
      <nav className="flex-1 space-y-0.5 overflow-y-auto px-2 pb-3">
        <div className={CAP}>{t('sidebar.myCalendars')}</div>
        {calendars.map((c) => {
          const shown = !hiddenCalendars.includes(c.id)
          return (
            <button
              key={c.id}
              onClick={() => toggleCalendar(c.id)}
              className="flex w-full items-center gap-2.5 rounded-md px-3 py-1.5 text-left text-[12.5px] text-[#cbd5e1] transition-colors hover:bg-white/5"
            >
              <span
                className="flex size-[17px] items-center justify-center rounded-[5px] border-2"
                style={{ borderColor: c.color, backgroundColor: shown ? c.color : 'transparent' }}
              >
                {shown && <Check className="size-3 text-white" strokeWidth={3} />}
              </span>
              <span className={cn('flex-1 truncate', !shown && 'text-[#64748b]')}>{c.name}</span>
            </button>
          )
        })}
        {!calendars.length && <p className="px-3 py-2 text-[12px] text-[#64748b]">{t('sidebar.noCalendars')}</p>}
      </nav>
    </>
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
