import { CircleCheckIcon, Loader2Icon, OctagonXIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Toaster } from 'sonner'

/**
 * Render accessible application notifications in the top-right corner.
 * @returns The global Sonner notification region.
 */
export function ToastRegion() {
  const { t } = useTranslation()

  return (
    <Toaster
      position="top-right"
      closeButton
      containerAriaLabel={t('notifications.regionLabel')}
      visibleToasts={4}
      icons={{
        loading: <Loader2Icon className="size-4 motion-safe:animate-spin motion-reduce:animate-none" aria-hidden="true" />,
        success: <CircleCheckIcon className="size-4 text-primary" aria-hidden="true" />,
        error: <OctagonXIcon className="size-4 text-destructive" aria-hidden="true" />,
      }}
      toastOptions={{
        closeButtonAriaLabel: t('notifications.dismiss'),
        classNames: {
          toast: 'border-border bg-popover text-popover-foreground shadow-lg',
          description: 'text-muted-foreground',
        },
      }}
      style={
        {
          '--normal-bg': 'var(--popover)',
          '--normal-text': 'var(--popover-foreground)',
          '--normal-border': 'var(--border)',
          '--border-radius': 'var(--radius)',
        } as React.CSSProperties
      }
    />
  )
}
