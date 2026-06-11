import { useEffect, useMemo, useRef, useState } from 'react'
import { useQueries } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { Bold, Italic, Underline, Strikethrough, List, ListOrdered, Paperclip, Send, Type, X } from 'lucide-react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { cn } from '@/shared/lib/utils'
import { apiGet } from '@/shared/api'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useSendMessage } from '@/shared/hooks/useMessages'
import { useOnlineStatus } from '@/shared/hooks/useOnlineStatus'
import type { Account, AccountAlias, AttachmentInput, Message } from '@/shared/types'
import type { ComposeInitialState } from '../types'
import { isValidEmail } from '@/shared/lib/email'
import { RecipientChips } from './RecipientChips'

const MAX_ATTACHMENT_BYTES = 25 * 1024 * 1024

interface ComposeDialogProps {
  open: boolean
  initialState: ComposeInitialState
  onClose: () => void
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

export function ComposeDialog({ open, initialState, onClose }: ComposeDialogProps) {
  const { t } = useTranslation()
  const editorRef = useRef<HTMLDivElement | null>(null)
  const isOnline = useOnlineStatus()
  const { data: accounts = [] } = useAccounts()
  const senderData = useSenderIdentityData(accounts)
  const identities = senderData.groups.flatMap((group) => group.identities)
  const sendMessage = useSendMessage()
  const sourceMessage = initialState.sourceMessage

  const initialTo =
    initialState.to ??
    (initialState.mode === 'reply' && sourceMessage ? parseAddressList(sourceMessage.from_addr) : [])
  const initialBody = buildBody(initialState.mode, sourceMessage)

  const [from, setFrom] = useState('')
  const [to, setTo] = useState<string[]>(initialTo)
  const [cc, setCc] = useState<string[]>([])
  const [bcc, setBcc] = useState<string[]>([])
  const [showCc, setShowCc] = useState(false)
  const [showBcc, setShowBcc] = useState(false)
  const [subject, setSubject] = useState(buildSubject(initialState.mode, sourceMessage))
  const [bodyHtml, setBodyHtml] = useState(initialBody)
  const [bodyText, setBodyText] = useState('')
  const [plainText, setPlainText] = useState(false)
  const [attachments, setAttachments] = useState<AttachmentDraft[]>([])
  const [attachmentError, setAttachmentError] = useState<string | null>(null)

  const selectedIdentity = identities.find((identity) => identityKey(identity) === from) ?? identities[0]
  const title =
    initialState.mode === 'reply'
      ? t('action.reply')
      : initialState.mode === 'forward'
        ? t('action.forward')
        : t('compose.newMessage')

  const allRecipients = [...to, ...cc, ...bcc]
  const recipientsValid = to.length > 0 && allRecipients.every(isValidEmail)

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

  function handleSend() {
    if (!selectedIdentity) return
    const html = plainText ? undefined : editorRef.current?.innerHTML ?? bodyHtml
    const text = plainText ? bodyText : htmlToText(html ?? '')
    sendMessage.mutate(
      {
        account_id: selectedIdentity.accountId,
        from: selectedIdentity.email,
        to,
        cc,
        bcc,
        subject,
        body_html: html,
        body_text: text,
        in_reply_to: initialState.mode === 'reply' ? sourceMessage?.message_id_header : null,
        references: initialState.mode === 'reply' || initialState.mode === 'forward' ? sourceMessage?.references : null,
        attachments: attachments.map(toAttachmentInput),
      },
      { onSuccess: () => onClose() },
    )
  }

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(860px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-3.5">
          <Field id="compose-from" label={t('compose.from')}>
            <Select
              id="compose-from"
              value={from || (identities[0] ? identityKey(identities[0]) : '')}
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
            <label className="ml-auto inline-flex h-8 cursor-pointer items-center gap-2 rounded-md border border-input px-3 text-[13px] transition-colors hover:bg-secondary">
              <Paperclip className="size-4" aria-hidden="true" />
              {t('compose.attach')}
              <input className="sr-only" type="file" multiple onChange={(event) => handleFiles(event.currentTarget.files)} />
            </label>
          </div>

          {plainText ? (
            <textarea
              aria-label={t('compose.body')}
              value={bodyText}
              onChange={(e) => setBodyText(e.currentTarget.value)}
              className="min-h-56 resize-y rounded-md border border-input bg-background px-3 py-2 font-mono text-[13px] leading-6 outline-none focus-visible:ring-1 focus-visible:ring-ring"
            />
          ) : (
            <div
              ref={editorRef}
              contentEditable
              aria-label={t('compose.body')}
              className="min-h-56 rounded-md border border-input bg-background px-3 py-2 text-sm leading-6 outline-none focus-visible:ring-1 focus-visible:ring-ring"
              onInput={(event) => setBodyHtml(event.currentTarget.innerHTML)}
            />
          )}

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
          {sendMessage.error ? <p className="text-sm text-destructive">{t('compose.sendFailed')}</p> : null}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              {t('action.cancel')}
            </Button>
            <Button
              type="button"
              onClick={handleSend}
              disabled={sendMessage.isPending || !selectedIdentity || !recipientsValid || !isOnline}
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

function toAttachmentInput(attachment: AttachmentDraft): AttachmentInput {
  return { filename: attachment.filename, content_type: attachment.content_type, data: attachment.data }
}

function buildSubject(mode: ComposeInitialState['mode'], message?: Message): string {
  if (!message) return ''
  if (mode === 'reply') return message.subject.toLowerCase().startsWith('re:') ? message.subject : `Re: ${message.subject}`
  if (mode === 'forward') return message.subject.toLowerCase().startsWith('fwd:') ? message.subject : `Fwd: ${message.subject}`
  return ''
}

function buildBody(mode: ComposeInitialState['mode'], message?: Message): string {
  if (!message || mode === 'new') return ''
  const quoted = escapeHtml(message.body_text ?? message.snippet)
  const heading = mode === 'reply' ? 'On previous message:' : 'Forwarded message:'
  return `<p><br></p><blockquote>${heading}<br>${quoted}</blockquote>`
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
