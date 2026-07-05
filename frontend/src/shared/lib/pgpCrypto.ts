import { argon2id } from 'hash-wasm'

const SESSION_PREFIX = 'mailquill:pgp:unlocked:'

export interface GeneratedPgpKey {
  fingerprint: string
  uid: string
  publicKeyArmored: string
  privateKeyEncryptedBlob: string
}

export interface UnlockedPgpKey {
  fingerprint: string
  privateKeyArmored: string
}

type OpenPgpModule = typeof import('openpgp')

async function openpgp(): Promise<OpenPgpModule> {
  return import('openpgp')
}

export function hasInlinePgpMessage(value?: string | null): boolean {
  return Boolean(value?.includes('-----BEGIN PGP MESSAGE-----'))
}

export function hasInlinePgpSignature(value?: string | null): boolean {
  return Boolean(value?.includes('-----BEGIN PGP SIGNED MESSAGE-----'))
}

export function hasPgpMime(value?: string | null): boolean {
  const lower = value?.toLowerCase() ?? ''
  return lower.includes('multipart/encrypted') || lower.includes('multipart/signed')
}

export function sessionKeyId(fingerprint: string): string {
  return `${SESSION_PREFIX}${fingerprint.toUpperCase()}`
}

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

export function cacheUnlockedKey(key: UnlockedPgpKey): void {
  sessionStorage.setItem(sessionKeyId(key.fingerprint), JSON.stringify(key))
}

export function clearUnlockedKey(fingerprint: string): void {
  sessionStorage.removeItem(sessionKeyId(fingerprint))
}

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

export async function decryptInlinePgp(armoredMessage: string, privateKeyArmored: string): Promise<string> {
  const pgp = await openpgp()
  const message = await pgp.readMessage({ armoredMessage })
  const privateKey = await pgp.readPrivateKey({ armoredKey: privateKeyArmored })
  const { data } = await pgp.decrypt({ message, decryptionKeys: privateKey, format: 'utf8' })
  return String(data)
}

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

export async function signText(text: string, privateKeyArmored: string): Promise<string> {
  const pgp = await openpgp()
  const privateKey = await pgp.readPrivateKey({ armoredKey: privateKeyArmored })
  const message = await pgp.createCleartextMessage({ text })
  return pgp.sign({ message, signingKeys: privateKey, format: 'armored' }) as Promise<string>
}

export async function signDetachedText(text: string, privateKeyArmored: string): Promise<string> {
  const pgp = await openpgp()
  const privateKey = await pgp.readPrivateKey({ armoredKey: privateKeyArmored })
  const message = await pgp.createMessage({ text })
  return pgp.sign({ message, signingKeys: privateKey, detached: true, format: 'armored' }) as Promise<string>
}

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

export function extractInlinePgpMessage(value: string): string | null {
  return extractArmor(value, 'PGP MESSAGE')
}

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
