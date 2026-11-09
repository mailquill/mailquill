import { create } from 'zustand'

interface ModuleNavState {
  contactGroup: string // 'all' | 'fav' | source id | `group:${group id}`
  hiddenCalendars: string[]
  setContactGroup: (group: string) => void
  toggleCalendar: (id: string) => void
}

/**
 * In-memory navigation state shared between the sidebar and the contacts and
 * calendar modules: which contact group is selected and which calendars are
 * hidden. Not persisted; it resets on reload.
 * @returns The store hook exposing `contactGroup`, `hiddenCalendars` and their setters.
 */
export const useModuleNav = create<ModuleNavState>((set) => ({
  contactGroup: 'all',
  hiddenCalendars: [],
  setContactGroup: (contactGroup) => set({ contactGroup }),
  toggleCalendar: (id) =>
    set((state) => ({
      hiddenCalendars: state.hiddenCalendars.includes(id)
        ? state.hiddenCalendars.filter((c) => c !== id)
        : [...state.hiddenCalendars, id],
    })),
}))
