import { argon2id } from 'hash-wasm'

const SESSION_PREFIX = 'mailquill:pgp:unlocked:'

/**
 * Result of generating or importing a PGP key pair, in the shape the backend stores.
 * @property fingerprint - Upper-case hex fingerprint of the key.
 * @property uid - Primary user id (`Name <email>`).
 * @property publicKeyArmored - ASCII-armored public key.
 * @property privateKeyEncryptedBlob - Private key wrapped with a passphrase-derived key; never usable without the passphrase.
 */
export interface GeneratedPgpKey {
  fingerprint: string
  uid: string
  publicKeyArmored: string
  privateKeyEncryptedBlob: string
}

/**
 * A private key decrypted for the current browser session.
 * @property fingerprint - Upper-case hex fingerprint.
 * @property privateKeyArmored - ASCII-armored, decrypted private key.
 */
export interface UnlockedPgpKey {
  fingerprint: string
  privateKeyArmored: string
}

type OpenPgpModule = typeof import('openpgp')

async function openpgp(): Promise<OpenPgpModule> {
  return import('openpgp')
}

/**
 * Whether a body contains an inline (PGP/INLINE) encrypted message block.
 * @param value - Message body text.
 * @returns `true` when a `BEGIN PGP MESSAGE` marker is present.
 */
export function hasInlinePgpMessage(value?: string | null): boolean {
  return Boolean(value?.includes('-----BEGIN PGP MESSAGE-----'))
}

/**
 * Whether a body contains an inline clear-signed message.
 * @param value - Message body text.
 * @returns `true` when a `BEGIN PGP SIGNED MESSAGE` marker is present.
 */
export function hasInlinePgpSignature(value?: string | null): boolean {
  return Boolean(value?.includes('-----BEGIN PGP SIGNED MESSAGE-----'))
}

/**
 * Whether a Content-Type (or raw header text) indicates PGP/MIME encryption or signing.
 * @param value - Content-Type header value or raw source.
 * @returns `true` for `multipart/encrypted` or `multipart/signed`.
 */
export function hasPgpMime(value?: string | null): boolean {
  const lower = value?.toLowerCase() ?? ''
  return lower.includes('multipart/encrypted') || lower.includes('multipart/signed')
}

/**
 * `sessionStorage` key under which an unlocked private key is cached.
 * @param fingerprint - Key fingerprint, normalised to upper case.
 * @returns The namespaced storage key.
 */
export function sessionKeyId(fingerprint: string): string {
  return `${SESSION_PREFIX}${fingerprint.toUpperCase()}`
}

/**
 * Read an unlocked private key from the session cache. A corrupt entry is removed and treated as absent.
 * @param fingerprint - Fingerprint of the key.
 * @returns The cached key, or `null` when the key has not been unlocked this session.
 */
export function getUnlockedKey(fingerprint: string): UnlockedPgpKey | null {
  const raw = sessionStorage.getItem(sessionKeyId(fingerprint))
  if (!raw) return null
  try {
    return JSON.parse(raw) as UnlockedPgpKey
  } catch {
    sessionStorage.removeItem(sessionKeyId(fingerprint))
    return null
  }
}

/**
 * Store an unlocked private key in `sessionStorage` so later decrypt/sign calls do not prompt for the passphrase again.
 * @param key - The unlocked key.
 * @returns Nothing.
 */
export function cacheUnlockedKey(key: UnlockedPgpKey): void {
  sessionStorage.setItem(sessionKeyId(key.fingerprint), JSON.stringify(key))
}

/**
 * Forget an unlocked private key (lock it again).
 * @param fingerprint - Fingerprint of the key to remove from the session cache.
 * @returns Nothing.
 */
export function clearUnlockedKey(fingerprint: string): void {
  sessionStorage.removeItem(sessionKeyId(fingerprint))
}

/**
 * Generate a new ECC key pair for an identity and wrap the private key for storage.
 * @param name - Display name for the user id.
 * @param email - Address for the user id.
 * @param passphrase - Protects the private key; must satisfy `assertStrongPassphrase`.
 * @returns The fingerprint, user id, armored public key and wrapped private key.
 * @throws Error `pgp.passphraseWeak` when the passphrase is too short.
 */
export async function generatePgpKey(name: string, email: string, passphrase: string): Promise<GeneratedPgpKey> {
  assertStrongPassphrase(passphrase)
  const pgp = await openpgp()
  const { privateKey, publicKey } = await pgp.generateKey({
    type: 'ecc',
    userIDs: [{ name, email }],
    passphrase,
    format: 'armored',
  })
  const publicKeyObj = await pgp.readKey({ armoredKey: publicKey })
  const fingerprint = publicKeyObj.getFingerprint().toUpperCase()
  const uid = publicKeyObj.getUserIDs()[0] ?? `${name} <${email}>`
  const privateKeyEncryptedBlob = await wrapPrivateKey(privateKey, passphrase, fingerprint)
  return { fingerprint, uid, publicKeyArmored: publicKey, privateKeyEncryptedBlob }
}

/**
 * Import an existing armored private key, verify the passphrase opens it, wrap it for storage and cache the unlocked key for this session.
 * @param armoredPrivateKey - ASCII-armored private key.
 * @param passphrase - Passphrase of the key; must satisfy `assertStrongPassphrase`.
 * @returns The key in storable form.
 * @throws Error when the passphrase is weak or does not decrypt the key.
 */
export async function importPgpPrivateKey(armoredPrivateKey: string, passphrase: string): Promise<GeneratedPgpKey> {
  assertStrongPassphrase(passphrase)
  const pgp = await openpgp()
  const privateKey = await pgp.readPrivateKey({ armoredKey: armoredPrivateKey })
  const decrypted = await pgp.decryptKey({ privateKey, passphrase })
  const publicKey = decrypted.toPublic().armor()
  const publicKeyObj = await pgp.readKey({ armoredKey: publicKey })
  const fingerprint = publicKeyObj.getFingerprint().toUpperCase()
  const uid = publicKeyObj.getUserIDs()[0] ?? fingerprint
  const privateKeyEncryptedBlob = await wrapPrivateKey(armoredPrivateKey, passphrase, fingerprint)
  cacheUnlockedKey({ fingerprint, privateKeyArmored: decrypted.armor() })
  return { fingerprint, uid, publicKeyArmored: publicKey, privateKeyEncryptedBlob }
}

/**
 * Unwrap a stored private key with its passphrase and cache it for the session.
 * @param fingerprint - Fingerprint of the key (salts the wrapping key).
 * @param privateKeyEncryptedBlob - Wrapped private key as stored by the backend.
 * @param passphrase - User's passphrase.
 * @returns The unlocked key.
 * @throws Error when the passphrase is wrong.
 */
export async function unlockPrivateKey(
  fingerprint: string,
  privateKeyEncryptedBlob: string,
  passphrase: string,
): Promise<UnlockedPgpKey> {
  const pgp = await openpgp()
  const wrapPassphrase = await derivedPassphrase(passphrase, fingerprint)
  const message = await pgp.readMessage({ armoredMessage: privateKeyEncryptedBlob })
  const decryptedBlob = await pgp.decrypt({ message, passwords: [wrapPassphrase], format: 'utf8' })
  const privateKey = await pgp.readPrivateKey({ armoredKey: String(decryptedBlob.data) })
  const unlocked = await pgp.decryptKey({ privateKey, passphrase })
  const key = { fingerprint: fingerprint.toUpperCase(), privateKeyArmored: unlocked.armor() }
  cacheUnlockedKey(key)
  return key
}

/**
 * Decrypt an armored PGP message.
 * @param armoredMessage - The `BEGIN PGP MESSAGE` block.
 * @param privateKeyArmored - Unlocked private key able to decrypt it.
 * @returns The decrypted text.
 */
export async function decryptInlinePgp(armoredMessage: string, privateKeyArmored: string): Promise<string> {
  const pgp = await openpgp()
  const message = await pgp.readMessage({ armoredMessage })
  const privateKey = await pgp.readPrivateKey({ armoredKey: privateKeyArmored })
  const { data } = await pgp.decrypt({ message, decryptionKeys: privateKey, format: 'utf8' })
  return String(data)
}

/**
 * Verify a clear-signed (inline) message against a sender's public key.
 * @param cleartextMessage - The `BEGIN PGP SIGNED MESSAGE` block.
 * @param publicKeyArmored - Armored public key of the expected signer.
 * @returns `verified: true` with the signing key id when the signature checks out, `verified: false` when the message carries no signature.
 * @throws Error when a signature is present but invalid.
 */
export async function verifyInlinePgp(
  cleartextMessage: string,
  publicKeyArmored: string,
): Promise<{ verified: boolean; fingerprint?: string }> {
  const pgp = await openpgp()
  const message = await pgp.readCleartextMessage({ cleartextMessage })
  const verificationKeys = await pgp.readKey({ armoredKey: publicKeyArmored })
  const { signatures } = await pgp.verify({ message, verificationKeys })
  const signature = signatures[0]
  if (!signature) return { verified: false }
  await signature.verified
  return { verified: true, fingerprint: signature.keyID.toHex().toUpperCase() }
}

/**
 * Verify a detached signature (PGP/MIME `multipart/signed`) over a text part.
 * @param text - The signed text exactly as transmitted.
 * @param armoredSignature - The armored detached signature.
 * @param publicKeyArmored - Armored public key of the expected signer.
 * @returns `verified: true` with the signing key id when the signature checks out, `verified: false` when no signature is present.
 * @throws Error when a signature is present but invalid.
 */
export async function verifyDetachedPgp(
  text: string,
  armoredSignature: string,
  publicKeyArmored: string,
): Promise<{ verified: boolean; fingerprint?: string }> {
  const pgp = await openpgp()
  const message = await pgp.createMessage({ text })
  const signature = await pgp.readSignature({ armoredSignature })
  const verificationKeys = await pgp.readKey({ armoredKey: publicKeyArmored })
  const { signatures } = await pgp.verify({ message, signature, verificationKeys })
  const result = signatures[0]
  if (!result) return { verified: false }
  await result.verified
  return { verified: true, fingerprint: result.keyID.toHex().toUpperCase() }
}

/**
 * Clear-sign text (inline signature).
 * @param text - Text to sign.
 * @param privateKeyArmored - Unlocked private signing key.
 * @returns The armored clear-signed message.
 */
export async function signText(text: string, privateKeyArmored: string): Promise<string> {
  const pgp = await openpgp()
  const privateKey = await pgp.readPrivateKey({ armoredKey: privateKeyArmored })
  const message = await pgp.createCleartextMessage({ text })
  return pgp.sign({ message, signingKeys: privateKey, format: 'armored' }) as Promise<string>
}

/**
 * Create a detached signature over text, for PGP/MIME signed mail.
 * @param text - Text to sign.
 * @param privateKeyArmored - Unlocked private signing key.
 * @returns The armored detached signature.
 */
export async function signDetachedText(text: string, privateKeyArmored: string): Promise<string> {
  const pgp = await openpgp()
  const privateKey = await pgp.readPrivateKey({ armoredKey: privateKeyArmored })
  const message = await pgp.createMessage({ text })
  return pgp.sign({ message, signingKeys: privateKey, detached: true, format: 'armored' }) as Promise<string>
}

/**
 * Encrypt text for one or more recipients, optionally signing it.
 * @param text - Plain text to encrypt.
 * @param publicKeysArmored - Armored public keys of all recipients.
 * @param privateKeyArmored - Unlocked private key to sign with; omit for an unsigned message.
 * @returns The armored encrypted message.
 */
export async function encryptText(
  text: string,
  publicKeysArmored: string[],
  privateKeyArmored?: string,
): Promise<string> {
  const pgp = await openpgp()
  const message = await pgp.createMessage({ text })
  const encryptionKeys = await Promise.all(publicKeysArmored.map((armoredKey) => pgp.readKey({ armoredKey })))
  const signingKeys = privateKeyArmored ? await pgp.readPrivateKey({ armoredKey: privateKeyArmored }) : undefined
  return pgp.encrypt({ message, encryptionKeys, signingKeys, format: 'armored' }) as Promise<string>
}

/**
 * Cut the armored PGP message block out of surrounding body text.
 * @param value - Body text that may contain a `BEGIN PGP MESSAGE` block.
 * @returns The block including its begin/end markers, or `null` when none is found.
 */
export function extractInlinePgpMessage(value: string): string | null {
  return extractArmor(value, 'PGP MESSAGE')
}

/**
 * Enforce the minimum passphrase length (12 characters) for key generation and import.
 * @param passphrase - Candidate passphrase.
 * @returns Nothing.
 * @throws Error with message `pgp.passphraseWeak` (an i18n key) when too short.
 */
export function assertStrongPassphrase(passphrase: string): void {
  if (passphrase.length < 12) {
    throw new Error('pgp.passphraseWeak')
  }
}

async function wrapPrivateKey(privateKeyArmored: string, passphrase: string, fingerprint: string): Promise<string> {
  const pgp = await openpgp()
  const wrapPassphrase = await derivedPassphrase(passphrase, fingerprint)
  const message = await pgp.createMessage({ text: privateKeyArmored })
  return pgp.encrypt({ message, passwords: [wrapPassphrase], format: 'armored' }) as Promise<string>
}

async function derivedPassphrase(passphrase: string, fingerprint: string): Promise<string> {
  const salt = await sha256Hex(`mailquill-pgp:${fingerprint.toUpperCase()}`)
  return argon2id({
    password: passphrase,
    salt,
    parallelism: 1,
    iterations: 3,
    memorySize: 64 * 1024,
    hashLength: 32,
    outputType: 'hex',
  })
}

async function sha256Hex(value: string): Promise<string> {
  const bytes = new TextEncoder().encode(value)
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('')
}

function extractArmor(value: string, label: string): string | null {
  const begin = `-----BEGIN ${label}-----`
  const end = `-----END ${label}-----`
  const start = value.indexOf(begin)
  const finish = value.indexOf(end)
  if (start < 0 || finish < start) return null
  return value.slice(start, finish + end.length)
}
