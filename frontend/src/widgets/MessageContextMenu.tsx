import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import { MailCheck, Mail, Archive, Trash2, FolderInput, ChevronRight, Wand2, ShieldCheck } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { useClickOutside } from '@/shared/hooks/useClickOutside'
import { useFolders } from '@/shared/hooks/useAccounts'
import {
  useArchiveMessage,
  useDeleteMessage,
  useMarkRead,
  useMoveMessage,
  useNotSpamMessage,
} from '@/shared/hooks/useMessages'
import { parseFromAddr } from '@/shared/lib/format'
import { buildFolderTree, flattenFolderTree, folderLeafLabel, folderIcon } from '@/shared/lib/folders'
import type { Message } from '@/shared/types'

export interface ContextMenuState {
  x: number
  y: number
  message: Message
}

export function MessageContextMenu({ state, onClose }: { state: ContextMenuState; onClose: () => void }) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { message } = state
  const ref = useClickOutside<HTMLDivElement>(onClose, true)
  const [submenu, setSubmenu] = useState(false)
  const { data: folders = [] } = useFolders(message.account_id)
  const markRead = useMarkRead()
  const archive = useArchiveMessage()
  const remove = useDeleteMessage()
  const move = useMoveMessage()
  const notSpam = useNotSpamMessage()
  const isSpam = message.folder_type === 'SPAM' || message.folder_type === 'JUNK'

  function run(fn: () => void) {
    fn()
    onClose()
  }

  // keep the menu inside the viewport
  const x = Math.min(state.x, window.innerWidth - 240)
  const y = Math.min(state.y, window.innerHeight - 320)

  return (
    <div
      ref={ref}
      className="fixed z-[100] w-56 rounded-xl border border-border bg-popover p-1.5 shadow-2xl"
      style={{ left: x, top: y }}
    >
      <Item
        icon={message.is_read ? Mail : MailCheck}
        label={message.is_read ? t('action.markUnread') : t('action.markRead')}
        onClick={() => run(() => markRead.mutate({ id: message.id, is_read: !message.is_read }))}
      />
      <Item icon={Archive} label={t('action.archive')} onClick={() => run(() => archive.mutate(message.id))} />
      {isSpam && (
        <Item
          icon={ShieldCheck}
          label={t('action.notSpam')}
          onClick={() => run(() => notSpam.mutate(message.id))}
        />
      )}

      {/* Move to submenu */}
      <div className="relative" onMouseEnter={() => setSubmenu(true)} onMouseLeave={() => setSubmenu(false)}>
        <button className="flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-[13px] text-secondary-foreground transition-colors hover:bg-secondary">
          <FolderInput className="size-4 text-muted-foreground" />
          <span className="flex-1">{t('action.moveTo')}</span>
          <ChevronRight className="size-3.5 text-muted-foreground" />
        </button>
        {submenu && folders.length > 0 && (
          <div className="absolute left-full top-0 pl-1">
            <div className="max-h-72 w-52 overflow-y-auto rounded-xl border border-border bg-popover p-1.5 shadow-2xl">
              {flattenFolderTree(buildFolderTree(folders))
                .filter((n) => n.folder.id !== message.folder_id)
                .map((n) => {
                  const Icon = folderIcon(n.folder)
                  return (
                    <button
                      key={n.folder.id}
                      onClick={() => run(() => move.mutate({ id: message.id, folder_id: n.folder.id }))}
                      style={{ paddingLeft: `${10 + n.depth * 14}px` }}
                      className="flex w-full items-center gap-2 rounded-md py-1.5 pr-2.5 text-left text-[12.5px] text-secondary-foreground hover:bg-secondary"
                    >
                      <Icon className="size-4 shrink-0 text-muted-foreground" />
                      <span className="flex-1 truncate">{folderLeafLabel(n.folder, t)}</span>
                    </button>
                  )
                })}
            </div>
          </div>
        )}
      </div>

      <Item
        icon={Wand2}
        label={t('action.createRule')}
        onClick={() =>
          run(() =>
            navigate(`/mail/settings?section=rules&from=${encodeURIComponent(parseFromAddr(message.from_addr).email)}`),
          )
        }
      />

      <div className="my-1 border-t border-border" />
      <Item icon={Trash2} label={t('action.delete')} danger onClick={() => run(() => remove.mutate(message.id))} />
    </div>
  )
}

function Item({
  icon: Icon,
  label,
  onClick,
  danger,
}: {
  icon: typeof Mail
  label: string
  onClick: () => void
  danger?: boolean
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-[13px] transition-colors hover:bg-secondary',
        danger ? 'text-destructive' : 'text-secondary-foreground',
      )}
    >
      <Icon className={cn('size-4', danger ? 'text-destructive' : 'text-muted-foreground')} />
      {label}
    </button>
  )
}
