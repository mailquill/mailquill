import * as React from 'react'
import { cn } from '@/shared/lib/utils'

const DialogTitleIdContext = React.createContext<string | undefined>(undefined)

interface DialogProps {
  open: boolean
  onClose: () => void
  children: React.ReactNode
}

export function Dialog({ open, onClose, children }: DialogProps) {
  const dialogRef = React.useRef<HTMLDivElement>(null)
  const restoreFocusRef = React.useRef<HTMLElement | null>(null)
  const titleId = React.useId()

  React.useEffect(() => {
    if (!open) return
    restoreFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const dialog = dialogRef.current
    const focusable = dialog?.querySelector<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
    )
    ;(focusable ?? dialog)?.focus()
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose()
        return
      }
      if (e.key !== 'Tab' || !dialog) return
      const items = Array.from(dialog.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
      ))
      if (!items.length) {
        e.preventDefault()
        return
      }
      const first = items[0]
      const last = items[items.length - 1]
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }
    document.addEventListener('keydown', handler)
    return () => {
      document.removeEventListener('keydown', handler)
      restoreFocusRef.current?.focus()
    }
  }, [open, onClose])

  if (!open) return null
  return (
    <DialogTitleIdContext.Provider value={titleId}>
      <div className="fixed inset-0 z-50 flex items-center justify-center">
        <div className="fixed inset-0 bg-black/50" onClick={onClose} />
        <div
          ref={dialogRef}
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          tabIndex={-1}
          className="relative z-10 max-h-[90vh] overflow-y-auto outline-none"
        >
          {children}
        </div>
      </div>
    </DialogTitleIdContext.Provider>
  )
}

export function DialogContent({ className, children, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn('w-full max-w-lg rounded-xl bg-background p-6 shadow-xl', className)}
      {...props}
    >
      {children}
    </div>
  )
}

export function DialogHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('mb-4 space-y-1', className)} {...props} />
}

export function DialogTitle({ className, id, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  const generatedId = React.useContext(DialogTitleIdContext)
  return <h2 id={id ?? generatedId} className={cn('text-lg font-semibold', className)} {...props} />
}
