import { create } from 'zustand'
import { persist } from 'zustand/middleware'

/** The user's colour scheme choice; `system` follows the OS setting. */
export type ThemePref = 'system' | 'light' | 'dark'

const mql = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null

/**
 * Resolve a preference to an actual light/dark and toggle the `dark` class on
 * the document element.
 * @param pref - The preference to apply; `system` reads `prefers-color-scheme`.
 */
function applyTheme(pref: ThemePref) {
  const dark = pref === 'dark' || (pref === 'system' && !!mql?.matches)
  document.documentElement.classList.toggle('dark', dark)
}

interface ThemeState {
  pref: ThemePref
  setPref: (pref: ThemePref) => void
}

/**
 * The theme preference, persisted in localStorage under `mailquill-theme`.
 * Setting it applies the theme immediately; rehydration re-applies it and the
 * module keeps `system` in step with OS appearance changes.
 * @returns The store hook exposing `pref` and `setPref`.
 */
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
