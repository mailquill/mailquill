import { useAuthStore } from '@/app/store'
import type { SyncStatus } from '@/shared/types'

/**
 * Begin an OAuth connect flow by navigating the browser to the provider-start
 * endpoint. A full-page redirect can't send an Authorization header, so the
 * short-lived access token is passed as a query param and validated server-side.
 */
export function startOAuthRedirect(
  provider: 'google' | 'microsoft',
  accountId?: string,
  destination?: 'calendar' | 'contacts',
) {
  const token = useAuthStore.getState().accessToken
  if (!token) return
  const params = new URLSearchParams({ token })
  if (accountId) params.set('account_id', accountId)
  if (destination === 'calendar') params.set('calendar', 'true')
  if (destination === 'contacts') params.set('contacts', 'true')
  window.location.assign(`/api/auth/oauth/${provider}/start?${params}`)
}

/**
 * Extract the OAuth provider from a mail sync status stuck in
 * `reauth_required`. The sync task encodes it as `oauth_reauthentication_required:<provider>`
 * in the status error (see mail-sync's `oauth_reauthentication_error`).
 */
export function oauthProviderFromStatus(status?: SyncStatus): 'google' | 'microsoft' | null {
  if (status?.state !== 'reauth_required') return null
  const provider = status.error?.split(':', 2)[1]
  return provider === 'google' || provider === 'microsoft' ? provider : null
}
