import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronRight, ChevronDown, Scale, ArrowUpRight } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { useClickOutside } from '@/shared/hooks/useClickOutside'

interface Dependency {
  name: string
  version: string
  license: keyof typeof LICENSE_TEXT
  copyright: string
  url: string
}

const OSS: Dependency[] = [
  { name: 'React', version: '19.x', license: 'MIT', copyright: '© Meta Platforms, Inc. and affiliates', url: 'https://github.com/facebook/react' },
  { name: 'React Router', version: '7.x', license: 'MIT', copyright: '© Remix Software, Inc.', url: 'https://github.com/remix-run/react-router' },
  { name: 'TanStack Query', version: '5.x', license: 'MIT', copyright: '© Tanner Linsley', url: 'https://github.com/TanStack/query' },
  { name: 'Zustand', version: '5.x', license: 'MIT', copyright: '© Paul Henschel', url: 'https://github.com/pmndrs/zustand' },
  { name: 'Tailwind CSS', version: '4.x', license: 'MIT', copyright: '© Tailwind Labs, Inc.', url: 'https://github.com/tailwindlabs/tailwindcss' },
  { name: 'Lucide', version: 'icons', license: 'ISC', copyright: '© Lucide Contributors', url: 'https://github.com/lucide-icons/lucide' },
  { name: 'Vite', version: '8.x', license: 'MIT', copyright: '© Evan You & Vite contributors', url: 'https://github.com/vitejs/vite' },
  { name: 'Noto Sans', version: '—', license: 'SIL OFL 1.1', copyright: '© The Noto Project Authors', url: 'https://github.com/notofonts/latin-greek-cyrillic' },
]

const LICENSE_TEXT = {
  MIT: 'Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, subject to the inclusion of the above copyright notice and this permission notice. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.',
  ISC: 'Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies. THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE.',
  'SIL OFL 1.1': 'This Font Software is licensed under the SIL Open Font License, Version 1.1. The fonts and derivatives may be bundled, embedded, redistributed and/or sold with any software provided that reserved font names are not used. The fonts are provided "AS IS", without warranty of any kind.',
}

export function LicensesMenu() {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState<number | null>(null)
  const ref = useClickOutside<HTMLDivElement>(() => {
    setOpen(false)
    setActive(null)
  }, open)

  const summary = useMemo(() => {
    const counts: Record<string, number> = {}
    OSS.forEach((d) => (counts[d.license] = (counts[d.license] ?? 0) + 1))
    return Object.entries(counts)
      .map(([k, v]) => `${v}× ${k}`)
      .join('  ·  ')
  }, [])

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        title={t('topbar.openSourceLicenses')}
        className={cn(
          'flex size-9 items-center justify-center rounded-lg border border-border text-secondary-foreground transition-colors hover:bg-secondary',
          open ? 'bg-secondary' : 'bg-card',
        )}
      >
        <Scale className="size-[17px]" />
      </button>

      {open && (
        <div className="absolute right-0 top-[120%] z-50 flex max-h-[min(560px,80vh)] w-[400px] flex-col overflow-hidden rounded-xl border border-border bg-popover shadow-2xl">
          <div className="shrink-0 border-b border-secondary px-4 pb-3 pt-3.5">
            <div className="flex items-center gap-2">
              <span className="text-[15px] font-bold tracking-tight text-foreground">{t('topbar.licenses')}</span>
              <span className="ml-auto rounded-full bg-secondary px-2 py-0.5 text-[11px] font-bold text-muted-foreground">
                {OSS.length}
              </span>
            </div>
            <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
              {t('topbar.licensesIntro')}
            </p>
          </div>

          <div className="flex-1 overflow-y-auto">
            {OSS.map((d, i) => {
              const isOpen = active === i
              return (
                <div key={d.name} className="border-b border-secondary">
                  <button
                    onClick={() => setActive((a) => (a === i ? null : i))}
                    className="flex w-full items-center gap-3 px-3.5 py-2.5 text-left transition-colors hover:bg-secondary"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-baseline gap-2">
                        <span className="text-[13.5px] font-semibold text-foreground">{d.name}</span>
                        {d.version !== '—' && (
                          <span className="font-mono text-[11px] text-muted-foreground">{d.version}</span>
                        )}
                      </div>
                      <div className="mt-0.5 truncate text-[11.5px] text-muted-foreground">{d.copyright}</div>
                    </div>
                    <span className="shrink-0 rounded-md bg-[var(--mq-row-open)] px-2 py-0.5 text-[11px] font-bold text-[#1d4ed8]">
                      {d.license}
                    </span>
                    {isOpen ? (
                      <ChevronDown className="size-4 text-muted-foreground" />
                    ) : (
                      <ChevronRight className="size-4 text-muted-foreground" />
                    )}
                  </button>
                  {isOpen && (
                    <div className="px-3.5 pb-3.5">
                      <p className="mb-2.5 rounded-lg bg-secondary px-3 py-2.5 text-[12px] leading-relaxed text-secondary-foreground">
                        {LICENSE_TEXT[d.license]}
                      </p>
                      <a
                        href={d.url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="inline-flex items-center gap-1.5 text-[12px] font-semibold text-[#2563eb] hover:underline"
                      >
                        View project
                        <ArrowUpRight className="size-3.5" />
                      </a>
                    </div>
                  )}
                </div>
              )
            })}
          </div>

          <div className="shrink-0 border-t border-secondary bg-secondary px-3 py-2.5 text-center text-[11.5px] font-semibold text-muted-foreground">
            {summary}
          </div>
        </div>
      )}
    </div>
  )
}
