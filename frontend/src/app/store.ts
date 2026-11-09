import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { setAccessToken, setRefreshSubscriber } from '@/shared/api'

interface AuthState {
  accessToken: string | null
  userId: string | null
  email: string | null
  setAuth: (token: string, userId: string, email: string) => void
  clearAuth: () => void
}

/**
 * Session store persisted to `localStorage` under `mailquill-auth`: the
 * short-lived access token plus the signed-in user's id and e-mail. `setAuth`
 * and `clearAuth` keep the API client's in-memory token in sync, and token
 * refreshes from the API client are written back through the subscriber below.
 */
export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      accessToken: null,
      userId: null,
      email: null,
      setAuth: (token, userId, email) => {
        setAccessToken(token)
        set({ accessToken: token, userId, email })
      },
      clearAuth: () => {
        setAccessToken(null)
        set({ accessToken: null, userId: null, email: null })
      },
    }),
    {
      name: 'mailquill-auth',
      partialize: (s) => ({ accessToken: s.accessToken, userId: s.userId, email: s.email }),
    },
  ),
)

// Rehydrate access token on module load
const stored = useAuthStore.getState()
if (stored.accessToken) {
  setAccessToken(stored.accessToken)
}

setRefreshSubscriber((token) => {
  useAuthStore.setState((state) => ({
    accessToken: token,
    userId: token ? state.userId : null,
    email: token ? state.email : null,
  }))
})
