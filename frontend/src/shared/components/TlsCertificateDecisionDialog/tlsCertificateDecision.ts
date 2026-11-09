import { ApiError } from '@/shared/api'

/** User's answer to an untrusted-certificate prompt: trust it once for this request, pin it permanently for the account, or abort. */
export type TlsDecision = 'accept' | 'accept_always' | 'deny'

/**
 * Certificate presented by a mail server that failed trust validation, as reported by the API's `tls_untrusted` error.
 * @property host - Server host the certificate was presented by.
 * @property port - Server port.
 * @property fingerprint_sha256 - SHA-256 fingerprint shown to the user for comparison.
 * @property der_base64 - DER-encoded certificate, sent back when the user pins it.
 */
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
