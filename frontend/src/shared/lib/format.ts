export function formatDate(dateStr: string | null | undefined): string {
  if (!dateStr) return ''
  const date = new Date(dateStr)
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  const days = Math.floor(diff / 86400_000)

  if (days === 0) {
    return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
  }
  if (days < 7) {
    return date.toLocaleDateString(undefined, { weekday: 'short' })
  }
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
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

export function parseFromAddr(addr: string): { name: string; email: string } {
  const match = addr.match(/^(.+?)\s*<(.+)>$/)
  if (match) return { name: match[1].trim(), email: match[2].trim() }
  return { name: addr, email: addr }
}

export function listIdToName(listId: string | null | undefined): string | null {
  if (!listId) return null
  const match = listId.match(/^([^<]+)/)
  return match ? match[1].trim().replace(/\.$/, '') : listId
}
