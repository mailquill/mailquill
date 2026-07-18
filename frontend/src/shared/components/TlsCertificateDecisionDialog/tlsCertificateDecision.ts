import { ApiError } from '@/shared/api'

export type TlsDecision = 'accept' | 'accept_always' | 'deny'

export interface TlsCertificateInfo {
  host: string
  port: number
  fingerprint_sha256: string
  der_base64: string
}

/** Extract certificate details from the API's stable `tls_untrusted` response. */
export function tlsCertificateFromError(error: unknown): TlsCertificateInfo | null {
  if (!(error instanceof ApiError)) return null
  const body = error.json as { code?: string; cert?: TlsCertificateInfo } | null
  return body?.code === 'tls_untrusted' && body.cert?.der_base64 ? body.cert : null
}
