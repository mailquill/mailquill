// Optimistic edits of the cached mail lists and sidebar badge counts.
//
// Every list action (read, flag, delete, archive, move) rewrites the TanStack
// Query cache first so the UI reacts instantly, and returns a restore() that
// puts the snapshots back when the request fails. The reconciling refetch in
// the mutation's onSettled then confirms the state server-side.
//
// Badge rules: the backend keeps `folders.unread_count` exact through DB
// triggers, so a read/visibility change is reflected in the next counts
// refetch. Only count what the backend confirms synchronously — the source
// unread drop and the flagged count — never a destination rise (Trash grows
// only after the queued IMAP move lands).
import type { QueryClient, QueryKey } from '@tanstack/react-query'
import type { Message } from '@/shared/types'
import type { UnifiedCounts, UnifiedView } from '@/shared/lib/unifiedViews'

/** Raw cache shape of the infinite mail-list queries (before `select` flattens). */
export type ListPage = { messages: Message[]; total?: number; next_cursor: string | null }
export type InfiniteList = { pages: ListPage[]; pageParams: unknown[] }

type Snapshot = [QueryKey, InfiniteList | undefined]

// Unified views whose sidebar badge is an *unread* count. `starred` is special:
// its badge counts flagged messages, not unread ones.
const UNREAD_VIEWS: UnifiedView[] = ['inbox', 'sent', 'drafts', 'archive', 'spam', 'trash']

function unifiedViewOfKey(key: QueryKey): UnifiedView | null {
  return key[0] === 'unified' ? (key[1] as UnifiedView) : null
}

/**
 * Stop in-flight list requests before editing the cache. A request already in
 * flight would otherwise resolve *after* the edit and write the pre-edit rows
 * back, leaving a row unread (or resurrected) until the next refetch.
 */
async function cancelMailLists(qc: QueryClient): Promise<void> {
  await Promise.all([
    qc.cancelQueries({ queryKey: ['unified'] }),
    qc.cancelQueries({ queryKey: ['folder-messages'] }),
    qc.cancelQueries({ queryKey: ['unified-counts'] }),
  ])
}

function listSnapshots(qc: QueryClient): Snapshot[] {
  return [
    ...qc.getQueriesData<InfiniteList>({ queryKey: ['unified'] }),
    ...qc.getQueriesData<InfiniteList>({ queryKey: ['folder-messages'] }),
  ]
}

function restoreSnapshots(qc: QueryClient, snaps: Snapshot[], prevCounts: UnifiedCounts | undefined) {
  return () => {
    if (prevCounts) qc.setQueryData(['unified-counts'], prevCounts)
    for (const [key, data] of snaps) qc.setQueryData(key, data)
  }
}

/** Rewrite every cached row with `edit`; `edit` returns the row unchanged to keep it. */
function mapRows(qc: QueryClient, snaps: Snapshot[], edit: (m: Message) => Message) {
  for (const [key, data] of snaps) {
    if (!data) continue
    qc.setQueryData<InfiniteList>(key, {
      ...data,
      pages: data.pages.map((p) => ({ ...p, messages: p.messages.map(edit) })),
    })
  }
}

/**
 * The matching rows of each unified view, with every view counted once.
 *
 * The same view is cached under several keys (unread filter on/off, account
 * scope), each holding the same rows. Adjusting a badge per cached key would
 * move it twice for one action and make it jump until the refetch corrects it.
 */
function rowsPerView(snaps: Snapshot[], match: (m: Message) => boolean): Map<UnifiedView, Message[]> {
  const perView = new Map<UnifiedView, Message[]>()
  for (const [key, data] of snaps) {
    const view = unifiedViewOfKey(key)
    if (!view || !data || perView.has(view)) continue
    const rows = data.pages.flatMap((p) => p.messages).filter(match)
    if (rows.length) perView.set(view, rows)
  }
  return perView
}

/** Unread messages a list row stands for (threads carry their own counter). */
function unreadOfRow(m: Message): number {
  return m.thread_unread ?? (m.is_read ? 0 : 1)
}

// Flip one cached row's read state, keeping the thread's unread counter — the
// value list rows actually render — consistent with it.
function markMessageRead(m: Message, isRead: boolean): Message {
  if (m.is_read === isRead) return m
  const threadUnread =
    m.thread_unread == null
      ? m.thread_unread
      : isRead
        ? Math.max(0, m.thread_unread - 1)
        : m.thread_unread + 1
  return { ...m, is_read: isRead, thread_unread: threadUnread }
}

/**
 * Optimistically flip a message's read state in every cached list and adjust
 * the affected unread badges.
 * @param qc - The query client holding the mail caches.
 * @param id - Message id to flip.
 * @param isRead - Target read state.
 * @returns A restore() that undoes the edit.
 */
export async function optimisticMarkRead(qc: QueryClient, id: string, isRead: boolean): Promise<() => void> {
  await cancelMailLists(qc)
  const prevCounts = qc.getQueryData<UnifiedCounts>(['unified-counts'])
  const snaps = listSnapshots(qc)
  if (prevCounts) {
    const next = { ...prevCounts }
    for (const [view, rows] of rowsPerView(snaps, (m) => m.id === id)) {
      if (!UNREAD_VIEWS.includes(view)) continue
      const m = rows[0]
      if (!m || m.is_read === isRead) continue
      next[view] = Math.max(0, next[view] + (isRead ? -1 : 1))
    }
    qc.setQueryData(['unified-counts'], next)
  }
  mapRows(qc, snaps, (m) => (m.id === id ? markMessageRead(m, isRead) : m))
  return restoreSnapshots(qc, snaps, prevCounts)
}

/**
 * Optimistically mark every message of a thread read or unread.
 * @param qc - The query client holding the mail caches.
 * @param threadId - Thread whose rows to flip.
 * @param isRead - Target read state for the whole thread.
 * @returns A restore() that undoes the edit.
 */
export async function optimisticMarkThreadRead(
  qc: QueryClient,
  threadId: string,
  isRead: boolean,
): Promise<() => void> {
  await cancelMailLists(qc)
  const prevCounts = qc.getQueryData<UnifiedCounts>(['unified-counts'])
  const snaps = listSnapshots(qc)
  const match = (m: Message) => m.thread_id === threadId
  // Reading a thread clears its unread counter; marking it unread sets every
  // message unread, so the counter becomes the thread size.
  const unreadAfter = (m: Message) => (isRead ? 0 : (m.thread_size ?? 1))
  if (prevCounts) {
    const next = { ...prevCounts }
    for (const [view, rows] of rowsPerView(snaps, match)) {
      if (!UNREAD_VIEWS.includes(view)) continue
      const delta = rows.reduce((sum, m) => sum + unreadAfter(m) - unreadOfRow(m), 0)
      next[view] = Math.max(0, next[view] + delta)
    }
    qc.setQueryData(['unified-counts'], next)
  }
  mapRows(qc, snaps, (m) =>
    match(m) ? { ...m, is_read: isRead, thread_unread: unreadAfter(m) } : m,
  )
  return restoreSnapshots(qc, snaps, prevCounts)
}

/**
 * Optimistically set a message's flag in every cached list, adjust the starred
 * badge, and drop the row from the starred view when unflagging.
 * @param qc - The query client holding the mail caches.
 * @param id - Message id to flag or unflag.
 * @param isFlagged - Target flag state.
 * @returns A restore() that undoes the edit.
 */
export async function optimisticToggleFlag(qc: QueryClient, id: string, isFlagged: boolean): Promise<() => void> {
  await cancelMailLists(qc)
  const prevCounts = qc.getQueryData<UnifiedCounts>(['unified-counts'])
  const snaps = listSnapshots(qc)
  const row = snaps
    .flatMap(([, data]) => data?.pages.flatMap((p) => p.messages) ?? [])
    .find((m) => m.id === id)
  if (prevCounts && row && row.is_flagged !== isFlagged) {
    qc.setQueryData<UnifiedCounts>(['unified-counts'], {
      ...prevCounts,
      starred: Math.max(0, prevCounts.starred + (isFlagged ? 1 : -1)),
    })
  }
  for (const [key, data] of snaps) {
    if (!data) continue
    const dropFromList = !isFlagged && unifiedViewOfKey(key) === 'starred'
    qc.setQueryData<InfiniteList>(key, {
      ...data,
      pages: data.pages.map((p) => ({
        ...p,
        messages: dropFromList
          ? p.messages.filter((m) => m.id !== id)
          : p.messages.map((m) => (m.id === id ? { ...m, is_flagged: isFlagged } : m)),
      })),
    })
  }
  return restoreSnapshots(qc, snaps, prevCounts)
}

/**
 * Drop every row matching `match` from all cached mail lists right away, so
 * delete/archive/move feel instant, and lower the badges those rows counted
 * towards (their unread messages, and the starred badge for flagged rows).
 * @param qc - The query client holding the mail caches.
 * @param match - Selects the rows that leave their view.
 * @returns A restore() that puts the snapshots back if the mutation fails.
 */
export async function optimisticRemove(qc: QueryClient, match: (m: Message) => boolean): Promise<() => void> {
  await cancelMailLists(qc)
  const prevCounts = qc.getQueryData<UnifiedCounts>(['unified-counts'])
  const snaps = listSnapshots(qc)
  if (prevCounts) {
    const next = { ...prevCounts }
    for (const [view, rows] of rowsPerView(snaps, match)) {
      if (view === 'starred') {
        next.starred = Math.max(0, next.starred - rows.length)
      } else if (UNREAD_VIEWS.includes(view)) {
        const unread = rows.reduce((sum, m) => sum + unreadOfRow(m), 0)
        next[view] = Math.max(0, next[view] - unread)
      }
    }
    qc.setQueryData(['unified-counts'], next)
  }
  for (const [key, data] of snaps) {
    if (!data) continue
    qc.setQueryData<InfiniteList>(key, {
      ...data,
      pages: data.pages.map((page) => {
        // `total` counts messages, but each removed row is a thread; decrement
        // by the thread's message count so the header stays accurate.
        const removed = page.messages.filter(match).reduce((sum, m) => sum + (m.thread_size ?? 1), 0)
        return {
          ...page,
          messages: page.messages.filter((m) => !match(m)),
          total: page.total != null ? Math.max(0, page.total - removed) : page.total,
        }
      }),
    })
  }
  return restoreSnapshots(qc, snaps, prevCounts)
}
