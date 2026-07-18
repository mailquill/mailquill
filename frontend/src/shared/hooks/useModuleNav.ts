import { create } from 'zustand'

interface ModuleNavState {
  contactGroup: string // 'all' | 'fav' | source id | `group:${group id}`
  hiddenCalendars: string[]
  setContactGroup: (group: string) => void
  toggleCalendar: (id: string) => void
}

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
