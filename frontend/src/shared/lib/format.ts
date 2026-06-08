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
