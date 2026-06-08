import { useEffect, useMemo, useRef, useState } from 'react'
import { useQueries } from '@tanstack/react-query'
import { Bold, Italic, Paperclip, Send, X } from 'lucide-react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { apiGet } from '@/shared/api'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useSendMessage } from '@/shared/hooks/useMessages'
import { useOnlineStatus } from '@/shared/hooks/useOnlineStatus'
import type { Account, AccountAlias, AttachmentInput, Message } from '@/shared/types'
import type { ComposeInitialState } from '../types'

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

export function ComposeDialog({ open, initialState, onClose }: ComposeDialogProps) {
  const editorRef = useRef<HTMLDivElement | null>(null)
  const isOnline = useOnlineStatus()
  const { data: accounts = [] } = useAccounts()
  const senderData = useSenderIdentityData(accounts)
  const identities = senderData.groups.flatMap((group) => group.identities)
  const sendMessage = useSendMessage()
  const sourceMessage = initialState.sourceMessage
  const initialTo = initialState.mode === 'reply' && sourceMessage ? parseAddressList(sourceMessage.from_addr).join(', ') : ''
  const initialSubject = buildSubject(initialState.mode, sourceMessage)
  const initialBody = buildBody(initialState.mode, sourceMessage)
  const [from, setFrom] = useState('')
  const [to, setTo] = useState(initialTo)
  const [cc, setCc] = useState('')
  const [bcc, setBcc] = useState('')
  const [subject, setSubject] = useState(initialSubject)
  const [bodyHtml, setBodyHtml] = useState(initialBody)
  const [attachments, setAttachments] = useState<AttachmentDraft[]>([])
  const [attachmentError, setAttachmentError] = useState<string | null>(null)

  const selectedIdentity = identities.find((identity) => identityKey(identity) === from) ?? identities[0]
  const title = initialState.mode === 'reply' ? 'Reply' : initialState.mode === 'forward' ? 'Forward' : 'New message'

  useEffect(() => {
    if (editorRef.current && open) {
      editorRef.current.innerHTML = bodyHtml
    }
  }, [bodyHtml, open])

  function applyFormat(command: 'bold' | 'italic') {
    editorRef.current?.focus()
    document.execCommand(command)
    setBodyHtml(editorRef.current?.innerHTML ?? '')
  }

  async function handleFiles(files: FileList | null) {
    if (!files) {
      return
    }

    const nextFiles = await Promise.all(Array.from(files).map(fileToAttachment))
    const nextSize = [...attachments, ...nextFiles].reduce((total, attachment) => total + attachment.size, 0)
    if (nextSize > MAX_ATTACHMENT_BYTES) {
      setAttachmentError('Attachments exceed the 25 MB message limit.')
      return
    }

    setAttachments((current) => [...current, ...nextFiles])
    setAttachmentError(null)
  }

  function handleSend() {
    if (!selectedIdentity) {
      return
    }

    const html = editorRef.current?.innerHTML ?? bodyHtml
    sendMessage.mutate(
      {
        account_id: selectedIdentity.accountId,
        from: selectedIdentity.email,
        to: parseAddressList(to),
        cc: parseAddressList(cc),
        bcc: parseAddressList(bcc),
        subject,
        body_html: html,
        body_text: htmlToText(html),
        in_reply_to: initialState.mode === 'reply' ? sourceMessage?.message_id_header : null,
        references: initialState.mode === 'reply' || initialState.mode === 'forward' ? sourceMessage?.references : null,
        attachments: attachments.map(toAttachmentInput),
      },
      {
        onSuccess: () => {
          onClose()
        },
      },
    )
  }

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(860px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <div className="grid gap-3 md:grid-cols-[220px_1fr]">
            <Field id="compose-from" label="From">
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
            <Field id="compose-to" label="To">
              <Input id="compose-to" value={to} onChange={(event) => setTo(event.currentTarget.value)} />
            </Field>
          </div>

          <div className="grid gap-3 md:grid-cols-2">
            <Field id="compose-cc" label="Cc">
              <Input id="compose-cc" value={cc} onChange={(event) => setCc(event.currentTarget.value)} />
            </Field>
            <Field id="compose-bcc" label="Bcc">
              <Input id="compose-bcc" value={bcc} onChange={(event) => setBcc(event.currentTarget.value)} />
            </Field>
          </div>

          <Field id="compose-subject" label="Subject">
            <Input id="compose-subject" value={subject} onChange={(event) => setSubject(event.currentTarget.value)} />
          </Field>

          <div className="flex items-center gap-2 border-b border-border pb-2">
            <Button type="button" variant="ghost" size="icon" aria-label="Bold" onClick={() => applyFormat('bold')}>
              <Bold className="size-4" aria-hidden="true" />
            </Button>
            <Button type="button" variant="ghost" size="icon" aria-label="Italic" onClick={() => applyFormat('italic')}>
              <Italic className="size-4" aria-hidden="true" />
            </Button>
            <label className="ml-auto inline-flex h-9 cursor-pointer items-center gap-2 rounded-md border border-input px-3 text-sm transition-colors hover:bg-accent">
              <Paperclip className="size-4" aria-hidden="true" />
              Attach
              <input className="sr-only" type="file" multiple onChange={(event) => handleFiles(event.currentTarget.files)} />
            </label>
          </div>

          <div
            ref={editorRef}
            contentEditable
            aria-label="Message body"
            className="min-h-56 rounded-md border border-input bg-background px-3 py-2 text-sm leading-6 outline-none focus-visible:ring-1 focus-visible:ring-ring"
            onInput={(event) => setBodyHtml(event.currentTarget.innerHTML)}
          />

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
          {sendMessage.error ? <p className="text-sm text-destructive">Message could not be sent.</p> : null}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button type="button" onClick={handleSend} disabled={sendMessage.isPending || !selectedIdentity || !to.trim() || !isOnline}>
              <Send className="size-4" aria-hidden="true" />
              {!isOnline ? 'No connection' : sendMessage.isPending ? 'Sending...' : 'Send'}
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
          {
            accountId: account.id,
            email: account.primary_email,
            label: account.primary_email,
            isPrimary: true,
          },
          ...aliases.map((alias) => ({
            accountId: account.id,
            email: alias.email,
            label: alias.display_name ? `${alias.display_name} <${alias.email}>` : alias.email,
            isPrimary: false,
          })),
        ]

        return {
          accountName: account.display_name,
          identities,
        }
      }),
    }),
    [accounts, aliasQueries],
  )
}

function identityKey(identity: SenderIdentity): string {
  return `${identity.accountId}|${identity.email}`
}

function toAttachmentInput(attachment: AttachmentDraft): AttachmentInput {
  return {
    filename: attachment.filename,
    content_type: attachment.content_type,
    data: attachment.data,
  }
}

function buildSubject(mode: ComposeInitialState['mode'], message?: Message): string {
  if (!message) {
    return ''
  }

  if (mode === 'reply') {
    return message.subject.toLowerCase().startsWith('re:') ? message.subject : `Re: ${message.subject}`
  }

  if (mode === 'forward') {
    return message.subject.toLowerCase().startsWith('fwd:') ? message.subject : `Fwd: ${message.subject}`
  }

  return ''
}

function buildBody(mode: ComposeInitialState['mode'], message?: Message): string {
  if (!message || mode === 'new') {
    return ''
  }

  const quoted = escapeHtml(message.body_text ?? message.snippet)
  const heading = mode === 'reply' ? 'On previous message:' : 'Forwarded message:'
  return `<p><br></p><blockquote>${heading}<br>${quoted}</blockquote>`
}

function parseAddressList(value: string): string[] {
  return value
    .split(',')
    .map((part) => part.trim())
    .filter(Boolean)
}

function htmlToText(value: string): string {
  const node = document.createElement('div')
  node.innerHTML = value
  return node.textContent ?? ''
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
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
      <Label htmlFor={id}>{label}</Label>
      {children}
    </div>
  )
}
