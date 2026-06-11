import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export type ThemePref = 'system' | 'light' | 'dark'

const mql = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null

/** Resolve a preference to an actual light/dark and toggle the `dark` class. */
export function applyTheme(pref: ThemePref) {
  const dark = pref === 'dark' || (pref === 'system' && !!mql?.matches)
  document.documentElement.classList.toggle('dark', dark)
}

interface ThemeState {
  pref: ThemePref
  setPref: (pref: ThemePref) => void
}

export const useThemeStore = create<ThemeState>()(
  persist(
    (set) => ({
      pref: 'system',
      setPref: (pref) => {
        applyTheme(pref)
        set({ pref })
      },
    }),
    {
      name: 'mailquill-theme',
      onRehydrateStorage: () => (state) => {
        applyTheme(state?.pref ?? 'system')
      },
    },
  ),
)

// Apply immediately on module load (before first paint where possible) and keep
// "system" in sync with OS appearance changes.
applyTheme(useThemeStore.getState().pref)
mql?.addEventListener('change', () => {
  if (useThemeStore.getState().pref === 'system') applyTheme('system')
})
