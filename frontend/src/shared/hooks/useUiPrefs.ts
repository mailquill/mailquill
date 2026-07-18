import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export type Density = 'compact' | 'comfortable' | 'roomy'
export type CalendarGrouping = 'account' | 'flat'

export const SIDEBAR_WIDTH = { default: 345, min: 200, max: 420 }
export const LIST_WIDTH = { default: 685, min: 300, max: 760 }

interface UiPrefsState {
  density: Density
  maxRecipients: number
  sidebarWidth: number
  listWidth: number
  /** Mailboxes explicitly collapsed by the user. New mailboxes stay open. */
  collapsedMailboxIds: string[]
  /** Expanded folder paths keyed by mailbox id. Folder paths are stable across refetches. */
  expandedFoldersByMailbox: Record<string, string[]>
  /** Desktop notifications on. Drives the foreground SSE stream and reflects
   *  the settings toggle; web push is attempted separately on enable. */
  notificationsEnabled: boolean
  /** How the sidebar groups calendars: by their owning account, or one flat list. */
  calendarGrouping: CalendarGrouping
  setDensity: (density: Density) => void
  setMaxRecipients: (n: number) => void
  setSidebarWidth: (w: number) => void
  setListWidth: (w: number) => void
  toggleMailboxCollapsed: (accountId: string) => void
  toggleFolderExpanded: (accountId: string, folderPath: string) => void
  setNotificationsEnabled: (on: boolean) => void
  setCalendarGrouping: (g: CalendarGrouping) => void
}

const clamp = (value: number, { min, max }: { min: number; max: number }) =>
  Math.min(max, Math.max(min, Math.round(value)))

export const useUiPrefs = create<UiPrefsState>()(
  persist(
    (set) => ({
      density: 'comfortable',
      maxRecipients: 25,
      sidebarWidth: SIDEBAR_WIDTH.default,
      listWidth: LIST_WIDTH.default,
      collapsedMailboxIds: [],
      expandedFoldersByMailbox: {},
      notificationsEnabled: false,
      calendarGrouping: 'account',
      setDensity: (density) => set({ density }),
      setMaxRecipients: (maxRecipients) => set({ maxRecipients: Math.max(1, maxRecipients) }),
      setSidebarWidth: (w) => set({ sidebarWidth: clamp(w, SIDEBAR_WIDTH) }),
      setListWidth: (w) => set({ listWidth: clamp(w, LIST_WIDTH) }),
      toggleMailboxCollapsed: (accountId) =>
        set((state) => ({
          collapsedMailboxIds: state.collapsedMailboxIds.includes(accountId)
            ? state.collapsedMailboxIds.filter((id) => id !== accountId)
            : [...state.collapsedMailboxIds, accountId],
        })),
      toggleFolderExpanded: (accountId, folderPath) =>
        set((state) => {
          const expanded = state.expandedFoldersByMailbox[accountId] ?? []
          return {
            expandedFoldersByMailbox: {
              ...state.expandedFoldersByMailbox,
              [accountId]: expanded.includes(folderPath)
                ? expanded.filter((path) => path !== folderPath)
                : [...expanded, folderPath],
            },
          }
        }),
      setNotificationsEnabled: (notificationsEnabled) => set({ notificationsEnabled }),
      setCalendarGrouping: (calendarGrouping) => set({ calendarGrouping }),
    }),
    { name: 'mailquill-ui' },
  ),
)
