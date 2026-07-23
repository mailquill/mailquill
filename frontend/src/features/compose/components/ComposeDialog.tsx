import { useEffect, useMemo, useRef, useState } from 'react'
import { useQueries } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { Bold, Italic, Underline, Strikethrough, List, ListOrdered, Lock, Paperclip, Send, ShieldCheck, TriangleAlert, Type, X } from 'lucide-react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { cn } from '@/shared/lib/utils'
import { apiGet, ApiError } from '@/shared/api'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useSaveDraft, useSendMessage } from '@/shared/hooks/useMessages'
import { useOnlineStatus } from '@/shared/hooks/useOnlineStatus'
import { usePgpKeys, type DiscoveryResponse } from '@/shared/hooks/usePgp'
import {
  encryptText,
  getUnlockedKey,
  hasInlinePgpMessage,
  hasPgpMime,
  signDetachedText,
  unlockPrivateKey,
} from '@/shared/lib/pgpCrypto'
import type { Account, AccountAlias, AttachmentInput, Message } from '@/shared/types'
import type { ComposeInitialState } from '../types'
import { isValidEmail } from '@/shared/lib/email'
import { messagePlain } from '@/shared/lib/messageSource'
import { RecipientChips } from './RecipientChips'

const MAX_ATTACHMENT_BYTES = 25 * 1024 * 1024

interface ComposeDialogProps {
  open: boolean
  initialState: ComposeInitialState
  onClose: () => void
  onSendQueued?: (send: { sendId: string; subject: string }) => void
}

interface AttachmentDraft extends AttachmentInput {
  size: number
}

interface SenderIdentity {
  accountId: string
  email: string
  label: string
  isPrimary: boolean
}

interface SenderGroup {
  accountName: string
  identities: SenderIdentity[]
}

type RichCommand = 'bold' | 'italic' | 'underline' | 'strikeThrough' | 'insertUnorderedList' | 'insertOrderedList'

const TOOLBAR: { command: RichCommand; tkey: string; icon: typeof Bold }[] = [
  { command: 'bold', tkey: 'compose.bold', icon: Bold },
  { command: 'italic', tkey: 'compose.italic', icon: Italic },
  { command: 'underline', tkey: 'compose.underline', icon: Underline },
  { command: 'strikeThrough', tkey: 'compose.strikethrough', icon: Strikethrough },
  { command: 'insertUnorderedList', tkey: 'compose.bulletedList', icon: List },
  { command: 'insertOrderedList', tkey: 'compose.numberedList', icon: ListOrdered },
]

export function ComposeDialog({ open, initialState, onClose, onSendQueued }: ComposeDialogProps) {
  const { t } = useTranslation()
  const editorRef = useRef<HTMLDivElement | null>(null)
  const isOnline = useOnlineStatus()
  const { data: accounts = [] } = useAccounts()
  const senderData = useSenderIdentityData(accounts)
  const identities = senderData.groups.flatMap((group) => group.identities)
  const sendMessage = useSendMessage()
  const saveDraft = useSaveDraft()
  const { data: pgpKeys = [] } = usePgpKeys()
  const sourceMessage = initialState.sourceMessage
  const sourceEncrypted =
    hasInlinePgpMessage(sourceMessage?.body_text) ||
    hasInlinePgpMessage(sourceMessage?.body_html) ||
    hasPgpMime(sourceMessage?.body_text) ||
    hasPgpMime(sourceMessage?.body_html)

  const initialTo =
    initialState.to ??
    (initialState.mode === 'draft' ? sourceMessage?.draft_to : undefined) ??
    (initialState.mode === 'reply' && sourceMessage ? parseAddressList(sourceMessage.from_addr) : [])
  const initialBody = buildBody(initialState.mode, sourceMessage, t)
  const draftIdRef = useRef(initialState.mode === 'draft' ? sourceMessage?.id : undefined)

  const [from, setFrom] = useState('')
  const [to, setTo] = useState<string[]>(initialTo)
  const [cc, setCc] = useState<string[]>(sourceMessage?.draft_cc ?? [])
  const [bcc, setBcc] = useState<string[]>(sourceMessage?.draft_bcc ?? [])
  const [showCc, setShowCc] = useState((sourceMessage?.draft_cc?.length ?? 0) > 0)
  const [showBcc, setShowBcc] = useState((sourceMessage?.draft_bcc?.length ?? 0) > 0)
  const [subject, setSubject] = useState(buildSubject(initialState.mode, sourceMessage))
  const [bodyHtml, setBodyHtml] = useState(initialBody)
  const [bodyText, setBodyText] = useState(sourceMessage?.body_text ?? '')
  const [plainText, setPlainText] = useState(initialState.mode === 'draft' && !sourceMessage?.body_html)
  const [attachments, setAttachments] = useState<AttachmentDraft[]>(() =>
    (sourceMessage?.draft_attachments ?? []).map((attachment) => ({
      ...attachment,
      size: Math.floor(attachment.data.length * 3 / 4),
    })),
  )
  const [attachmentError, setAttachmentError] = useState<string | null>(null)
  const [signOverride, setSignOverride] = useState<boolean | null>(null)
  const [encryptOverride, setEncryptOverride] = useState<boolean | null>(null)
  const [cryptoError, setCryptoError] = useState<string | null>(null)

  const defaultFrom = identities.length ? defaultFromIdentity(initialState, identities) : ''
  const effectiveFrom = from || defaultFrom
  // Accounts load asynchronously. Keep the empty state explicit so opening
  // compose during bootstrap cannot dereference a non-existent sender.
  const selectedIdentity: SenderIdentity | undefined =
    identities.find((identity) => identityKey(identity) === effectiveFrom) ?? identities[0]
  const selectedAccount = selectedIdentity
    ? accounts.find((account) => account.id === selectedIdentity.accountId)
    : undefined
  const primaryPgpKey =
    (selectedAccount?.pgp_key_id ? pgpKeys.find((key) => key.id === selectedAccount.pgp_key_id) : undefined) ??
    pgpKeys.find((key) => key.is_primary) ??
    pgpKeys[0]
  const sign = signOverride ?? Boolean(sourceEncrypted || selectedAccount?.sign_by_default)
  const encrypt = encryptOverride ?? sourceEncrypted
  const title =
    initialState.mode === 'reply'
      ? t('action.reply')
      : initialState.mode === 'forward'
        ? t('action.forward')
        : initialState.mode === 'draft'
          ? t('compose.editDraft')
          : t('compose.newMessage')

  const allRecipients = useMemo(() => [...to, ...cc, ...bcc], [bcc, cc, to])
  const recipientsValid =
    to.length > 0 && allRecipients.every((recipient) => isValidEmail(parseAddr(recipient)))
  const uniqueRecipients = useMemo(
    () =>
      Array.from(
        new Set(
          allRecipients
            .map((recipient) => parseAddr(recipient).trim().toLowerCase())
            .filter(isValidEmail),
        ),
      ),
    [allRecipients],
  )
  const recipientKeyQueries = useQueries({
    queries: uniqueRecipients.map((recipient) => ({
      queryKey: ['key-discovery', recipient],
      queryFn: () => apiGet<DiscoveryResponse>(`/keys/discover?email=${encodeURIComponent(recipient)}`),
      enabled: open && encrypt,
      staleTime: Number.POSITIVE_INFINITY,
    })),
  })
  const keyDiscoveryPending = encrypt && recipientKeyQueries.some((query) => query.isPending || query.isFetching)
  const missingRecipientKeys =
    encrypt && !keyDiscoveryPending
      ? uniqueRecipients.filter((_recipient, index) => !recipientKeyQueries[index]?.data?.key)
      : []
  useEffect(() => {
    // Seed the editor only when opening or switching back to rich mode — not on
    // every keystroke, which would reset the caret.
    if (editorRef.current && open && !plainText) {
      editorRef.current.innerHTML = bodyHtml
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, plainText])

  function applyFormat(command: RichCommand) {
    editorRef.current?.focus()
    document.execCommand(command)
    setBodyHtml(editorRef.current?.innerHTML ?? '')
  }

  async function handleFiles(files: FileList | null) {
    if (!files) return
    const nextFiles = await Promise.all(Array.from(files).map(fileToAttachment))
    const nextSize = [...attachments, ...nextFiles].reduce((total, attachment) => total + attachment.size, 0)
    if (nextSize > MAX_ATTACHMENT_BYTES) {
      setAttachmentError(t('compose.attachmentTooLarge'))
      return
    }
    setAttachments((current) => [...current, ...nextFiles])
    setAttachmentError(null)
  }

  async function handleSend() {
    if (!selectedIdentity) return
    setCryptoError(null)
    try {
      const html = plainText ? undefined : editorRef.current?.innerHTML ?? bodyHtml
      let text = plainText ? bodyText : htmlToText(html ?? '')
      let nextHtml = html
      let pgpMimeMode: 'signed' | 'encrypted' | undefined
      let pgpSignature: string | undefined
      let privateKeyArmored: string | undefined
      if (sign || encrypt) {
        privateKeyArmored = await ensureUnlockedPrimaryKey()
      }
      if (encrypt) {
        if (!primaryPgpKey) throw new Error('compose.noSigningKey')
        const recipientPublicKeys = recipientKeyQueries
          .map((query) => query.data?.key?.public_key_data)
          .filter((key): key is string => Boolean(key))
        if (keyDiscoveryPending || recipientPublicKeys.length !== uniqueRecipients.length) {
          throw new Error('compose.missingRecipientKeys')
        }
        text = await encryptText(text, [...recipientPublicKeys, primaryPgpKey.public_key_armored], privateKeyArmored)
        nextHtml = undefined
        pgpMimeMode = 'encrypted'
      } else if (sign && privateKeyArmored) {
        pgpSignature = await signDetachedText(text, privateKeyArmored)
        nextHtml = undefined
        pgpMimeMode = 'signed'
      }
      sendMessage.mutate(
        {
          draft_id: draftIdRef.current,
          account_id: selectedIdentity.accountId,
          from: selectedIdentity.email,
          to,
          cc,
          bcc,
          subject,
          body_html: nextHtml,
          body_text: text,
          pgp_mime_mode: pgpMimeMode,
          pgp_signature: pgpSignature,
          in_reply_to: initialState.mode === 'reply' ? sourceMessage?.message_id_header : null,
          references: initialState.mode === 'reply' || initialState.mode === 'forward' ? sourceMessage?.references : null,
          attachments: attachments.map(toAttachmentInput),
        },
        {
          onSuccess: ({ send_id: sendId }) => {
            onSendQueued?.({ sendId, subject })
            onClose()
          },
        },
      )
    } catch (err) {
      setCryptoError(t(err instanceof Error && err.message.startsWith('compose.') ? err.message : 'compose.cryptoFailed'))
    }
  }

  async function handleSaveAndClose() {
    const html = plainText ? undefined : editorRef.current?.innerHTML ?? bodyHtml
    const hasContent = Boolean(
      to.length || cc.length || bcc.length || subject.trim() || bodyText.trim() || htmlToText(html ?? '').trim() || attachments.length,
    )
    if (!hasContent) {
      onClose()
      return
    }
    if (!selectedIdentity) return

    try {
      const result = await saveDraft.mutateAsync({
        draft_id: draftIdRef.current,
        account_id: selectedIdentity.accountId,
        from: selectedIdentity.email,
        to,
        cc,
        bcc,
        subject,
        body_html: html,
        body_text: plainText ? bodyText : htmlToText(html ?? ''),
        in_reply_to: initialState.mode === 'reply' ? sourceMessage?.message_id_header : sourceMessage?.in_reply_to,
        references: sourceMessage?.references,
        attachments: attachments.map(toAttachmentInput),
      })
      draftIdRef.current = result.id
      onClose()
    } catch {
      // The mutation error is rendered below; keep the composer open.
    }
  }

  async function ensureUnlockedPrimaryKey(): Promise<string> {
    if (!primaryPgpKey) throw new Error('compose.noSigningKey')
    const cached = getUnlockedKey(primaryPgpKey.fingerprint)
    if (cached) return cached.privateKeyArmored
    const passphrase = window.prompt(t('pgp.unlockPrompt'))
    if (!passphrase) throw new Error('compose.unlockRequired')
    const blob = await apiGet<{ private_key_encrypted_blob: string }>(`/pgp-keys/${primaryPgpKey.id}/blob`)
    const unlocked = await unlockPrivateKey(primaryPgpKey.fingerprint, blob.private_key_encrypted_blob, passphrase)
    return unlocked.privateKeyArmored
  }

  return (
    <Dialog open={open} onClose={() => void handleSaveAndClose()}>
      <DialogContent className="flex h-[min(820px,90vh)] w-[min(860px,calc(100vw-2rem))] max-w-none flex-col overflow-hidden p-0">
        <DialogHeader className="mb-0 shrink-0 border-b border-border px-6 pb-3 pt-5">
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>

        <div className="shrink-0 space-y-3.5 px-6 py-4">
          <Field id="compose-from" label={t('compose.from')}>
            <Select
              id="compose-from"
              value={effectiveFrom}
              onChange={(event) => setFrom(event.currentTarget.value)}
            >
              {senderData.groups.map((group) => (
                <optgroup key={group.accountName} label={group.accountName}>
                  {group.identities.map((identity) => (
                    <option key={identityKey(identity)} value={identityKey(identity)}>
                      {identity.isPrimary ? `${identity.email} (primary)` : `- ${identity.label}`}
                    </option>
                  ))}
                </optgroup>
              ))}
            </Select>
          </Field>

          <RecipientChips
            label={t('compose.to')}
            value={to}
            onChange={setTo}
            mailboxId={selectedIdentity?.accountId}
            accessory={
              <div className="ml-auto flex gap-2 text-[12px] font-semibold text-[#2563eb]">
                {!showCc && (
                  <button type="button" onClick={() => setShowCc(true)} className="hover:underline">
                    {t('compose.cc')}
                  </button>
                )}
                {!showBcc && (
                  <button type="button" onClick={() => setShowBcc(true)} className="hover:underline">
                    {t('compose.bcc')}
                  </button>
                )}
              </div>
            }
          />

          {showCc && (
            <RecipientChips
              label={t('compose.cc')}
              value={cc}
              onChange={setCc}
              mailboxId={selectedIdentity?.accountId}
              autoFocus
              accessory={
                <button
                  type="button"
                  onClick={() => {
                    setShowCc(false)
                    setCc([])
                  }}
                  className="ml-auto text-muted-foreground hover:text-foreground"
                >
                  <X className="size-3.5" />
                </button>
              }
            />
          )}
          {showBcc && (
            <RecipientChips
              label={t('compose.bcc')}
              value={bcc}
              onChange={setBcc}
              mailboxId={selectedIdentity?.accountId}
              autoFocus
              accessory={
                <button
                  type="button"
                  onClick={() => {
                    setShowBcc(false)
                    setBcc([])
                  }}
                  className="ml-auto text-muted-foreground hover:text-foreground"
                >
                  <X className="size-3.5" />
                </button>
              }
            />
          )}

          <Field id="compose-subject" label={t('compose.subject')}>
            <Input id="compose-subject" value={subject} onChange={(event) => setSubject(event.currentTarget.value)} />
          </Field>

          {/* formatting toolbar */}
          <div className="flex items-center gap-1 border-b border-border pb-2">
            {!plainText &&
              TOOLBAR.map(({ command, tkey, icon: Icon }) => (
                <button
                  key={command}
                  type="button"
                  aria-label={t(tkey)}
                  title={t(tkey)}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => applyFormat(command)}
                  className="flex size-8 items-center justify-center rounded-md text-secondary-foreground transition-colors hover:bg-secondary"
                >
                  <Icon className="size-4" />
                </button>
              ))}
            <button
              type="button"
              onClick={() => setPlainText((p) => !p)}
              className={cn(
                'flex h-8 items-center gap-1.5 rounded-md px-2.5 text-[12px] font-semibold transition-colors',
                plainText ? 'bg-[var(--mq-row-open)] text-[#1d4ed8]' : 'text-secondary-foreground hover:bg-secondary',
              )}
            >
              <Type className="size-4" />
              {plainText ? t('compose.richText') : t('compose.plainText')}
            </button>
            <button
              type="button"
              aria-pressed={sign}
              onClick={() => setSignOverride(!sign)}
              className={cn(
                'flex h-8 items-center gap-1.5 rounded-md px-2.5 text-[12px] font-semibold transition-colors',
                sign ? 'bg-primary text-primary-foreground' : 'text-secondary-foreground hover:bg-secondary',
              )}
            >
              <ShieldCheck className="size-4" />
              {t('compose.sign')}
            </button>
            <button
              type="button"
              aria-pressed={encrypt}
              onClick={() => setEncryptOverride(!encrypt)}
              className={cn(
                'flex h-8 items-center gap-1.5 rounded-md px-2.5 text-[12px] font-semibold transition-colors',
                encrypt ? 'bg-primary text-primary-foreground' : 'text-secondary-foreground hover:bg-secondary',
              )}
            >
              <Lock className="size-4" />
              {t('compose.encrypt')}
            </button>
            <label className="ml-auto inline-flex h-8 cursor-pointer items-center gap-2 rounded-md border border-input px-3 text-[13px] transition-colors hover:bg-secondary">
              <Paperclip className="size-4" aria-hidden="true" />
              {t('compose.attach')}
              <input className="sr-only" type="file" multiple onChange={(event) => handleFiles(event.currentTarget.files)} />
            </label>
          </div>
        </div>

        <div className="min-h-0 flex-1 px-6 pb-4">
          {plainText ? (
            <textarea
              aria-label={t('compose.body')}
              value={bodyText}
              onChange={(e) => setBodyText(e.currentTarget.value)}
              className="h-full min-h-0 w-full resize-none overflow-y-auto rounded-md border border-input bg-background px-3 py-2 font-mono text-[13px] leading-6 outline-none focus-visible:ring-1 focus-visible:ring-ring"
            />
          ) : (
            <div
              ref={editorRef}
              contentEditable
              aria-label={t('compose.body')}
              className="h-full min-h-0 overflow-y-auto rounded-md border border-input bg-background px-3 py-2 text-sm leading-6 outline-none focus-visible:ring-1 focus-visible:ring-ring"
              onInput={(event) => setBodyHtml(event.currentTarget.innerHTML)}
            />
          )}
        </div>

        <div className="shrink-0 space-y-3 border-t border-border px-6 pb-5 pt-3">
          {attachments.length ? (
            <div className="flex flex-wrap gap-2">
              {attachments.map((attachment) => (
                <span
                  key={`${attachment.filename}-${attachment.size}`}
                  className="inline-flex items-center gap-2 rounded-md border border-border px-2 py-1 text-xs"
                >
                  {attachment.filename}
                  <button
                    type="button"
                    aria-label={`Remove ${attachment.filename}`}
                    onClick={() => setAttachments((current) => current.filter((item) => item !== attachment))}
                  >
                    <X className="size-3" aria-hidden="true" />
                  </button>
                </span>
              ))}
            </div>
          ) : null}

          {attachmentError ? <p className="text-sm text-destructive">{attachmentError}</p> : null}
          {encrypt && (
            <div className="flex flex-wrap items-center gap-2 rounded-md border border-border bg-secondary/40 px-3 py-2 text-[12.5px] text-muted-foreground">
              {keyDiscoveryPending ? (
                <Lock className="size-4 animate-pulse" aria-hidden="true" />
              ) : missingRecipientKeys.length ? (
                <TriangleAlert className="size-4 text-destructive" aria-hidden="true" />
              ) : (
                <Lock className="size-4" aria-hidden="true" />
              )}
              <span>
                {keyDiscoveryPending
                  ? t('compose.discoveringKeys')
                  : missingRecipientKeys.length
                  ? t('compose.missingKeys', { emails: missingRecipientKeys.join(', ') })
                  : t('compose.allKeysReady')}
              </span>
            </div>
          )}
          {cryptoError ? <p className="text-sm text-destructive">{cryptoError}</p> : null}
          {sendMessage.error ? (
            <p className="text-sm text-destructive">
              {t('compose.sendFailed')} {sendErrorDetail(sendMessage.error)}
            </p>
          ) : null}
          {saveDraft.error ? <p className="text-sm text-destructive">{t('compose.draftSaveFailed')}</p> : null}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => void handleSaveAndClose()} disabled={saveDraft.isPending}>
              {saveDraft.isPending ? t('compose.savingDraft') : t('compose.saveDraft')}
            </Button>
            <Button
              type="button"
              onClick={handleSend}
              disabled={
                sendMessage.isPending ||
                saveDraft.isPending ||
                !selectedIdentity ||
                !recipientsValid ||
                !isOnline ||
                keyDiscoveryPending ||
                missingRecipientKeys.length > 0
              }
            >
              <Send className="size-4" aria-hidden="true" />
              {!isOnline ? t('compose.noConnection') : sendMessage.isPending ? t('compose.sending') : t('compose.send')}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function useSenderIdentityData(accounts: Account[]): { groups: SenderGroup[] } {
  const aliasQueries = useQueries({
    queries: accounts.map((account) => ({
      queryKey: ['aliases', account.id],
      queryFn: () => apiGet<AccountAlias[]>(`/accounts/${account.id}/aliases`),
    })),
  })

  return useMemo(
    () => ({
      groups: accounts.map((account, index) => {
        const aliases = aliasQueries[index]?.data ?? []
        const identities = [
          { accountId: account.id, email: account.primary_email, label: account.primary_email, isPrimary: true },
          ...aliases.map((alias) => ({
            accountId: account.id,
            email: alias.email,
            label: alias.display_name ? `${alias.display_name} <${alias.email}>` : alias.email,
            isPrimary: false,
          })),
        ]
        return { accountName: account.display_name, identities }
      }),
    }),
    [accounts, aliasQueries],
  )
}

function identityKey(identity: SenderIdentity): string {
  return `${identity.accountId}|${identity.email}`
}

function defaultFromIdentity(initialState: ComposeInitialState, identities: SenderIdentity[]): string {
  const sourceMessage = initialState.sourceMessage
  if (!sourceMessage || (initialState.mode !== 'reply' && initialState.mode !== 'draft')) return identityKey(identities[0])

  const accountIdentities = identities.filter((identity) => identity.accountId === sourceMessage.account_id)
  if (initialState.mode === 'draft') {
    const draftIdentity = accountIdentities.find(
      (identity) => identity.email.toLowerCase() === sourceMessage.from_addr.toLowerCase(),
    )
    return identityKey(draftIdentity ?? accountIdentities[0] ?? identities[0])
  }
  const candidates = parseAddressList(`${sourceMessage.to_addrs},${sourceMessage.cc_addrs}`).map((address) =>
    address.toLowerCase(),
  )

  for (const candidate of candidates) {
    const match = accountIdentities.find((identity) => identity.email.toLowerCase() === candidate)
    if (match) return identityKey(match)
  }

  const primary = accountIdentities.find((identity) => identity.isPrimary)
  return identityKey(primary ?? accountIdentities[0] ?? identities[0])
}

function toAttachmentInput(attachment: AttachmentDraft): AttachmentInput {
  return { filename: attachment.filename, content_type: attachment.content_type, data: attachment.data }
}

function sendErrorDetail(error: unknown): string {
  if (error instanceof ApiError) return error.detail ?? error.message
  if (error instanceof Error) return error.message
  return ''
}

function buildSubject(mode: ComposeInitialState['mode'], message?: Message): string {
  if (!message) return ''
  if (mode === 'draft') return message.subject
  if (mode === 'reply') return message.subject.toLowerCase().startsWith('re:') ? message.subject : `Re: ${message.subject}`
  if (mode === 'forward') return message.subject.toLowerCase().startsWith('fwd:') ? message.subject : `Fwd: ${message.subject}`
  return ''
}

function buildBody(mode: ComposeInitialState['mode'], message: Message | undefined, t: TFunction): string {
  if (mode === 'draft') return message?.body_html ?? ''
  if (!message || mode === 'new') return ''
  const quoted = escapeHtml(messagePlain(message)).replaceAll(/\r?\n/g, '<br>')
  if (mode === 'reply') {
    return `<p><br></p><blockquote>${escapeHtml(t('compose.previousMessage'))}<br>${quoted}</blockquote>`
  }

  const date = formatForwardedDate(message.date ?? message.internal_date)
  const headerRows = [
    [t('compose.forwardedFrom'), message.from_addr],
    [t('compose.forwardedDate'), date],
    [t('compose.forwardedSubject'), message.subject],
    [t('compose.forwardedTo'), message.to_addrs],
    ...(message.cc_addrs ? [[t('compose.forwardedCc'), message.cc_addrs]] : []),
  ]
    .map(([label, value]) => `<div><strong>${escapeHtml(label)}:</strong> ${escapeHtml(value)}</div>`)
    .join('')

  return `<p><br></p><div>---------- ${escapeHtml(t('compose.forwardedMessage'))} ---------</div>${headerRows}<div><br></div><div>${quoted}</div>`
}

function formatForwardedDate(value: string): string {
  const date = new Date(value)
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
}

function parseAddressList(value: string): string[] {
  return value
    .split(',')
    .map((part) => parseAddr(part))
    .filter(Boolean)
}

function parseAddr(part: string): string {
  const trimmed = part.trim()
  const match = trimmed.match(/<(.+)>/)
  return (match ? match[1] : trimmed).trim()
}

function htmlToText(value: string): string {
  const node = document.createElement('div')
  node.innerHTML = value
  return node.textContent ?? ''
}

function escapeHtml(value: string): string {
  return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;')
}

async function fileToAttachment(file: File): Promise<AttachmentDraft> {
  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(file)
  })
  return {
    filename: file.name,
    content_type: file.type || 'application/octet-stream',
    data: dataUrl.split(',')[1] ?? '',
    size: file.size,
  }
}

interface FieldProps {
  id: string
  label: string
  children: React.ReactNode
}

function Field({ id, label, children }: FieldProps) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={id} className="text-[11px] font-bold uppercase tracking-wide text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  )
}
