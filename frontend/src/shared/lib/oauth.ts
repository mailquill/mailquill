import { useAuthStore } from '@/app/store'

/**
 * Begin an OAuth connect flow by navigating the browser to the provider-start
 * endpoint. A full-page redirect can't send an Authorization header, so the
 * short-lived access token is passed as a query param and validated server-side.
 */
export function startOAuthRedirect(
  provider: 'google' | 'microsoft',
  accountId?: string,
  destination?: 'calendar',
) {
  const token = useAuthStore.getState().accessToken
  if (!token) return
  const params = new URLSearchParams({ token })
  if (accountId) params.set('account_id', accountId)
  if (destination === 'calendar') params.set('calendar', 'true')
  window.location.assign(`/api/auth/oauth/${provider}/start?${params}`)
}
