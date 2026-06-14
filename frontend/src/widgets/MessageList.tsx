import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useVirtualizer } from '@tanstack/react-virtual'
import { cn } from '@/shared/lib/utils'
import { formatDate, parseFromAddr, listIdToName } from '@/shared/lib/format'
import { accountColor } from '@/shared/lib/avatar'
import { useUiPrefs } from '@/shared/hooks/useUiPrefs'
import {
  useMarkRead,
  useArchiveMessage,
  useDeleteMessage,
  useDeleteThread,
  useToggleFlag,
  useBulkAction,
  type BulkScope,
} from '@/shared/hooks/useMessages'
import { Star, Check, MailOpen, Archive, Trash2, ShieldAlert } from 'lucide-react'
import { MessageContextMenu, type ContextMenuState } from '@/widgets/MessageContextMenu'
import type { Message } from '@/shared/types'

interface MessageRowProps {
  message: Message
  index: number
  isActive: boolean
  checked: boolean
  onClick: () => void
  onToggleCheck: (id: string) => void
  onToggleFlag: (message: Message) => void
  onContextMenu: (event: React.MouseEvent, message: Message) => void
  onRowMouseDown: (index: number, id: string) => void
  onRowMouseEnter: (index: number) => void
}

const DENSITY_PAD: Record<string, string> = {
  compact: 'py-1.5',
  comfortable: 'py-2.5',
  roomy: 'py-3.5',
}

function Checkbox({ checked }: { checked: boolean }) {
  return (
    <span
      className={cn(
        'flex size-[18px] shrink-0 items-center justify-center rounded border',
        checked ? 'border-transparent bg-[#2563eb]' : 'border-input bg-card',
      )}
    >
      {checked && <Check className="size-3 text-white" strokeWidth={3} />}
    </span>
  )
}

export function MessageRow({
  message,
  index,
  isActive,
  checked,
  onClick,
  onToggleCheck,
  onToggleFlag,
  onContextMenu,
  onRowMouseDown,
  onRowMouseEnter,
}: MessageRowProps) {
  const { t } = useTranslation()
  const density = useUiPrefs((s) => s.density)
  const marker = useUiPrefs((s) => s.marker)
  const { name } = parseFromAddr(message.from_addr)
  const listName = listIdToName(message.list_id)
  const date = formatDate(message.internal_date)
  const participants = message.thread_participants
    ?.slice(0, 3)
    .map((participant) => parseFromAddr(participant).name)
    .join(', ')
  const senderLabel = participants || name
  // A row is unread if the thread has any unread message (falls back to the
  // representative message's own read state for non-threaded rows).
  const unread = message.thread_unread != null ? message.thread_unread > 0 : !message.is_read
  const count = message.thread_size ?? 1
  const account = accountColor(message.account_id)
  const compact = density === 'compact'
  const showStripe = marker !== 'dot'
  const showAccountDot = marker !== 'stripe'

  return (
    <button
      onClick={() => onClick()}
      onContextMenu={(e) => onContextMenu(e, message)}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.effectAllowed = 'move'
        e.dataTransfer.setData('application/x-mailtastic-message', message.id)
        e.dataTransfer.setData('text/plain', message.id)
      }}
      onMouseEnter={() => onRowMouseEnter(index)}
      className={cn(
        'relative flex w-full select-none items-start gap-2.5 border-b border-secondary pl-4 pr-3 text-left transition-colors',
        DENSITY_PAD[density],
        checked ? 'bg-[var(--mq-bulk)]' : isActive ? 'bg-[var(--mq-row-open)]' : 'bg-card hover:bg-secondary/60',
      )}
    >
      {showStripe && (
        <span
          className="absolute bottom-0 left-0 top-0 w-[3px]"
          style={{ backgroundColor: isActive ? '#2563EB' : account }}
        />
      )}

      <span className="mt-0.5 flex shrink-0 items-center gap-2">
        <span
          role="checkbox"
          aria-checked={checked}
          aria-label={t('ml.select')}
          tabIndex={0}
          onMouseDown={(e) => {
            if (e.button !== 0) return
            e.preventDefault()
            e.stopPropagation()
            onRowMouseDown(index, message.id)
          }}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            if (e.key === ' ' || e.key === 'Enter') {
              e.preventDefault()
              e.stopPropagation()
              onToggleCheck(message.id)
            }
          }}
          className="cursor-pointer"
        >
          <Checkbox checked={checked} />
        </span>
        <span
          role="button"
          aria-label={t('ml.flag')}
          aria-pressed={message.is_flagged}
          tabIndex={0}
          onMouseDown={(e) => {
            if (e.button !== 0) return
            e.preventDefault()
            e.stopPropagation()
            onToggleFlag(message)
          }}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            if (e.key === ' ' || e.key === 'Enter') {
              e.preventDefault()
              e.stopPropagation()
              onToggleFlag(message)
            }
          }}
          className="cursor-pointer"
        >
          <Star
            className={cn(
              'size-4 shrink-0',
              message.is_flagged ? 'fill-primary text-primary' : 'text-input hover:text-primary',
            )}
          />
        </span>
      </span>

      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex h-6 items-center gap-1.5">
          {showAccountDot ? (
            <span
              className={cn('size-2 shrink-0 rounded-full', unread && 'ring-2 ring-[#2563eb]/30')}
              style={{ backgroundColor: account }}
            />
          ) : (
            unread && <span className="size-2 shrink-0 rounded-full bg-[#2563eb]" />
          )}
          <span
            className={cn(
              'max-w-[240px] truncate text-[13.5px]',
              unread ? 'font-bold text-foreground' : 'font-semibold text-secondary-foreground',
            )}
          >
            {senderLabel}
          </span>
          {(message.phishing_verdict === 'phishing' || message.phishing_verdict === 'suspicious') && (
            <ShieldAlert
              className={cn(
                'size-3.5 shrink-0',
                message.phishing_verdict === 'phishing' ? 'text-red-500' : 'text-amber-500',
              )}
            />
          )}
          {count > 1 && (
            <span className="shrink-0 rounded-full bg-secondary px-1.5 text-[11px] font-bold leading-4 text-[var(--mq-text-3)]">
              {count}
            </span>
          )}
          {listName && (
            <span className="shrink-0 truncate rounded-[4px] bg-secondary px-1.5 text-[10px] font-bold text-[var(--mq-text-3)]">
              {listName}
            </span>
          )}
          {compact && (
            <span
              className={cn(
                'min-w-0 flex-1 truncate text-[13px]',
                unread ? 'font-semibold text-foreground' : 'text-secondary-foreground',
              )}
            >
              {message.subject || '(no subject)'}
            </span>
          )}
          <span
            className={cn(
              'ml-auto shrink-0 text-[11.5px] tabular-nums',
              unread ? 'font-bold text-foreground' : 'font-medium text-[var(--mq-text-3)]',
            )}
          >
            {date}
          </span>
        </div>

        {!compact && (
          <>
            <div
              className={cn(
                'truncate text-[13.5px] tracking-[-0.005em]',
                unread ? 'font-bold text-foreground' : 'font-semibold text-secondary-foreground',
              )}
            >
              {message.subject || '(no subject)'}
            </div>
            <div
              className={cn(
                'text-[13px] leading-snug text-[var(--mq-text-3)]',
                density === 'roomy' ? 'line-clamp-2' : 'truncate',
              )}
            >
              {message.snippet}
            </div>
          </>
        )}
      </div>
    </button>
  )
}

interface MessageListProps {
  messages: Message[]
  activeId?: string
  onSelect: (msg: Message) => void
  onRefresh?: () => void
  loading?: boolean
  /** Infinite scroll: fetch the next page when scrolling near the end. */
  onLoadMore?: () => void
  hasMore?: boolean
  loadingMore?: boolean
  /** Total conversations matching the view/folder, for "select all N". */
  total?: number
  /** Scope for server-side "select all" bulk actions. Omit to disable it. */
  scope?: BulkScope
}

// Row-height estimates per density; real heights are measured after render.
const DENSITY_ESTIMATE: Record<string, number> = {
  compact: 38,
  comfortable: 86,
  roomy: 112,
}

export function MessageList({
  messages,
  activeId,
  onSelect,
  loading,
  onLoadMore,
  hasMore = false,
  loadingMore = false,
  total,
  scope,
}: MessageListProps) {
  const { t } = useTranslation()
  const [menu, setMenu] = useState<ContextMenuState | null>(null)
  const [checked, setChecked] = useState<Set<string>>(() => new Set())
  // True once the user opts into "select all N matching" (beyond the loaded page).
  const [selectAllMatching, setSelectAllMatching] = useState(false)
  const density = useUiPrefs((s) => s.density)
  const scrollRef = useRef<HTMLDivElement>(null)

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => DENSITY_ESTIMATE[density] ?? 86,
    overscan: 12,
    getItemKey: (i) => messages[i]?.id ?? i,
  })

  const virtualItems = virtualizer.getVirtualItems()
  const lastVisibleIndex = virtualItems.at(-1)?.index ?? 0

  useEffect(() => {
    if (hasMore && !loadingMore && onLoadMore && lastVisibleIndex >= messages.length - 15) {
      onLoadMore()
    }
  }, [hasMore, loadingMore, onLoadMore, lastVisibleIndex, messages.length])

  const markRead = useMarkRead()
  const archive = useArchiveMessage()
  const remove = useDeleteMessage()
  const deleteThread = useDeleteThread()
  const toggleFlag = useToggleFlag()
  const bulkAction = useBulkAction()

  // Delete/Entf deletes the open conversation (mirrors the reading-pane trash).
  // Ignored while typing in a field so it doesn't eat text edits.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Delete' || !activeId) return
      const el = document.activeElement as HTMLElement | null
      if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)) return
      const msg = messages.find((m) => m.id === activeId || m.thread_id === activeId)
      if (!msg) return
      e.preventDefault()
      deleteThread.mutate(msg.thread_id ?? msg.id)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [activeId, messages, deleteThread])

  // drag-to-select: paint a contiguous range from the press row; "additive"
  // mirrors the anchor's start state so dragging over selected rows deselects.
  const drag = useRef({ active: false, anchor: -1, base: new Set<string>(), additive: true })

  useEffect(() => {
    const up = () => {
      drag.current.active = false
    }
    window.addEventListener('mouseup', up)
    return () => window.removeEventListener('mouseup', up)
  }, [])

  function applyRange(toIndex: number) {
    const d = drag.current
    if (!d.active || d.anchor < 0) return
    const lo = Math.min(d.anchor, toIndex)
    const hi = Math.max(d.anchor, toIndex)
    const next = new Set(d.base)
    for (let i = lo; i <= hi; i++) {
      const msg = messages[i]
      if (!msg) continue
      if (d.additive) next.add(msg.id)
      else next.delete(msg.id)
    }
    setChecked(next)
  }

  function onRowMouseDown(index: number, id: string) {
    drag.current = { active: true, anchor: index, base: new Set(checked), additive: !checked.has(id) }
    applyRange(index)
  }

  function onRowMouseEnter(index: number) {
    if (drag.current.active) applyRange(index)
  }

  function toggleCheck(id: string) {
    setChecked((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  function bulk(action: 'read' | 'archive' | 'delete' | 'flag') {
    if (selectAllMatching && scope) {
      // Whole view/folder — one server-side operation.
      bulkAction.mutate({ action, ...scope })
    } else {
      for (const id of checked) {
        if (action === 'read') markRead.mutate({ id, is_read: true })
        else if (action === 'archive') archive.mutate(id)
        else if (action === 'flag') toggleFlag.mutate({ id, is_flagged: true })
        else remove.mutate(id)
      }
    }
    setChecked(new Set())
    setSelectAllMatching(false)
  }

  function toggleSelectAll() {
    const allOnPage = messages.length > 0 && messages.every((m) => checked.has(m.id))
    // Header checkbox selects only the loaded page (≤ PAGE_SIZE). When more
    // match than are loaded, the "select all N" button (rendered below) escalates
    // to the whole view via the server-side bulk path. A second click clears it.
    setSelectAllMatching(false)
    setChecked(allOnPage ? new Set() : new Set(messages.map((m) => m.id)))
  }

  function openMenu(event: React.MouseEvent, message: Message) {
    event.preventDefault()
    setMenu({ x: event.clientX, y: event.clientY, message })
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">{t('mail.loading')}</div>
    )
  }

  if (!messages.length) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">{t('mail.noMessages')}</div>
    )
  }

  const allOnPageChecked = messages.length > 0 && messages.every((m) => checked.has(m.id))
  const anyChecked = checked.size > 0 || selectAllMatching
  const moreThanLoaded = total != null && total > messages.length
  const selectedCount = selectAllMatching ? total ?? checked.size : checked.size
  const checkboxChecked = selectAllMatching || allOnPageChecked

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* persistent bar: conversation count when idle, bulk actions when selecting */}
      <div
        className={cn(
          'flex h-11 shrink-0 items-center gap-2.5 border-b border-secondary px-3',
          anyChecked ? 'bg-[var(--mq-bulk)]' : 'bg-card',
        )}
      >
        <span
          role="checkbox"
          aria-checked={checkboxChecked}
          aria-label={t('ml.select')}
          tabIndex={0}
          onClick={toggleSelectAll}
          onKeyDown={(e) => {
            if (e.key === ' ' || e.key === 'Enter') {
              e.preventDefault()
              toggleSelectAll()
            }
          }}
          className="cursor-pointer"
        >
          <Checkbox checked={checkboxChecked} />
        </span>

        {anyChecked ? (
          <>
            <span className="text-[12.5px] font-semibold text-[#1d4ed8]">
              {t('ml.selected', { n: selectedCount })}
            </span>
            {!selectAllMatching && allOnPageChecked && moreThanLoaded && scope && (
              <button
                type="button"
                onClick={() => setSelectAllMatching(true)}
                className="rounded-md px-2 py-1 text-[12px] font-semibold text-[#1d4ed8] transition-colors hover:bg-[#dbeafe] dark:hover:bg-[#1e293b]"
              >
                {t('ml.selectAllMatching', { n: total })}
              </button>
            )}
            <div className="ml-auto flex gap-1">
              <BulkButton icon={MailOpen} label={t('ml.read')} onClick={() => bulk('read')} />
              <BulkButton icon={Archive} label={t('action.archive')} onClick={() => bulk('archive')} />
              <BulkButton icon={Trash2} label={t('action.delete')} onClick={() => bulk('delete')} />
              <BulkButton icon={Star} label={t('ml.flag')} onClick={() => bulk('flag')} />
            </div>
          </>
        ) : (
          <span className="text-[12.5px] font-medium text-muted-foreground">
            {t('ml.conversations', { n: total ?? messages.length })}
          </span>
        )}
      </div>

      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
          {virtualItems.map((item) => {
            const msg = messages[item.index]
            if (!msg) return null
            return (
              <div
                key={item.key}
                data-index={item.index}
                ref={virtualizer.measureElement}
                className="absolute left-0 top-0 w-full"
                style={{ transform: `translateY(${item.start}px)` }}
              >
                <MessageRow
                  message={msg}
                  index={item.index}
                  isActive={msg.id === activeId}
                  checked={checked.has(msg.id)}
                  onClick={() => onSelect(msg)}
                  onToggleCheck={toggleCheck}
                  onToggleFlag={(m) => toggleFlag.mutate({ id: m.id, is_flagged: !m.is_flagged })}
                  onContextMenu={openMenu}
                  onRowMouseDown={onRowMouseDown}
                  onRowMouseEnter={onRowMouseEnter}
                />
              </div>
            )
          })}
        </div>
        {loadingMore && (
          <div className="flex items-center justify-center py-3 text-xs text-muted-foreground">
            {t('mail.loading')}
          </div>
        )}
        {menu && <MessageContextMenu state={menu} onClose={() => setMenu(null)} />}
      </div>
    </div>
  )
}

function BulkButton({ icon: Icon, label, onClick }: { icon: typeof MailOpen; label: string; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      title={label}
      className="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] font-semibold text-[#1d4ed8] transition-colors hover:bg-[#dbeafe] dark:hover:bg-[#1e293b]"
    >
      <Icon className="size-4" />
      {label}
    </button>
  )
}
