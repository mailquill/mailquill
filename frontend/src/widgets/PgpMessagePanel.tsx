import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { KeyRound, Lock, ShieldAlert, ShieldCheck } from 'lucide-react'
import { Button } from '@/shared/components/ui/button'
import { Badge } from '@/shared/components/ui/badge'
import { apiGet, apiGetBlob } from '@/shared/api'
import { parseFromAddr } from '@/shared/lib/format'
import { messagePlain, messageRawEml } from '@/shared/lib/messageSource'
import { useDiscoverKey, usePgpKeys } from '@/shared/hooks/usePgp'
import type { Message } from '@/shared/types'
import {
  decryptInlinePgp,
  extractInlinePgpMessage,
  getUnlockedKey,
  hasInlinePgpMessage,
  hasInlinePgpSignature,
  unlockPrivateKey,
  verifyInlinePgp,
  verifyDetachedPgp,
} from '@/shared/lib/pgpCrypto'

export function PgpMessagePanel({ message }: { message: Message }) {
  const { t } = useTranslation()
  const { data: keys = [] } = usePgpKeys()
  const discoverKey = useDiscoverKey()
  const [plaintext, setPlaintext] = useState<string | null>(null)
  const [rawVisible, setRawVisible] = useState(false)
  const [status, setStatus] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const source = useMemo(() => `${message.body_text ?? ''}\n${message.body_html ?? ''}\n${message.snippet ?? ''}`, [message])
  const encryptedArmor = useMemo(() => extractInlinePgpMessage(source), [source])
  const attachmentTypes = (message.attachments ?? []).map((attachment) => attachment.content_type.toLowerCase())
  const encryptedAttachment = (message.attachments ?? []).find((attachment) => {
    const type = attachment.content_type.toLowerCase()
    return type.includes('octet-stream') || attachment.filename?.toLowerCase().includes('encrypted.asc')
  })
  const signatureAttachment = (message.attachments ?? []).find((attachment) =>
    attachment.content_type.toLowerCase().includes('pgp-signature'),
  )
  const signed = hasInlinePgpSignature(source) || attachmentTypes.some((type) => type.includes('pgp-signature'))
  const encrypted =
    hasInlinePgpMessage(source) ||
    attachmentTypes.some((type) => type.includes('pgp-encrypted') || type.includes('octet-stream+pgp')) ||
    Boolean(encryptedAttachment)

  if (!encrypted && !signed) return null

  async function decrypt() {
    setBusy(true)
    setStatus(null)
    try {
      const payload = encryptedArmor ?? (encryptedAttachment ? await attachmentText(encryptedAttachment.id) : null)
      if (!payload) {
        setStatus(t('pgpMessage.noPgpPayload'))
        return
      }
      for (const key of keys) {
        const cached = getUnlockedKey(key.fingerprint) ?? (await promptUnlock(key.id, key.fingerprint))
        if (!cached) continue
        try {
          setPlaintext(await decryptInlinePgp(payload, cached.privateKeyArmored))
          setStatus(t('pgpMessage.decrypted'))
          return
        } catch {
          // Try the next configured key.
        }
      }
      setStatus(t('pgpMessage.noMatchingKey'))
    } catch {
      setStatus(t('pgpMessage.decryptFailed'))
    } finally {
      setBusy(false)
    }
  }

  async function verify() {
    setBusy(true)
    setStatus(null)
    try {
      const sender = parseFromAddr(message.from_addr).email
      const result = await discoverKey.mutateAsync(sender)
      if (!result.key) {
        setStatus(t('pgpMessage.unverified'))
        return
      }
      const verification = signatureAttachment
        ? await verifyDetachedPgp(messagePlain(message), await attachmentText(signatureAttachment.id), result.key.public_key_data)
        : await verifyInlinePgp(source, result.key.public_key_data)
      setStatus(verification.verified ? t('pgpMessage.verified') : t('pgpMessage.invalid'))
    } catch {
      setStatus(t('pgpMessage.invalid'))
    } finally {
      setBusy(false)
    }
  }

  async function attachmentText(id: string): Promise<string> {
    return (await apiGetBlob(`/attachments/${id}`)).text()
  }

  async function promptUnlock(id: string, fingerprint: string) {
    const passphrase = window.prompt(t('pgp.unlockPrompt'))
    if (!passphrase) return null
    const blob = await apiGet<{ private_key_encrypted_blob: string }>(`/pgp-keys/${id}/blob`)
    return unlockPrivateKey(fingerprint, blob.private_key_encrypted_blob, passphrase)
  }

  return (
    <div className="mb-3 rounded-md border border-border bg-secondary/40 px-3 py-2 text-[12.5px]">
      <div className="flex flex-wrap items-center gap-2">
        {encrypted ? <Lock className="size-4" aria-hidden="true" /> : <ShieldCheck className="size-4" aria-hidden="true" />}
        <span className="font-semibold">{encrypted ? t('pgpMessage.encrypted') : t('pgpMessage.signed')}</span>
        {status && <Badge variant={status === t('pgpMessage.invalid') ? 'destructive' : 'secondary'}>{status}</Badge>}
        <div className="ml-auto flex gap-1.5">
          {encrypted && (
            <Button type="button" size="sm" variant="outline" onClick={decrypt} disabled={busy}>
              <KeyRound className="size-3.5" />
              {t('pgpMessage.decrypt')}
            </Button>
          )}
          {signed && (
            <Button type="button" size="sm" variant="outline" onClick={verify} disabled={busy}>
              <ShieldAlert className="size-3.5" />
              {t('pgpMessage.verify')}
            </Button>
          )}
        </div>
      </div>
      {plaintext && (
        <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background p-3 font-mono text-xs">
          {plaintext}
        </pre>
      )}
      {status === t('pgpMessage.decryptFailed') && (
        <button type="button" className="mt-2 text-xs font-semibold underline" onClick={() => setRawVisible((v) => !v)}>
          {rawVisible ? t('pgpMessage.hideRaw') : t('pgpMessage.showRaw')}
        </button>
      )}
      {rawVisible && (
        <pre className="mt-2 max-h-60 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background p-3 font-mono text-xs">
          {messagePlain(message) || messageRawEml(message)}
        </pre>
      )}
    </div>
  )
}
