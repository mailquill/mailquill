import { avatarColor, avatarInitial } from '@/shared/lib/avatar'

interface AvatarProps {
  name: string
  size?: number
  className?: string
}

export function Avatar({ name, size = 32, className }: AvatarProps) {
  const bg = avatarColor(name)
  const initial = avatarInitial(name)
  return (
    <span
      className={className}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        width: size,
        height: size,
        borderRadius: '50%',
        backgroundColor: bg,
        color: '#fff',
        fontSize: size * 0.45,
        fontWeight: 600,
        flexShrink: 0,
        userSelect: 'none',
      }}
      aria-hidden="true"
    >
      {initial}
    </span>
  )
}
