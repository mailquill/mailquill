import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiDelete, apiGet, apiPost, apiPut } from '@/shared/api'

/**
 * One of the user's own OpenPGP key pairs as listed by the server. The private
 * half is never included in the listing.
 * @property fingerprint - Full key fingerprint, used to match signatures and recipients.
 * @property uid - The user id packet (name and address) the key was generated for.
 * @property public_key_armored - ASCII-armored public key for export and sharing.
 * @property is_primary - Whether this key signs and decrypts by default.
 */
export interface PgpKey {
  id: string
  fingerprint: string
  uid: string
  public_key_armored: string
  is_primary: boolean
  created_at: string
}

/**
 * A correspondent's public key known to the server.
 * @property public_key_data - ASCII-armored public key.
 * @property source - Where the key came from: imported by hand, WKD or a keyserver.
 * @property fetched_at - When the key was last obtained from its source.
 */
export interface ContactKey {
  id: string
  email: string
  public_key_data: string
  source: string
  fingerprint: string
  fetched_at: string
}

/**
 * Outcome of looking up a correspondent's key.
 * @property found - Whether any key was located.
 * @property key - The located key, or null.
 * @property local - True when the key was already stored, so no network lookup ran.
 * @property wkd_enabled - Whether Web Key Directory lookups are allowed by the user's settings.
 * @property keyserver_enabled - Whether keyserver lookups are allowed by the user's settings.
 */
export interface DiscoveryResponse {
  found: boolean
  key: ContactKey | null
  local: boolean
  wkd_enabled: boolean
  keyserver_enabled: boolean
}

/**
 * Payload for storing a key pair generated in the browser.
 * @property private_key_encrypted_blob - Private key already encrypted client-side.
 * @property is_primary - Make this the default key; the first key becomes primary regardless.
 */
export interface CreatePgpKeyInput {
  fingerprint: string
  uid: string
  public_key_armored: string
  private_key_encrypted_blob: string
  is_primary?: boolean
}

/**
 * The user's own key pairs from `GET /pgp-keys`, cached under `['pgp-keys']`.
 * @returns A query for the `PgpKey[]`.
 */
export function usePgpKeys() {
  return useQuery({
    queryKey: ['pgp-keys'],
    queryFn: () => apiGet<PgpKey[]>('/pgp-keys'),
  })
}

/**
 * Store a key pair generated or imported in the browser through
 * `POST /pgp-keys`, then refetch `['pgp-keys']`.
 * @returns A mutation taking a `CreatePgpKeyInput` and resolving to the stored `PgpKey`.
 */
export function useCreatePgpKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: CreatePgpKeyInput) => apiPost<PgpKey>('/pgp-keys', data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pgp-keys'] }),
  })
}

/**
 * Delete one of the user's key pairs through `DELETE /pgp-keys/:id`, then
 * refetch `['pgp-keys']`.
 * @returns A mutation taking the key id.
 */
export function useDeletePgpKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiDelete(`/pgp-keys/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pgp-keys'] }),
  })
}

/**
 * Make a key the default for signing and decryption through
 * `PUT /pgp-keys/:id/primary`, then refetch `['pgp-keys']`.
 * @returns A mutation taking the key id and resolving to the updated `PgpKey`.
 */
export function useSetPrimaryPgpKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => apiPut<PgpKey>(`/pgp-keys/${id}/primary`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pgp-keys'] }),
  })
}

/**
 * Import a correspondent's public key by hand through `POST /contact-keys`,
 * then refetch that address's `['contact-keys', email]` entry and every
 * cached discovery result.
 * @returns A mutation taking `{ email, public_key_data, fingerprint? }` and
 *   resolving to the stored `ContactKey`.
 */
export function useCreateContactKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: { email: string; public_key_data: string; fingerprint?: string }) =>
      apiPost<ContactKey>('/contact-keys', data),
    onSuccess: (_data, variables) => {
      qc.invalidateQueries({ queryKey: ['contact-keys', variables.email.toLowerCase()] })
      qc.invalidateQueries({ queryKey: ['key-discovery'] })
    },
  })
}

/**
 * Look a correspondent's key up through `GET /keys/discover?email=` (local
 * store, then WKD and keyservers as allowed). The result is written to
 * `['key-discovery', email]` so the compose UI can read it, and a found key
 * refetches that address's `['contact-keys', email]` entry.
 * @returns A mutation taking the address and resolving to the `DiscoveryResponse`.
 */
export function useDiscoverKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (email: string) =>
      apiGet<DiscoveryResponse>(`/keys/discover?email=${encodeURIComponent(email.trim().toLowerCase())}`),
    onSuccess: (data, email) => {
      qc.setQueryData(['key-discovery', email.trim().toLowerCase()], data)
      if (data.key) qc.invalidateQueries({ queryKey: ['contact-keys', email.trim().toLowerCase()] })
    },
  })
}
