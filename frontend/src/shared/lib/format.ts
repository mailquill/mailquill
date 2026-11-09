// `toLocale*String` builds a fresh Intl formatter per call, and the message
// list formats one date per visible row on every render. Build the three
// formatters once (lazily, so module load stays cheap) and reuse them.
let dateFormatters: { time: Intl.DateTimeFormat; weekday: Intl.DateTimeFormat; monthDay: Intl.DateTimeFormat } | null =
  null

function formatters() {
  dateFormatters ??= {
    time: new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }),
    weekday: new Intl.DateTimeFormat(undefined, { weekday: 'short' }),
    monthDay: new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }),
  }
  return dateFormatters
}

/**
 * Compact date label for a message-list row, relative to now: the time of day for today, the short weekday within the last week, and "month day" otherwise.
 * @param dateStr - Date string parseable by `Date`, typically the message's ISO date.
 * @returns The formatted label in the browser locale, or an empty string for a missing or invalid date.
 */
export function formatDate(dateStr: string | null | undefined): string {
  if (!dateStr) return ''
  const date = new Date(dateStr)
  if (Number.isNaN(date.getTime())) return ''
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  const days = Math.floor(diff / 86400_000)

  if (days === 0) return formatters().time.format(date)
  if (days < 7) return formatters().weekday.format(date)
  return formatters().monthDay.format(date)
}

/** Localized relative time from now, e.g. "vor 13 Min." / "13 min ago". */
export function relativeFromNow(dateStr: string | null | undefined, locale?: string): string {
  if (!dateStr) return ''
  const diffMs = new Date(dateStr).getTime() - Date.now()
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })
  const sec = Math.round(diffMs / 1000)
  const abs = Math.abs(sec)
  if (abs < 60) return rtf.format(Math.round(sec), 'second')
  if (abs < 3600) return rtf.format(Math.round(sec / 60), 'minute')
  if (abs < 86400) return rtf.format(Math.round(sec / 3600), 'hour')
  return rtf.format(Math.round(sec / 86400), 'day')
}

/**
 * Split an RFC 5322 mailbox such as `Jane Doe <jane@example.org>` into display name and address.
 * @param addr - Raw `From`/`To` header value for a single mailbox.
 * @returns The trimmed name and address; a bare address yields the same value for both fields.
 */
export function parseFromAddr(addr: string): { name: string; email: string } {
  const match = addr.match(/^(.+?)\s*<(.+)>$/)
  if (match) return { name: match[1].trim(), email: match[2].trim() }
  return { name: addr, email: addr }
}

/**
 * Human-readable mailing-list name from a `List-Id` header (RFC 2919).
 *
 * Keeps the optional phrase in front of the bracketed id, stripping quotes and a trailing dot. Phrases that are opaque tokens (base64, long numeric or hex ids) are treated as noise.
 * @param listId - Raw `List-Id` header value.
 * @returns The list name, or `null` when the header is missing or carries no friendly name.
 */
export function listIdToName(listId: string | null | undefined): string | null {
  if (!listId) return null
  // RFC 2919: an optional human-readable phrase followed by <list-label.host>.
  // Keep the phrase; drop the bracketed id, surrounding quotes and trailing dot.
  const phrase = listId
    .replace(/<[^>]*>/, '')
    .trim()
    .replace(/^"(.*)"$/, '$1')
    .replace(/\.$/, '')
    .trim()
  if (!phrase) return null

  // Many ESPs (Sendinblue/Brevo, Civey, JACOB, …) put an opaque token in the
  // phrase instead of a real list name — base64 ids, long numeric or hex
  // strings. Those are noise, not a friendly tag, so show nothing.
  const isOpaqueToken =
    !/\s/.test(phrase) &&
    (/=$/.test(phrase) ||
      /^\d{7,}$/.test(phrase) ||
      /^[0-9a-f]{16,}$/i.test(phrase) ||
      (phrase.length >= 20 && /\d/.test(phrase) && /[a-z]/i.test(phrase)))
  return isOpaqueToken ? null : phrase
}
