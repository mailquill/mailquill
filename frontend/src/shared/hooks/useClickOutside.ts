import { useEffect, useRef } from 'react'

/**
 * Close a popover/menu when the user clicks outside it or presses Escape.
 * Returns a ref to attach to the popover's outermost element.
 */
export function useClickOutside<T extends HTMLElement>(onClose: () => void, active = true) {
  const ref = useRef<T | null>(null)

  useEffect(() => {
    if (!active) return
    const onPointer = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) onClose()
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('mousedown', onPointer)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', onPointer)
      document.removeEventListener('keydown', onKey)
    }
  }, [onClose, active])

  return ref
}
