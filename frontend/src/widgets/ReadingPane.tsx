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
} from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { formatDate, parseFromAddr } from '@/shared/lib/format'
import { accountColor, accountInitials } from '@/shared/lib/avatar'
import {
  messageHeaders,
  messageHtml,
  messagePlain,
  messageRawEml,
  downloadEml,
} from '@/shared/lib/messageSource'
import { apiGetBlob } from '@/shared/api'
import { blockRemoteContent } from '@/shared/lib/remoteContent'
import {
  useArchiveThread,
  useDeleteThread,
  useMarkThreadRead,
  useMarkRead,
  useToggleFlag,
  useDeleteMessage,
  useMessage,
  useThread,
} from '@/shared/hooks/useMessages'
import { useAddAllowedImageSender, useImageAllowlist, useSettings } from '@/shared/hooks/useSettings'
import type { MailOutletContext } from '@/pages/MailLayout'
import type { Message } from '@/shared/types'

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
  const { openCompose } = useOutletContext<MailOutletContext>()
  const messages = useMemo(() => data?.messages ?? [], [data?.messages])
  const firstMessage = messages[0]

  const defaultExpandedId = useMemo(() => {
    const unread = [...messages].reverse().find((m) => !m.is_read)
    return unread?.id ?? messages.at(-1)?.id
  }, [messages])

  if (!threadId) return <ReadingPaneEmpty />
  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center bg-background text-sm text-muted-foreground">
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
  const color = accountColor(firstMessage.account_id)
  const lastMessage = messages.at(-1)!

  return (
    <article className="flex h-full min-h-0 flex-col overflow-y-auto bg-background">
      {/* toolbar — sticky so the actions stay reachable while the thread scrolls */}
      <div className="sticky top-0 z-10 flex shrink-0 items-center gap-2 border-b border-border bg-card px-5 py-3">
        <ToolButton
          label={t('action.archive')}
          onClick={() => archiveThread.mutate(threadId, { onSuccess: () => onThreadGone?.() })}
          disabled={archiveThread.isPending}
        >
          <Archive className="size-4" />
        </ToolButton>
        <ToolButton
          label={t('action.delete')}
          danger
          onClick={() => deleteThread.mutate(threadId, { onSuccess: () => onThreadGone?.() })}
          disabled={deleteThread.isPending}
        >
          <Trash2 className="size-4" />
        </ToolButton>
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

        <div className="mt-1.5 flex shrink-0 gap-2.5">
          <button
            onClick={() => openCompose({ mode: 'reply', sourceMessage: lastMessage })}
            className="inline-flex h-9 items-center gap-2 rounded-md bg-[#2563eb] px-4 text-[13px] font-semibold text-white shadow-sm transition-colors hover:bg-[#1d4ed8]"
          >
            <Reply className="size-[15px]" />
            {t('action.reply')}
          </button>
          <button
            onClick={() => openCompose({ mode: 'reply', sourceMessage: lastMessage })}
            className="inline-flex h-9 items-center gap-2 rounded-md border border-border bg-card px-4 text-[13px] font-semibold text-secondary-foreground transition-colors hover:bg-secondary"
          >
            <ReplyAll className="size-[15px]" />
            {t('action.replyAll')}
          </button>
          <button
            onClick={() => openCompose({ mode: 'forward', sourceMessage: lastMessage })}
            className="inline-flex h-9 items-center gap-2 rounded-md border border-border bg-card px-4 text-[13px] font-semibold text-secondary-foreground transition-colors hover:bg-secondary"
          >
            <Forward className="size-[15px]" />
            {t('action.forward')}
          </button>
        </div>
      </div>
    </article>
  )
}

function PhishingBanner({ message }: { message: Message }) {
  const { t } = useTranslation()
  const [expanded, setExpanded] = useState(false)
  const verdict = message.phishing_verdict
  if (verdict !== 'suspicious' && verdict !== 'phishing') return null
  const danger = verdict === 'phishing'
  const checks = message.phishing_checks ?? []

  return (
    <div
      className={cn(
        'mb-3.5 rounded-md border px-3 py-2.5 text-[12.5px]',
        danger
          ? 'border-red-500/50 bg-red-500/10 text-red-600 dark:text-red-300'
          : 'border-amber-500/50 bg-amber-500/10 text-amber-700 dark:text-amber-300',
      )}
    >
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center gap-2 text-left font-semibold"
      >
        <ShieldAlert className="size-4 shrink-0" aria-hidden="true" />
        <span>{danger ? t('mail.phishingLikely') : t('mail.phishingSuspicious')}</span>
        {checks.length > 0 && (
          <ChevronDown className={cn('ml-auto size-4 shrink-0 transition-transform', expanded && 'rotate-180')} />
        )}
      </button>
      {expanded && checks.length > 0 && (
        <ul className="mt-2 flex list-disc flex-col gap-1 pl-9">
          {checks.map((check, i) => (
            <li key={check.id + i}>
              {check.detail} <span className="opacity-70">(+{check.points})</span>
            </li>
          ))}
        </ul>
      )}
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
  const [expanded, setExpanded] = useState(defaultExpanded)
  const [view, setView] = useState<SourceView>('html')
  const sender = parseFromAddr(message.from_addr)
  const { data: detail, isLoading } = useMessage(expanded ? message.id : '')
  const markRead = useMarkRead()
  const toggleFlag = useToggleFlag()
  const deleteMessage = useDeleteMessage()
  const { data: settings } = useSettings()
  const { data: imageAllowlist } = useImageAllowlist()
  const addAllowedSender = useAddAllowedImageSender()
  const [showRemoteOnce, setShowRemoteOnce] = useState(false)
  const [cidUrls, setCidUrls] = useState<Record<string, string>>({})
  const displayed = detail ?? message

  // Resolve inline attachments: the HTML references them as cid:<Content-ID>,
  // which the browser can't load — fetch each one (authenticated) and swap in
  // an object URL.
  const inlineAttachments = useMemo(
    () => (displayed.attachments ?? []).filter((a) => a.content_id),
    [displayed.attachments],
  )
  useEffect(() => {
    if (!expanded || !inlineAttachments.length) return
    let cancelled = false
    const urls: string[] = []
    Promise.all(
      inlineAttachments.map(async (att) => {
        const blob = await apiGetBlob(`/attachments/${att.id}`)
        const url = URL.createObjectURL(blob)
        urls.push(url)
        return [att.content_id as string, url] as const
      }),
    )
      .then((entries) => {
        if (!cancelled) setCidUrls(Object.fromEntries(entries))
      })
      .catch(() => {})
    return () => {
      cancelled = true
      urls.forEach((url) => URL.revokeObjectURL(url))
    }
  }, [expanded, inlineAttachments])
  const bodyLoaded = Boolean(displayed.body_html || displayed.body_text || displayed.snippet)
  const offlineMissing = !navigator.onLine && displayed.body_available === false
  const color = accountColor(message.account_id)

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
    let rawHtml = displayed.body_html ?? messageHtml(displayed)
    for (const [cid, url] of Object.entries(cidUrls)) {
      rawHtml = rawHtml.split(`cid:${cid}`).join(url)
    }
    return allowRemote ? { html: rawHtml, blocked: false } : blockRemoteContent(rawHtml)
  }, [expanded, view, displayed, allowRemote, cidUrls])

  return (
    <section className="shrink-0 overflow-hidden rounded-lg border border-border bg-card">
      <button
        type="button"
        className="flex w-full items-start gap-3 px-4 py-3.5 text-left"
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
            <span className="truncate text-[14px] font-bold text-foreground">{sender.name}</span>
            <span className="truncate font-mono text-[12px] text-muted-foreground">{sender.email}</span>
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
          <PhishingBanner message={displayed} />
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
            {isLoading ? (
              <div className="flex h-24 items-center justify-center text-sm text-muted-foreground">
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
            <ToolButton
              label={displayed.is_flagged ? t('action.unstar') : t('action.star')}
              onClick={() => toggleFlag.mutate({ id: displayed.id, is_flagged: !displayed.is_flagged })}
              disabled={toggleFlag.isPending}
            >
              <Star className={cn('size-4', displayed.is_flagged && 'fill-primary text-primary')} />
            </ToolButton>
            <ToolButton label={t('action.reply')} onClick={() => openCompose({ mode: 'reply', sourceMessage: displayed })}>
              <Reply className="size-4" />
            </ToolButton>
            <ToolButton label={t('action.forward')} onClick={() => openCompose({ mode: 'forward', sourceMessage: displayed })}>
              <Forward className="size-4" />
            </ToolButton>
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
      // Background matches the reading pane (not white) so any few-px overshoot
      // below the email body blends in across themes instead of showing a strip.
      className="block w-full rounded-md border border-border bg-background"
    />
  )
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
