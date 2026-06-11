import { useAuthStore } from '@/app/store'

/**
 * Begin an OAuth connect flow by navigating the browser to the provider-start
 * endpoint. A full-page redirect can't send an Authorization header, so the
 * short-lived access token is passed as a query param and validated server-side.
 */
export function startOAuthRedirect(provider: 'google' | 'microsoft') {
  const token = useAuthStore.getState().accessToken
  if (!token) return
  window.location.assign(`/api/auth/oauth/${provider}/start?token=${encodeURIComponent(token)}`)
}
