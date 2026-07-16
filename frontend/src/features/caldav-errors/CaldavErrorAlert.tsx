import { AlertCircle, ShieldAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '@/shared/api'

/**
 * Present a curated, actionable explanation for CalDAV connection failures.
 *
 * @param props - Component properties.
 * @param props.error - Error returned by a CalDAV discovery or account connection request.
 * @returns An accessible alert, or `null` when no error is present.
 */
export function CaldavErrorAlert({ error }: { error: unknown }) {
  const { t } = useTranslation()
  if (!error) return null

  const code = error instanceof ApiError ? error.code : null
  const isCertificateError = code === 'caldav_tls_certificate_invalid'
  const [title, description] = errorCopy(code, t)
  const Icon = isCertificateError ? ShieldAlert : AlertCircle

  return (
    <div role="alert" className="flex gap-2 rounded-md border border-destructive/40 bg-destructive/5 p-3">
      <Icon className="mt-0.5 size-4 shrink-0 text-destructive" aria-hidden="true" />
      <div className="flex flex-col gap-1 text-[12px]">
        <p className="font-semibold text-destructive">{title}</p>
        <p className="text-foreground">{description}</p>
        {isCertificateError && (
          <p className="text-muted-foreground">{t('calendar.tlsCertificateErrorDecision')}</p>
        )}
      </div>
    </div>
  )
}

function errorCopy(code: string | null, t: (key: string) => string): [string, string] {
  switch (code) {
    case 'caldav_tls_certificate_invalid':
      return [t('calendar.tlsCertificateErrorTitle'), t('calendar.tlsCertificateErrorDescription')]
    case 'caldav_authentication_failed':
      return [t('calendar.authenticationErrorTitle'), t('calendar.authenticationErrorDescription')]
    case 'caldav_access_denied':
      return [t('calendar.accessDeniedErrorTitle'), t('calendar.accessDeniedErrorDescription')]
    case 'caldav_timeout':
      return [t('calendar.timeoutErrorTitle'), t('calendar.timeoutErrorDescription')]
    case 'caldav_connection_failed':
      return [t('calendar.connectionErrorTitle'), t('calendar.connectionErrorDescription')]
    case 'caldav_endpoint_not_found':
      return [t('calendar.endpointNotFoundErrorTitle'), t('calendar.endpointNotFoundErrorDescription')]
    case 'caldav_rate_limited':
      return [t('calendar.rateLimitedErrorTitle'), t('calendar.rateLimitedErrorDescription')]
    case 'caldav_server_error':
      return [t('calendar.serverErrorTitle'), t('calendar.serverErrorDescription')]
    case 'caldav_invalid_response':
      return [t('calendar.invalidResponseErrorTitle'), t('calendar.invalidResponseErrorDescription')]
    case 'caldav_no_calendars':
      return [t('calendar.noCalendarsErrorTitle'), t('calendar.noCalendarsErrorDescription')]
    default:
      return [t('calendar.discoveryErrorTitle'), t('calendar.discoverFailed')]
  }
}
