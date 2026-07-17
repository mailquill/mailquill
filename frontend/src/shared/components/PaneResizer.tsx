interface PaneResizerProps {
  width: number
  min: number
  max: number
  onChange: (width: number) => void
  /** Accessible name for the separator, e.g. "Resize sidebar". */
  label: string
}

/**
 * Vertical drag handle between two panes. Uses pointer capture so the drag
 * keeps working when the cursor crosses the reading-pane iframe (which would
 * otherwise swallow the move events).
 */
export function PaneResizer({ width, min, max, onChange, label }: PaneResizerProps) {
  function onPointerDown(e: React.PointerEvent<HTMLDivElement>) {
    e.preventDefault()
    const el = e.currentTarget
    el.setPointerCapture(e.pointerId)
    const startX = e.clientX
    const startWidth = width

    const move = (ev: PointerEvent) => {
      onChange(Math.min(max, Math.max(min, startWidth + ev.clientX - startX)))
    }
    const up = (ev: PointerEvent) => {
      if (el.hasPointerCapture(ev.pointerId)) el.releasePointerCapture(ev.pointerId)
      el.removeEventListener('pointermove', move)
      el.removeEventListener('pointerup', up)
      el.removeEventListener('pointercancel', up)
    }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', up)
    el.addEventListener('pointercancel', up)
  }

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      title={label}
      onPointerDown={onPointerDown}
      className="relative z-10 -ml-[3px] -mr-[3px] w-[6px] shrink-0 cursor-col-resize touch-none select-none transition-colors hover:bg-primary/30 active:bg-primary/40"
    />
  )
}
