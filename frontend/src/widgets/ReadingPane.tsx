import { useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useOutletContext } from 'react-router-dom'
import {
  Archive,
  Trash2,
  MailCheck,
  Mail,
  MailOpen,
  Star,
  Reply,
  ReplyAll,
  Forward,
  Check,
  Copy,
  Download,
  Code,
  FileText,
  ImageOff,
  List,
  ChevronDown,
  ShieldAlert,
  ShieldCheck,
  FilePenLine,
  RefreshCw,
} from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { Button } from '@/shared/components/ui/button'
import { formatDate, parseFromAddr } from '@/shared/lib/format'
import { accountColor, accountInitials } from '@/shared/lib/avatar'
import { useAccountColorLookup } from '@/shared/hooks/useAccounts'
import {
  messageHeaders,
  messageHtml,
  messagePlain,
  messageRawEml,
  downloadEml,
} from '@/shared/lib/messageSource'
import { apiGetBlob, getAccessToken } from '@/shared/api'
import { blockRemoteContent, proxyRemoteContent } from '@/shared/lib/remoteContent'
import { prepareEmailHtml } from '@/shared/lib/emailHtml'
import {
  useArchiveThread,
  useDeleteThread,
  useMarkThreadRead,
  useMarkRead,
  useToggleFlag,
  useDeleteMessage,
  useNotSpamMessage,
  useNotSpamInPlace,
  useMessage,
  useReanalyseMessage,
  useThread,
} from '@/shared/hooks/useMessages'
import { useMeetingInvitations, useRsvpInvitation } from '@/shared/hooks/useCalendar'
import { useContactSearch } from '@/shared/hooks/useContacts'
import { useAddAllowedImageSender, useImageAllowlist, usePublicConfig, useSettings } from '@/shared/hooks/useSettings'
import { AttachmentList } from '@/widgets/AttachmentList'
import { PgpMessagePanel } from '@/widgets/PgpMessagePanel'
import type { MailOutletContext } from '@/pages/MailLayout'
import type { MeetingInvitation, Message } from '@/shared/types'

function ToolButton({
  label,
  onClick,
  disabled,
  danger,
  children,
}: {
  label: string
  onClick: () => void
  disabled?: boolean
  danger?: boolean
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'flex size-9 items-center justify-center rounded-md border border-border bg-card transition-colors hover:bg-secondary disabled:opacity-50',
        danger ? 'text-destructive' : 'text-secondary-foreground',
      )}
    >
      {children}
    </button>
  )
}

export function ReadingPaneEmpty() {
  const { t } = useTranslation()
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 bg-background p-8 text-center">
      <div className="flex size-[76px] items-center justify-center rounded-full border border-border bg-card shadow-sm">
        <MailOpen className="size-9 text-input" aria-hidden="true" />
      </div>
      <h2 className="text-[15px] font-semibold text-muted-foreground">{t('mail.selectConversation')}</h2>
      <p className="max-w-[280px] text-[13px] leading-relaxed text-muted-foreground">{t('mail.selectHint')}</p>
    </div>
  )
}

export function ThreadDetail({ threadId, onThreadGone }: { threadId: string; onThreadGone?: () => void }) {
  const { t } = useTranslation()
  const { data, isLoading } = useThread(threadId)
  const archiveThread = useArchiveThread()
  const deleteThread = useDeleteThread()
  const markThreadRead = useMarkThreadRead()
  const notSpam = useNotSpamMessage()
  const { openCompose } = useOutletContext<MailOutletContext>()
  const colorFor = useAccountColorLookup()
  const messages = useMemo(() => data?.messages ?? [], [data?.messages])
  const lastMessageId = messages.at(-1)?.id ?? ''
  const { data: lastMessageDetail, isLoading: isLastMessageLoading } = useMessage(lastMessageId)
  const firstMessage = messages[0]

  const defaultExpandedId = useMemo(() => {
    const unread = [...messages].reverse().find((m) => !m.is_read)
    return unread?.id ?? messages.at(-1)?.id
  }, [messages])

  if (!threadId) return <ReadingPaneEmpty />
  if (isLoading) {
    return (
      <div
        className="flex h-full items-center justify-center gap-2 bg-background text-sm text-muted-foreground"
        role="status"
        aria-live="polite"
      >
        <RefreshCw className="size-4 motion-safe:animate-spin motion-reduce:animate-none" aria-hidden="true" />
        {t('mail.loadingThread')}
      </div>
    )
  }
  if (!messages.length) {
    return (
      <div className="flex h-full items-center justify-center bg-background text-sm text-muted-foreground">
        {t('mail.threadNotFound')}
      </div>
    )
  }

  const fromEmail = parseFromAddr(firstMessage.from_addr).email
  const color = colorFor(firstMessage.account_id)
  const lastMessage = messages.at(-1)!
  const lastMessageSource = lastMessageDetail ?? lastMessage
  const lastMessageBodyReady = hasCompleteBody(lastMessageDetail)
  const lastMessageIsSpam = isSpamMessage(lastMessage)
  const lastMessageIsDraft = Boolean(lastMessage.is_local_draft)
  const hasSpamMessages = messages.some(isSpamMessage)

  return (
    <article className="flex h-full min-h-0 flex-col overflow-y-auto bg-background">
      {/* toolbar — sticky so the actions stay reachable while the thread scrolls */}
      <div className="sticky top-0 z-10 flex shrink-0 items-center gap-2 border-b border-border bg-card px-5 py-3">
        {lastMessageIsDraft ? (
          <ToolButton
            label={t('compose.editDraft')}
            onClick={() => lastMessageDetail && openCompose({ mode: 'draft', sourceMessage: lastMessageDetail })}
            disabled={isLastMessageLoading || !lastMessageDetail}
          >
            <FilePenLine className="size-4" />
          </ToolButton>
        ) : null}
        {!lastMessageIsDraft ? (
          <ToolButton
            label={t('action.archive')}
            onClick={() => archiveThread.mutate(threadId, { onSuccess: () => onThreadGone?.() })}
            disabled={archiveThread.isPending}
          >
            <Archive className="size-4" />
          </ToolButton>
        ) : null}
        {lastMessageIsSpam && (
          <ToolButton
            label={t('action.notSpam')}
            onClick={() => notSpam.mutate(lastMessage.id, { onSuccess: () => messages.length === 1 && onThreadGone?.() })}
            disabled={notSpam.isPending}
          >
            <ShieldCheck className="size-4" />
          </ToolButton>
        )}
        <ToolButton
          label={t('action.delete')}
          danger
          onClick={() => deleteThread.mutate(threadId, { onSuccess: () => onThreadGone?.() })}
          disabled={deleteThread.isPending}
        >
          <Trash2 className="size-4" />
        </ToolButton>
        {!lastMessageIsDraft ? (
          <>
            <ToolButton
              label={t('action.markRead')}
              onClick={() => markThreadRead.mutate({ threadId, is_read: true })}
              disabled={markThreadRead.isPending}
            >
              <MailCheck className="size-4" />
            </ToolButton>
            <ToolButton
              label={t('action.markUnread')}
              onClick={() => markThreadRead.mutate({ threadId, is_read: false })}
              disabled={markThreadRead.isPending}
            >
              <Mail className="size-4" />
            </ToolButton>
          </>
        ) : null}
        <span
          className="ml-auto inline-flex items-center gap-2 rounded-full px-3 py-1 text-[12px] font-semibold"
          style={{ color, backgroundColor: `${color}1f` }}
        >
          <span className="size-2 rounded-full" style={{ backgroundColor: color }} />
          {fromEmail}
        </span>
      </div>

      {/* subject */}
      <div className="shrink-0 px-6 pb-2.5 pt-5">
        <h1 className="text-[21px] font-bold leading-tight tracking-tight text-foreground">
          {firstMessage.subject || t('mail.noSubject')}
        </h1>
        <div className="mt-2.5 flex flex-wrap gap-1.5">
          <span className="rounded bg-secondary px-2 py-0.5 text-[11px] font-semibold text-muted-foreground">
            {t('mail.messages', { count: messages.length })}
          </span>
          {hasSpamMessages && (
            <span className="inline-flex items-center gap-1 rounded border border-amber-500/50 bg-amber-500/15 px-2 py-0.5 text-[11px] font-bold text-amber-800 dark:text-amber-200">
              <ShieldAlert className="size-3" aria-hidden="true" />
              {t('mail.spamSuspected')}
            </span>
          )}
        </div>
      </div>

      {/* messages */}
      <div className="flex flex-col gap-2.5 px-6 pb-6 pt-2">
        {messages.map((message) => (
          <MessageCard
            key={message.id}
            message={message}
            defaultExpanded={message.id === defaultExpandedId}
            onDeleted={messages.length === 1 ? onThreadGone : undefined}
          />
        ))}

        {!lastMessageIsDraft ? (
          <div className="mt-1.5 flex shrink-0 gap-2.5">
            <button
              onClick={() => openCompose({ mode: 'reply', sourceMessage: lastMessageSource })}
              disabled={isLastMessageLoading || !lastMessageBodyReady}
              className="inline-flex h-9 items-center gap-2 rounded-md bg-[#2563eb] px-4 text-[13px] font-semibold text-white shadow-sm transition-colors hover:bg-[#1d4ed8] disabled:opacity-50"
            >
              <Reply className="size-[15px]" />
              {t('action.reply')}
            </button>
          <button
            onClick={() => openCompose({ mode: 'reply', sourceMessage: lastMessageSource })}
            disabled={isLastMessageLoading || !lastMessageBodyReady}
            className="inline-flex h-9 items-center gap-2 rounded-md border border-border bg-card px-4 text-[13px] font-semibold text-secondary-foreground transition-colors hover:bg-secondary disabled:opacity-50"
          >
            <ReplyAll className="size-[15px]" />
            {t('action.replyAll')}
          </button>
          <button
            onClick={() => openCompose({ mode: 'forward', sourceMessage: lastMessageSource })}
            disabled={isLastMessageLoading || !lastMessageBodyReady}
            className="inline-flex h-9 items-center gap-2 rounded-md border border-border bg-card px-4 text-[13px] font-semibold text-secondary-foreground transition-colors hover:bg-secondary disabled:opacity-50"
          >
            <Forward className="size-[15px]" />
            {t('action.forward')}
          </button>
          </div>
        ) : null}
      </div>
    </article>
  )
}

function PhishingBanner({ message }: { message: Message }) {
  const { t } = useTranslation()
  const [expanded, setExpanded] = useState(false)
  const notSpam = useNotSpamInPlace()
  const verdict = message.phishing_verdict
  if (verdict !== 'suspicious' && verdict !== 'phishing') return null
  const danger = verdict === 'phishing'
  const checks = message.phishing_checks ?? []
  // The spam-folder banner already offers "not spam" with move-to-inbox
  // semantics; don't show a second, subtly different button next to it.
  const inSpamFolder = isSpamMessage(message)

  return (
    <div
      className={cn(
        'mb-3.5 rounded-md border px-3 py-2.5 text-[12.5px]',
        danger
          ? 'border-red-500/50 bg-red-500/10 text-red-600 dark:text-red-300'
          : 'border-amber-500/50 bg-amber-500/10 text-amber-700 dark:text-amber-300',
      )}
    >
      <div className="flex w-full items-center gap-2">
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="flex min-w-0 flex-1 items-center gap-2 text-left font-semibold"
        >
          <ShieldAlert className="size-4 shrink-0" aria-hidden="true" />
          <span>{danger ? t('mail.phishingLikely') : t('mail.phishingSuspicious')}</span>
          {checks.length > 0 && (
            <ChevronDown className={cn('ml-auto size-4 shrink-0 transition-transform', expanded && 'rotate-180')} />
          )}
        </button>
        {!inSpamFolder && (
          <button
            type="button"
            onClick={() => notSpam.mutate(message.id)}
            disabled={notSpam.isPending}
            title={t('mail.notSpamLearns')}
            className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border bg-card px-2.5 text-[12px] font-bold text-foreground shadow-sm hover:bg-secondary disabled:opacity-50"
          >
            <ShieldCheck className="size-3.5" aria-hidden="true" />
            {t('action.notSpam')}
          </button>
        )}
      </div>
      {expanded && checks.length > 0 && (
        <ul className="mt-2 flex list-disc flex-col gap-1 pl-9">
          {checks.map((check, i) => (
            <li key={check.id + i}>
              {/* Older analyses lack `params`; fall back to the English detail
                  rather than leaving "{{placeholders}}" unfilled. */}
              {check.params
                ? t(`phishingCheck.${check.id}`, { ...check.params, defaultValue: check.detail })
                : check.detail}{' '}
              <span className="opacity-70">(+{check.points})</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

function SpamBanner({ onNotSpam, pending }: { onNotSpam: () => void; pending: boolean }) {
  const { t } = useTranslation()
  return (
    <div className="mb-3.5 flex flex-wrap items-center gap-3 rounded-md border border-amber-500/60 bg-amber-500/15 px-3 py-2.5 text-amber-950 dark:text-amber-100">
      <span className="flex size-9 shrink-0 items-center justify-center rounded-full bg-amber-500/20">
        <ShieldAlert className="size-5" aria-hidden="true" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[13px] font-bold">{t('mail.spamSuspected')}</span>
        <span className="block text-[12.5px] leading-relaxed text-amber-900/80 dark:text-amber-100/80">
          {t('mail.spamSuspectedDescription')}
        </span>
      </span>
      <button
        type="button"
        onClick={onNotSpam}
        disabled={pending}
        className="inline-flex h-8 items-center gap-1.5 rounded-md border border-amber-600/40 bg-card px-3 text-[12px] font-bold text-foreground shadow-sm hover:bg-secondary disabled:opacity-50"
      >
        <ShieldCheck className="size-4" aria-hidden="true" />
        {t('action.notSpam')}
      </button>
    </div>
  )
}

type SourceView = 'html' | 'text' | 'headers' | 'raw'

const VIEW_TABS: { id: SourceView; tkey: string; icon: typeof Code }[] = [
  { id: 'html', tkey: 'view.html', icon: MailOpen },
  { id: 'text', tkey: 'view.text', icon: FileText },
  { id: 'headers', tkey: 'view.headers', icon: List },
  { id: 'raw', tkey: 'view.raw', icon: Code },
]

function MessageCard({
  message,
  defaultExpanded,
  onDeleted,
}: {
  message: Message
  defaultExpanded: boolean
  onDeleted?: () => void
}) {
  const { t } = useTranslation()
  const { openCompose } = useOutletContext<MailOutletContext>()
  const colorFor = useAccountColorLookup()
  const [expanded, setExpanded] = useState(defaultExpanded)
  const [view, setView] = useState<SourceView>('html')
  const sender = parseFromAddr(message.from_addr)
  const senderDomain = sender.email.split('@')[1]?.toLowerCase() ?? ''
  const { data: detail, isLoading } = useMessage(expanded ? message.id : '')
  const markRead = useMarkRead()
  const toggleFlag = useToggleFlag()
  const deleteMessage = useDeleteMessage()
  const notSpam = useNotSpamMessage()
  const reanalyseMessage = useReanalyseMessage()
  const { data: settings } = useSettings()
  const { data: publicConfig } = usePublicConfig()
  const { data: imageAllowlist } = useImageAllowlist()
  const addAllowedSender = useAddAllowedImageSender()
  const [showRemoteOnce, setShowRemoteOnce] = useState(false)
  const [contactOpen, setContactOpen] = useState(false)
  const [cidUrls, setCidUrls] = useState<Record<string, string>>({})
  const [bodySpinnerExpired, setBodySpinnerExpired] = useState(false)
  const displayed = detail ?? message
  const { data: senderMatches = [] } = useContactSearch(contactOpen ? sender.email : '')
  const senderContact = senderMatches.find((contact) =>
    contact.emails.some((email) => email.value.toLowerCase() === sender.email.toLowerCase()),
  )

  // Never leave the entire reading surface behind an unbounded network
  // spinner. After a short grace period render the list's safe snippet while
  // the real body request continues and replaces it from the query cache.
  useEffect(() => {
    if (!expanded || detail) return
    const timeout = window.setTimeout(() => setBodySpinnerExpired(true), 750)
    return () => window.clearTimeout(timeout)
  }, [detail, expanded, message.id])

  // Resolve inline attachments: the HTML references them as cid:<Content-ID>,
  // which the browser can't load — fetch each one (authenticated) and swap in
  // an object URL.
  const inlineAttachments = useMemo(
    () => (displayed.attachments ?? []).filter((a) => a.content_id),
    [displayed.attachments],
  )
  const visibleAttachments = useMemo(() => {
    const html = (displayed.body_html ?? '').toLowerCase()
    return (displayed.attachments ?? []).filter((attachment) => {
      const contentId = attachment.content_id?.toLowerCase()
      return !contentId || !html.includes(`cid:${contentId}`)
    })
  }, [displayed.attachments, displayed.body_html])
  useEffect(() => {
    if (!expanded || !inlineAttachments.length) return
    let cancelled = false
    const urls: string[] = []
    Promise.allSettled(
      inlineAttachments.map(async (att) => {
        const blob = await apiGetBlob(`/attachments/${att.id}`)
        const url = URL.createObjectURL(blob)
        urls.push(url)
        return [att.content_id as string, url] as const
      }),
    )
      .then((results) => {
        const entries = results.flatMap((result) => (result.status === 'fulfilled' ? [result.value] : []))
        if (!cancelled) setCidUrls(Object.fromEntries(entries))
      })
    return () => {
      cancelled = true
      urls.forEach((url) => URL.revokeObjectURL(url))
    }
  }, [expanded, inlineAttachments])
  const bodyLoaded = Boolean(displayed.body_html || displayed.body_text || displayed.snippet)
  const offlineMissing = !navigator.onLine && displayed.body_available === false
  const color = colorFor(message.account_id)
  const isSpam = isSpamMessage(displayed)

  useEffect(() => {
    if (expanded && bodyLoaded && !displayed.is_read && !markRead.isPending) {
      markRead.mutate({ id: displayed.id, is_read: true })
    }
  }, [bodyLoaded, displayed.id, displayed.is_read, expanded, markRead])

  const headers = useMemo(() => (expanded ? messageHeaders(displayed) : []), [expanded, displayed])
  const raw = useMemo(() => (expanded && view === 'raw' ? messageRawEml(displayed) : ''), [expanded, view, displayed])

  const senderEmail = sender.email.toLowerCase()
  const allowRemote =
    Boolean(settings?.load_external_images) ||
    showRemoteOnce ||
    (imageAllowlist ?? []).some((entry) => entry.sender === senderEmail)
  const htmlContent = useMemo(() => {
    if (!expanded || view !== 'html') return { html: '', blocked: false }
    const rawHtml = prepareEmailHtml(displayed.body_html ?? messageHtml(displayed), cidUrls)
    const content = allowRemote
      ? {
          html: publicConfig?.remote_image_proxy_enabled
            ? proxyRemoteContent(rawHtml, getAccessToken())
            : rawHtml,
          blocked: false,
        }
      : blockRemoteContent(rawHtml)
    return displayed.phishing_verdict === 'phishing'
      ? { ...content, html: highlightMismatchedLinks(content.html) }
      : content
  }, [expanded, view, displayed, allowRemote, cidUrls, publicConfig?.remote_image_proxy_enabled])

  return (
    <section
      className={cn(
        'shrink-0 overflow-hidden rounded-lg border bg-card',
        isSpam ? 'border-amber-500/70 ring-1 ring-amber-500/20' : 'border-border',
      )}
    >
      <button
        type="button"
        className={cn(
          'flex w-full items-start gap-3 px-4 py-3.5 text-left',
          isSpam && 'bg-amber-500/[0.07]',
        )}
        onClick={() => setExpanded((v) => !v)}
      >
        <span
          className="flex size-9 shrink-0 items-center justify-center rounded-full text-[14px] font-bold text-white"
          style={{ backgroundColor: color }}
        >
          {accountInitials(sender.name)}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="relative">
              <span
                role="button"
                tabIndex={0}
                onClick={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                  setContactOpen((value) => !value)
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault()
                    event.stopPropagation()
                    setContactOpen((value) => !value)
                  }
                }}
                className="block truncate text-[14px] font-bold text-foreground hover:underline"
              >
                {sender.name}
              </span>
              {contactOpen && (
                <span className="absolute left-0 top-7 z-50 flex w-72 flex-col gap-2 rounded-md border border-border bg-popover p-3 text-sm shadow-lg">
                  <span className="flex items-center gap-2">
                    <span
                      className="flex size-9 shrink-0 items-center justify-center rounded-full text-[12px] font-bold uppercase text-white"
                      style={{ backgroundColor: accountColor(sender.email) }}
                    >
                      {accountInitials(senderContact?.display_name ?? sender.name)}
                    </span>
                    <span className="min-w-0">
                      <span className="block truncate font-semibold text-foreground">
                        {senderContact?.display_name ?? sender.name}
                      </span>
                      <span className="block truncate font-mono text-xs text-muted-foreground">{sender.email}</span>
                    </span>
                  </span>
                  {senderContact?.org && <span className="text-xs text-muted-foreground">{senderContact.org}</span>}
                  {senderContact?.phones[0]?.value && (
                    <span className="font-mono text-xs text-muted-foreground">{senderContact.phones[0].value}</span>
                  )}
                  <Button
                    type="button"
                    size="sm"
                    onClick={(event) => {
                      event.stopPropagation()
                      openCompose({ mode: 'new', to: [sender.email] })
                    }}
                  >
                    <Mail className="size-4" />
                    {t('action.sendEmail')}
                  </Button>
                </span>
              )}
            </span>
            {senderDomain && (
              <span className="truncate font-mono text-[12px] text-muted-foreground">({senderDomain})</span>
            )}
            <span className="truncate font-mono text-[12px] text-muted-foreground">{sender.email}</span>
            {isSpam && (
              <span className="inline-flex shrink-0 items-center gap-1 rounded bg-amber-500/20 px-1.5 py-0.5 text-[10.5px] font-bold text-amber-800 dark:text-amber-200">
                <ShieldAlert className="size-3" aria-hidden="true" />
                {t('mail.spamSuspected')}
              </span>
            )}
            <span className="ml-auto shrink-0 text-[12px] text-muted-foreground">
              {formatDate(message.internal_date)}
            </span>
          </div>
          <div className="mt-0.5 truncate text-[12.5px] text-muted-foreground">
            {expanded ? `To ${message.to_addrs}` : message.snippet}
          </div>
        </div>
        <ChevronDown
          className={cn('mt-1 size-4 shrink-0 text-muted-foreground transition-transform', expanded && 'rotate-180')}
        />
      </button>

      {expanded && (
        <div className="px-4 pb-4">
          {isSpam && (
            <SpamBanner
              onNotSpam={() => notSpam.mutate(displayed.id, { onSuccess: () => onDeleted?.() })}
              pending={notSpam.isPending}
            />
          )}
          <PhishingBanner message={displayed} />
          <div className="pl-[3.25rem]">
            <PgpMessagePanel message={displayed} />
          </div>
          <div className="pl-[3.25rem]">
            <RsvpCard messageId={displayed.id} />
          </div>
          {/* view switcher + actions */}
          <div className="mb-3.5 flex flex-wrap items-center gap-2 pl-[3.25rem]">
            <div className="inline-flex gap-0.5 rounded-lg bg-secondary p-0.5">
              {VIEW_TABS.map(({ id, tkey, icon: Icon }) => {
                const on = view === id
                return (
                  <button
                    key={id}
                    onClick={() => setView(id)}
                    className={cn(
                      'inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-[12px] font-semibold transition-colors',
                      on ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-secondary-foreground',
                    )}
                  >
                    <Icon className="size-3.5" />
                    {t(tkey)}
                  </button>
                )
              })}
            </div>
            <div className="ml-auto inline-flex gap-1.5">
              {view === 'headers' && <CopyButton text={headers.map(([k, v]) => `${k}: ${v}`).join('\n')} />}
              {view === 'raw' && (
                <>
                  <CopyButton text={raw} />
                  <button
                    onClick={() => downloadEml(raw, message.subject)}
                    className="inline-flex h-7 items-center gap-1.5 rounded-md border border-border bg-card px-2.5 text-[12px] font-semibold text-secondary-foreground hover:bg-secondary"
                  >
                    <Download className="size-3.5 text-muted-foreground" />
                    {t('action.download')}
                  </button>
                </>
              )}
            </div>
          </div>

          {/* content */}
          <div className="pl-[3.25rem]">
            {visibleAttachments.length > 0 && <AttachmentList attachments={visibleAttachments} />}
            {view === 'html' && htmlContent.blocked && (
              <div className="mb-3 flex flex-wrap items-center gap-2 rounded-md border border-border bg-secondary/40 px-3 py-2 text-[12.5px] text-muted-foreground">
                <ImageOff className="size-4 shrink-0" aria-hidden="true" />
                <span>{t('mail.imagesBlocked')}</span>
                <span className="ml-auto inline-flex gap-1.5">
                  <button
                    type="button"
                    onClick={() => setShowRemoteOnce(true)}
                    className="rounded-md border border-border bg-card px-2.5 py-1 text-[12px] font-semibold text-secondary-foreground hover:bg-secondary"
                  >
                    {t('mail.loadImages')}
                  </button>
                  <button
                    type="button"
                    onClick={() => addAllowedSender.mutate(senderEmail)}
                    disabled={addAllowedSender.isPending}
                    className="rounded-md border border-border bg-card px-2.5 py-1 text-[12px] font-semibold text-secondary-foreground hover:bg-secondary disabled:opacity-50"
                  >
                    {t('mail.allowSender')}
                  </button>
                </span>
              </div>
            )}
            {isLoading && bodySpinnerExpired && (
              <div
                className="mb-3 flex items-center gap-2 rounded-md border border-border bg-secondary/40 px-3 py-2 text-[12.5px] text-muted-foreground"
                role="status"
                aria-live="polite"
              >
                <RefreshCw className="size-4 shrink-0 motion-safe:animate-spin motion-reduce:animate-none" aria-hidden="true" />
                <span>{t('mail.loadingBody')}</span>
              </div>
            )}
            {isLoading && !bodySpinnerExpired ? (
              <div
                className="flex h-24 items-center justify-center gap-2 text-sm text-muted-foreground"
                role="status"
                aria-live="polite"
              >
                <RefreshCw className="size-4 motion-safe:animate-spin motion-reduce:animate-none" aria-hidden="true" />
                {t('mail.loadingBody')}
              </div>
            ) : offlineMissing ? (
              <div className="rounded-md border border-border bg-secondary/40 p-4 text-sm text-muted-foreground">
                {t('mail.bodyOffline')}
              </div>
            ) : view === 'html' ? (
              <HtmlBody html={htmlContent.html} title={message.subject} />
            ) : view === 'text' ? (
              <pre className="whitespace-pre-wrap break-words rounded-lg border border-border bg-background p-4 font-mono text-[12.5px] leading-relaxed text-foreground">
                {messagePlain(displayed)}
              </pre>
            ) : view === 'headers' ? (
              <div className="overflow-hidden rounded-lg border border-border">
                {headers.map(([k, v], i) => (
                  <div
                    key={k + i}
                    className={cn('flex gap-3 px-3 py-2', i % 2 ? 'bg-card' : 'bg-background')}
                  >
                    <span className="w-40 shrink-0 break-words font-mono text-[11.5px] font-bold text-secondary-foreground">
                      {k}
                    </span>
                    <span className="min-w-0 flex-1 whitespace-pre-wrap break-words font-mono text-[11.5px] text-muted-foreground">
                      {v}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <pre className="max-h-[420px] overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border bg-background p-4 font-mono text-[11.5px] leading-relaxed text-secondary-foreground">
                {raw}
              </pre>
            )}
          </div>

          {/* per-message actions */}
          <div className="mt-3 flex items-center gap-1 pl-[3.25rem]">
            {!displayed.is_local_draft ? (
              <>
                <ToolButton
                  label={displayed.is_flagged ? t('action.unstar') : t('action.star')}
                  onClick={() => toggleFlag.mutate({ id: displayed.id, is_flagged: !displayed.is_flagged })}
                  disabled={toggleFlag.isPending}
                >
                  <Star className={cn('size-4', displayed.is_flagged && 'fill-primary text-primary')} />
                </ToolButton>
                <ToolButton
                  label={t('action.reply')}
                  onClick={() => openCompose({ mode: 'reply', sourceMessage: displayed })}
                  disabled={isLoading || !hasCompleteBody(detail)}
                >
                  <Reply className="size-4" />
                </ToolButton>
                <ToolButton
                  label={t('action.forward')}
                  onClick={() => openCompose({ mode: 'forward', sourceMessage: displayed })}
                  disabled={isLoading || !hasCompleteBody(detail)}
                >
                  <Forward className="size-4" />
                </ToolButton>
                <ToolButton
                  label={t('action.reanalyse')}
                  onClick={() => reanalyseMessage.mutate(displayed.id)}
                  disabled={reanalyseMessage.isPending}
                >
                  <ShieldAlert className="size-4" />
                </ToolButton>
              </>
            ) : null}
            {isSpam && (
              <ToolButton
                label={t('action.notSpam')}
                onClick={() => notSpam.mutate(displayed.id, { onSuccess: () => onDeleted?.() })}
                disabled={notSpam.isPending}
              >
                <ShieldCheck className="size-4" />
              </ToolButton>
            )}
            <ToolButton
              label={t('action.delete')}
              danger
              onClick={() => deleteMessage.mutate(displayed.id, { onSuccess: () => onDeleted?.() })}
              disabled={deleteMessage.isPending}
            >
              <Trash2 className="size-4" />
            </ToolButton>
          </div>
        </div>
      )}
    </section>
  )
}

function isSpamMessage(message: Message) {
  return message.folder_type === 'SPAM' || message.folder_type === 'JUNK'
}

function hasCompleteBody(message: Message | undefined): boolean {
  return Boolean(message && (message.body_available === true || message.body_html != null || message.body_text != null))
}

function RsvpCard({ messageId }: { messageId: string }) {
  const { t } = useTranslation()
  const { data: invitations = [] } = useMeetingInvitations(messageId)
  const rsvp = useRsvpInvitation()
  if (!invitations.length) return null
  const invitation = invitations[0]
  const attendees = parseAttendees(invitation.attendees)
  return (
    <div className="mb-3.5 rounded-lg border border-[#2563eb]/30 bg-[#2563eb]/5 p-3">
      <div className="flex flex-wrap items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="truncate text-[14px] font-bold">{invitation.summary || t('calendar.invitation')}</div>
          <div className="mt-1 text-[12.5px] text-muted-foreground">
            {invitation.start_dt ? new Date(invitation.start_dt).toLocaleString() : ''}
            {invitation.organizer_email ? ` · ${invitation.organizer_email}` : ''}
          </div>
          {attendees.length > 0 && (
            <div className="mt-1 truncate text-[12px] text-muted-foreground">
              {attendees.map((a) => `${a.email}${a.partstat ? ` (${a.partstat})` : ''}`).join(', ')}
            </div>
          )}
        </div>
        <span className="rounded-full bg-card px-2.5 py-1 text-[11px] font-bold text-secondary-foreground">
          {invitation.user_rsvp_status}
        </span>
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        {invitation.ms_teams_url && (
          <a className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-card px-3 text-[12px] font-semibold hover:bg-secondary" href={invitation.ms_teams_url} target="_blank" rel="noreferrer">
            {t('calendar.joinTeams')}
          </a>
        )}
        <button className="inline-flex h-8 items-center rounded-md bg-[#2563eb] px-3 text-[12px] font-semibold text-white disabled:opacity-60" disabled={rsvp.isPending} onClick={() => rsvp.mutate({ id: invitation.id, response: 'accepted' })}>
          {t('calendar.accept')}
        </button>
        <button className="inline-flex h-8 items-center rounded-md border border-border bg-card px-3 text-[12px] font-semibold hover:bg-secondary disabled:opacity-60" disabled={rsvp.isPending} onClick={() => rsvp.mutate({ id: invitation.id, response: 'tentative' })}>
          {t('calendar.tentative')}
        </button>
        <button className="inline-flex h-8 items-center rounded-md border border-border bg-card px-3 text-[12px] font-semibold text-destructive hover:bg-secondary disabled:opacity-60" disabled={rsvp.isPending} onClick={() => rsvp.mutate({ id: invitation.id, response: 'declined' })}>
          {t('calendar.decline')}
        </button>
        <button className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-card px-3 text-[12px] font-semibold hover:bg-secondary" onClick={() => downloadIcs(invitation)}>
          <Download className="size-3.5" />
          {t('calendar.downloadIcs')}
        </button>
      </div>
    </div>
  )
}

function parseAttendees(raw: string): Array<{ email: string; partstat?: string }> {
  try {
    const parsed = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed : []
  } catch {
    return []
  }
}

function downloadIcs(invitation: MeetingInvitation) {
  const blob = new Blob([invitation.raw_ical], { type: 'text/calendar;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `${invitation.uid || 'invitation'}.ics`
  a.click()
  URL.revokeObjectURL(url)
}

type RgbColor = { red: number; green: number; blue: number; alpha: number }

function parseRgbColor(value: string): RgbColor | null {
  const channels = value.match(/[\d.]+/g)?.map(Number)
  if (!channels || channels.length < 3) return null
  return {
    red: channels[0],
    green: channels[1],
    blue: channels[2],
    alpha: channels[3] ?? 1,
  }
}

function contrastRatio(foreground: RgbColor, background: RgbColor): number {
  const composite = (channel: number, backgroundChannel: number) =>
    channel * foreground.alpha + backgroundChannel * (1 - foreground.alpha)
  const luminance = (color: RgbColor) => {
    const linear = [color.red, color.green, color.blue].map((channel) => {
      const value = channel / 255
      return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
    })
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
  }
  const compositedForeground = {
    red: composite(foreground.red, background.red),
    green: composite(foreground.green, background.green),
    blue: composite(foreground.blue, background.blue),
    alpha: 1,
  }
  const lighter = Math.max(luminance(compositedForeground), luminance(background))
  const darker = Math.min(luminance(compositedForeground), luminance(background))
  return (lighter + 0.05) / (darker + 0.05)
}

function hasPaintedBackground(style: CSSStyleDeclaration): boolean {
  const color = parseRgbColor(style.backgroundColor)
  return style.backgroundImage !== 'none' || Boolean(color && color.alpha > 0)
}

type EmailColorProperty = 'background' | 'color'

function styleDefinesColorProperty(style: CSSStyleDeclaration, property: EmailColorProperty): boolean {
  if (property === 'color') return Boolean(style.getPropertyValue('color'))
  return Boolean(style.getPropertyValue('background-color') || style.getPropertyValue('background-image'))
}

function rulesDefineColorProperty(
  rules: CSSRuleList,
  elements: Element[],
  property: EmailColorProperty,
  view: Window,
): boolean {
  for (const rule of rules) {
    if (rule.type === CSSRule.MEDIA_RULE && !view.matchMedia((rule as CSSMediaRule).conditionText).matches) continue
    if (rule.type === CSSRule.STYLE_RULE) {
      const styleRule = rule as CSSStyleRule
      let matches = false
      try {
        matches = elements.some((element) => element.matches(styleRule.selectorText))
      } catch {
        // Ignore selectors unsupported by the current browser.
      }
      if (matches && styleDefinesColorProperty(styleRule.style, property)) return true
    }
    const nestedRules = (rule as CSSRule & { cssRules?: CSSRuleList }).cssRules
    if (nestedRules && rulesDefineColorProperty(nestedRules, elements, property, view)) return true
  }
  return false
}

function emailDefinesColorProperty(
  doc: Document,
  elements: [HTMLElement, HTMLElement],
  property: EmailColorProperty,
): boolean {
  const [, body] = elements
  if (property === 'color') {
    if (body.hasAttribute('text')) return true
  } else if (elements.some((element) => element.hasAttribute('bgcolor') || element.hasAttribute('background'))) {
    return true
  }
  if (elements.some((element) => styleDefinesColorProperty(element.style, property))) return true

  for (const sheet of doc.styleSheets) {
    try {
      if (rulesDefineColorProperty(sheet.cssRules, elements, property, doc.defaultView!)) return true
    } catch {
      // A cross-origin stylesheet cannot be inspected. Treat its colours as
      // authored so the fallback never overrides rules we cannot verify.
      return true
    }
  }
  return false
}

function solidBackground(styles: CSSStyleDeclaration[]): RgbColor | null {
  let result: RgbColor = { red: 255, green: 255, blue: 255, alpha: 1 }
  let painted = false
  for (const style of [...styles].reverse()) {
    const color = parseRgbColor(style.backgroundColor)
    if (!color || color.alpha <= 0) continue
    result = {
      red: color.red * color.alpha + result.red * (1 - color.alpha),
      green: color.green * color.alpha + result.green * (1 - color.alpha),
      blue: color.blue * color.alpha + result.blue * (1 - color.alpha),
      alpha: 1,
    }
    painted = true
  }
  return painted ? result : null
}

function applyMissingEmailCanvas(doc: Document, iframe: HTMLIFrameElement) {
  const body = doc.body
  const root = doc.documentElement
  if (!body || !root || !doc.defaultView) return

  const bodyStyle = doc.defaultView.getComputedStyle(body)
  const rootStyle = doc.defaultView.getComputedStyle(root)
  const styles = [bodyStyle, rootStyle]
  iframe.style.backgroundColor = ''

  const elements: [HTMLElement, HTMLElement] = [root, body]
  const backgroundDefined =
    emailDefinesColorProperty(doc, elements, 'background') || styles.some(hasPaintedBackground)
  const colorDefined = emailDefinesColorProperty(doc, elements, 'color')
  if (backgroundDefined && colorDefined) return

  const light = { red: 255, green: 255, blue: 255, alpha: 1 }
  const dark = { red: 17, green: 24, blue: 39, alpha: 1 }

  if (!backgroundDefined) {
    const foreground = colorDefined ? parseRgbColor(bodyStyle.color) : dark
    const background =
      foreground && contrastRatio(foreground, light) < contrastRatio(foreground, dark) ? '#111827' : '#fff'
    root.style.backgroundColor = background
    iframe.style.backgroundColor = background
  }

  if (!colorDefined) {
    const background = backgroundDefined ? solidBackground(styles) : light
    const foreground =
      background && contrastRatio(light, background) > contrastRatio(dark, background) ? '#fff' : '#111827'
    body.style.color = foreground
  }
}

/**
 * Renders email HTML in a sandboxed iframe that auto-sizes to its content so the
 * whole message is visible and the reading pane (not the iframe) scrolls.
 *
 * `allow-same-origin` (without `allow-scripts`) lets the parent read the iframe
 * document's height; scripts still cannot run, so the email stays inert.
 * `allow-popups` + `allow-popups-to-escape-sandbox` let links open in a new,
 * unsandboxed tab — every link is forced to `target="_blank"` on load.
 */
function HtmlBody({ html, title }: { html: string; title: string }) {
  const ref = useRef<HTMLIFrameElement>(null)
  const [height, setHeight] = useState(300)

  useEffect(() => {
    const iframe = ref.current
    if (!iframe) return
    let observer: ResizeObserver | undefined
    let poll: number | undefined

    const measure = () => {
      const doc = iframe.contentDocument
      const body = doc?.body
      // The injected reset makes <body> size to content, so its height is exact.
      // documentElement.scrollHeight tends to overshoot by a few px (html-level
      // box), which would leave a white strip below the email.
      const fromBody = Math.max(body?.scrollHeight ?? 0, body?.offsetHeight ?? 0)
      return fromBody || doc?.documentElement?.scrollHeight || 0
    }
    const apply = () => {
      const h = measure()
      if (h) setHeight(h)
    }

    const onLoad = () => {
      const doc = iframe.contentDocument
      // Inject a reset INTO <head> (not before the doctype, which would trigger
      // quirks mode) so html/body size to content and images don't overflow.
      if (doc?.head) {
        const style = doc.createElement('style')
        // Reset html/body to content height; drop the inline-image baseline gap
        // (descender space below images) and hide 1×1 tracking pixels, both of
        // which otherwise add a stray line of height below the email.
        style.textContent =
          'html{height:auto!important;margin:0!important}' +
          // Small inset so text/images don't sit flush against the edge.
          'body{height:auto!important;margin:0!important;padding:8px!important;box-sizing:border-box!important}' +
          'img{max-width:100%;vertical-align:middle}' +
          'img[width="1"],img[height="1"]{display:none!important}'
        doc.head.appendChild(style)
        // Open every link in a new tab: <base> covers plain links, the per-link
        // pass overrides explicit target="_self" and strips window.opener access.
        const base = doc.createElement('base')
        base.target = '_blank'
        doc.head.appendChild(base)
        doc.querySelectorAll('a[href]').forEach((a) => {
          a.setAttribute('target', '_blank')
          a.setAttribute('rel', 'noopener noreferrer')
        })
        applyMissingEmailCanvas(doc, iframe)
      }
      apply()
      if (doc?.body) {
        observer = new ResizeObserver(apply)
        observer.observe(doc.body)
      }
      // Remote email images load late (and slowly) and grow the layout, which a
      // ResizeObserver can miss when the body box is pinned. Poll for the whole
      // budget — not early-stopping — and grow the height monotonically so a
      // briefly-stable hero image doesn't freeze the measurement before the rest
      // of the email loads.
      let best = 0
      let ticks = 0
      poll = window.setInterval(() => {
        const h = measure()
        if (h > best) {
          best = h
          setHeight(h)
        }
        ticks += 1
        if (ticks > 50) {
          // ~7.5s budget
          window.clearInterval(poll)
          poll = undefined
        }
      }, 150)
    }

    iframe.addEventListener('load', onLoad)
    if (iframe.contentDocument?.readyState === 'complete') onLoad()
    return () => {
      iframe.removeEventListener('load', onLoad)
      observer?.disconnect()
      if (poll) window.clearInterval(poll)
    }
  }, [html])

  return (
    <iframe
      ref={ref}
      title={title}
      sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"
      srcDoc={html}
      scrolling="no"
      style={{ height }}
      // Default while the message document determines its own canvas.
      className="block w-full rounded-md border border-border bg-white"
    />
  )
}

function highlightMismatchedLinks(html: string): string {
  const parser = new DOMParser()
  const doc = parser.parseFromString(html, 'text/html')
  doc.querySelectorAll<HTMLAnchorElement>('a[href]').forEach((anchor) => {
    const hrefDomain = linkDomain(anchor.href)
    const textDomain = visibleLinkDomain(anchor.textContent ?? '')
    if (!hrefDomain || !textDomain || sameDomainOrg(hrefDomain, textDomain)) return
    anchor.style.outline = '2px solid #dc2626'
    anchor.style.borderRadius = '4px'
    anchor.style.backgroundColor = 'rgba(220, 38, 38, 0.12)'
    anchor.title = `Displayed ${textDomain}, opens ${hrefDomain}`
  })
  return doc.documentElement.outerHTML
}

function visibleLinkDomain(value: string): string {
  const match = value.trim().match(/(?:https?:\/\/|www\.)([a-z0-9.-]+\.[a-z]{2,})/i)
  return match?.[1]?.toLowerCase() ?? ''
}

function linkDomain(value: string): string {
  try {
    return new URL(value).hostname.toLowerCase()
  } catch {
    return ''
  }
}

function sameDomainOrg(a: string, b: string): boolean {
  return registrableDomain(a) === registrableDomain(b)
}

function registrableDomain(domain: string): string {
  const parts = domain.split('.')
  return parts.length <= 2 ? domain : parts.slice(-2).join('.')
}

function CopyButton({ text }: { text: string }) {
  const { t } = useTranslation()
  const [done, setDone] = useState(false)
  return (
    <button
      onClick={() => {
        navigator.clipboard?.writeText(text).catch(() => {})
        setDone(true)
        setTimeout(() => setDone(false), 1400)
      }}
      className={cn(
        'inline-flex h-7 items-center gap-1.5 rounded-md border border-border bg-card px-2.5 text-[12px] font-semibold hover:bg-secondary',
        done ? 'text-[#16a34a]' : 'text-secondary-foreground',
      )}
    >
      {done ? <Check className="size-3.5 text-[#16a34a]" /> : <Copy className="size-3.5 text-muted-foreground" />}
      {done ? t('action.copied') : t('action.copy')}
    </button>
  )
}
