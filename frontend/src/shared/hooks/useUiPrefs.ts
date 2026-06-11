import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export type Density = 'compact' | 'comfortable' | 'roomy'
export type AccountMarker = 'stripe' | 'dot' | 'both'

export const SIDEBAR_WIDTH = { default: 345, min: 200, max: 420 }
export const LIST_WIDTH = { default: 685, min: 300, max: 760 }

interface UiPrefsState {
  density: Density
  marker: AccountMarker
  maxRecipients: number
  sidebarWidth: number
  listWidth: number
  setDensity: (density: Density) => void
  setMarker: (marker: AccountMarker) => void
  setMaxRecipients: (n: number) => void
  setSidebarWidth: (w: number) => void
  setListWidth: (w: number) => void
}

const clamp = (value: number, { min, max }: { min: number; max: number }) =>
  Math.min(max, Math.max(min, Math.round(value)))

export const useUiPrefs = create<UiPrefsState>()(
  persist(
    (set) => ({
      density: 'comfortable',
      marker: 'both',
      maxRecipients: 25,
      sidebarWidth: SIDEBAR_WIDTH.default,
      listWidth: LIST_WIDTH.default,
      setDensity: (density) => set({ density }),
      setMarker: (marker) => set({ marker }),
      setMaxRecipients: (maxRecipients) => set({ maxRecipients: Math.max(1, maxRecipients) }),
      setSidebarWidth: (w) => set({ sidebarWidth: clamp(w, SIDEBAR_WIDTH) }),
      setListWidth: (w) => set({ listWidth: clamp(w, LIST_WIDTH) }),
    }),
    { name: 'mailquill-ui' },
  ),
)
