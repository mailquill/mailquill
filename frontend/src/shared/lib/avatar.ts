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

export function avatarColor(name: string): string {
  return PALETTE[hashName(name) % PALETTE.length]
}

export function avatarInitial(name: string): string {
  const trimmed = name.trim()
  if (!trimmed) return '?'
  return trimmed[0].toUpperCase()
}
