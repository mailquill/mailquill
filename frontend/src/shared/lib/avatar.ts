const PALETTE = [
  '#5B8DEF', '#9B59B6', '#E74C3C', '#1ABC9C',
  '#F39C12', '#2ECC71', '#3498DB', '#E91E63',
  '#FF5722', '#607D8B', '#795548', '#009688',
]

function hashName(name: string): number {
  let h = 0
  for (let i = 0; i < name.length; i++) {
    h = (h * 31 + name.charCodeAt(i)) >>> 0
  }
  return h
}

/**
 * Pick a stable background colour for a sender avatar.
 * @param name - Display name (or address) the avatar stands for; hashed to choose the colour.
 * @returns A hex colour from the avatar palette; the same name always yields the same colour.
 */
export function avatarColor(name: string): string {
  return PALETTE[hashName(name) % PALETTE.length]
}

/**
 * Single upper-case initial shown inside a sender avatar.
 * @param name - Display name or address; surrounding whitespace is ignored.
 * @returns The first character upper-cased, or `?` when the name is blank.
 */
export function avatarInitial(name: string): string {
  const trimmed = name.trim()
  if (!trimmed) return '?'
  return trimmed[0].toUpperCase()
}

// Distinct, saturated colours used to give each account a stable identity
// (left stripe, sidebar badge, account dot) — keyed off the account id so a
// given mailbox always reads with the same colour everywhere.
const ACCOUNT_PALETTE = [
  '#2563EB', '#DB2777', '#7C3AED', '#059669',
  '#EA580C', '#0891B2', '#CA8A04', '#DC2626',
]

/**
 * Stable identity colour for an account (left stripe, sidebar badge, account dot).
 * @param id - Account id; hashed so the same mailbox reads with the same colour everywhere.
 * @returns A hex colour from the account palette.
 */
export function accountColor(id: string): string {
  return ACCOUNT_PALETTE[hashName(id) % ACCOUNT_PALETTE.length]
}

/**
 * Colour to draw an account with: the user-chosen colour when one is set, else the hash-derived one.
 * @param account - Account id plus its optional user-chosen colour.
 * @returns A hex colour string.
 */
export function resolveAccountColor(account: { id: string; color?: string | null }): string {
  return account.color ?? accountColor(account.id)
}

/**
 * Up to two upper-case initials for an account badge: the first letters of the first two words, else the first two characters.
 * @param name - Account display name.
 * @returns One or two upper-case characters, or `?` when the name is blank.
 */
export function accountInitials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase()
  return name.trim().slice(0, 2).toUpperCase() || '?'
}
