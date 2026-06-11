import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Monitor, Sun, Moon, Check } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { useClickOutside } from '@/shared/hooks/useClickOutside'
import { useThemeStore, type ThemePref } from '@/shared/hooks/useTheme'

const OPTIONS: { value: ThemePref; labelKey: string; icon: LucideIcon }[] = [
  { value: 'system', labelKey: 'settings.system', icon: Monitor },
  { value: 'light', labelKey: 'settings.light', icon: Sun },
  { value: 'dark', labelKey: 'settings.dark', icon: Moon },
]

export function ThemeMenu() {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const pref = useThemeStore((s) => s.pref)
  const setPref = useThemeStore((s) => s.setPref)
  const ref = useClickOutside<HTMLDivElement>(() => setOpen(false), open)

  const current = OPTIONS.find((o) => o.value === pref) ?? OPTIONS[0]
  const CurrentIcon = current.icon

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        title={t('settings.appearance')}
        className="flex size-9 items-center justify-center rounded-lg border border-border bg-card text-secondary-foreground transition-colors hover:bg-secondary"
      >
        <CurrentIcon className="size-4" />
      </button>

      {open && (
        <div className="absolute right-0 top-[120%] z-50 w-44 rounded-xl border border-border bg-popover p-1.5 shadow-xl">
          <div className="px-2 pb-1.5 pt-1 text-[10px] font-bold uppercase tracking-[0.08em] text-muted-foreground">
            {t('settings.appearance')}
          </div>
          {OPTIONS.map(({ value, labelKey, icon: Icon }) => {
            const on = pref === value
            return (
              <button
                key={value}
                onClick={() => {
                  setPref(value)
                  setOpen(false)
                }}
                className={cn(
                  'flex w-full items-center gap-2.5 rounded-md px-2 py-2 text-left text-[13px] transition-colors',
                  on ? 'bg-[var(--mq-row-open)] font-semibold text-[#1d4ed8]' : 'hover:bg-secondary',
                )}
              >
                <Icon className={cn('size-4', on ? 'text-[#2563eb]' : 'text-secondary-foreground')} />
                <span className="flex-1">{t(labelKey)}</span>
                {on && <Check className="size-4 text-[#2563eb]" />}
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
