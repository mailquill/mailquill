import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Info, ChevronRight, ChevronDown, ArrowUpRight } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { useClickOutside } from '@/shared/hooks/useClickOutside'
import { APP_VERSION, CHANGELOG, type ChangeType } from '@/shared/changelog'

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

type Tab = 'changelog' | 'legal'

const CHANGE_BADGE: Record<ChangeType, string> = {
  new: 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400',
  improved: 'bg-blue-500/15 text-blue-600 dark:text-blue-400',
  fixed: 'bg-amber-500/15 text-amber-600 dark:text-amber-400',
}

export function SystemInfoMenu() {
  const { t, i18n } = useTranslation()
  const [open, setOpen] = useState(false)
  const [tab, setTab] = useState<Tab>('changelog')
  const ref = useClickOutside<HTMLDivElement>(() => setOpen(false), open)

  const changeLabel: Record<ChangeType, string> = {
    new: t('sysInfo.new'),
    improved: t('sysInfo.improved'),
    fixed: t('sysInfo.fixed'),
  }

  const formatDate = (iso: string) =>
    new Date(iso).toLocaleDateString(i18n.language, { day: 'numeric', month: 'long', year: 'numeric' })

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        title={t('sysInfo.title')}
        className={cn(
          'flex size-9 items-center justify-center rounded-lg border transition-colors',
          open
            ? 'border-[#2563eb] bg-[var(--mq-row-open)] text-[#1d4ed8]'
            : 'border-border bg-card text-secondary-foreground hover:bg-secondary',
        )}
      >
        <Info className="size-[17px]" />
      </button>

      {open && (
        <div className="absolute right-0 top-[120%] z-50 flex max-h-[min(620px,80vh)] w-[420px] flex-col overflow-hidden rounded-xl border border-border bg-popover shadow-2xl">
          <div className="flex shrink-0 items-center gap-2 border-b border-secondary px-4 pb-3 pt-3.5">
            <span className="text-[15px] font-bold tracking-tight text-foreground">{t('sysInfo.title')}</span>
            <span className="ml-auto font-mono text-[12px] text-muted-foreground">
              Mailquill {APP_VERSION}
            </span>
          </div>

          {/* tabs */}
          <div className="flex shrink-0 gap-4 border-b border-secondary px-4">
            {(['changelog', 'legal'] as const).map((value) => {
              const on = tab === value
              return (
                <button
                  key={value}
                  onClick={() => setTab(value)}
                  className={cn(
                    '-mb-px border-b-2 py-2.5 text-[13px] font-semibold transition-colors',
                    on
                      ? 'border-[#2563eb] text-[#1d4ed8]'
                      : 'border-transparent text-muted-foreground hover:text-secondary-foreground',
                  )}
                >
                  {value === 'changelog' ? t('sysInfo.changelog') : t('sysInfo.legal')}
                </button>
              )
            })}
          </div>

          <div className="flex-1 overflow-y-auto">
            {tab === 'changelog' ? (
              <div className="px-4 py-3.5">
                {CHANGELOG.map((release, ri) => {
                  const current = release.version === APP_VERSION
                  return (
                    <div key={release.version} className="relative pl-5">
                      {/* timeline rail */}
                      {ri < CHANGELOG.length - 1 && (
                        <span className="absolute bottom-0 left-[3px] top-3 w-px bg-border" />
                      )}
                      <span
                        className={cn(
                          'absolute left-0 top-[5px] size-[7px] rounded-full',
                          current ? 'bg-[#2563eb]' : 'bg-input',
                        )}
                      />
                      <div className="flex items-baseline gap-2 pb-1.5">
                        <span className="font-mono text-[13px] font-bold text-foreground">{release.version}</span>
                        {current && (
                          <span className="rounded-full bg-[var(--mq-row-open)] px-1.5 py-0.5 text-[10px] font-bold text-[#1d4ed8]">
                            {t('sysInfo.current')}
                          </span>
                        )}
                        <span className="ml-auto text-[11.5px] text-muted-foreground">{formatDate(release.date)}</span>
                      </div>
                      <ul className="flex flex-col gap-1.5 pb-5">
                        {release.entries.map((entry, ei) => (
                          <li key={ei} className="flex items-start gap-2">
                            <span
                              className={cn(
                                'mt-px shrink-0 rounded px-1.5 py-0.5 text-[9.5px] font-bold uppercase tracking-wide',
                                CHANGE_BADGE[entry.type],
                              )}
                            >
                              {changeLabel[entry.type]}
                            </span>
                            <span className="text-[12.5px] leading-snug text-secondary-foreground">{entry.text}</span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  )
                })}
              </div>
            ) : (
              <LegalTab />
            )}
          </div>
        </div>
      )}
    </div>
  )
}

function LegalTab() {
  const { t } = useTranslation()
  const [active, setActive] = useState<number | null>(null)

  const summary = useMemo(() => {
    const counts: Record<string, number> = {}
    OSS.forEach((d) => (counts[d.license] = (counts[d.license] ?? 0) + 1))
    return Object.entries(counts)
      .map(([k, v]) => `${v}× ${k}`)
      .join('  ·  ')
  }, [])

  return (
    <div className="flex flex-col">
      <p className="border-b border-secondary px-4 py-3 text-xs leading-relaxed text-muted-foreground">
        {t('topbar.licensesIntro')}
      </p>
      {OSS.map((d, i) => {
        const isOpen = active === i
        return (
          <div key={d.name} className="border-b border-secondary">
            <button
              onClick={() => setActive((a) => (a === i ? null : i))}
              className="flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors hover:bg-secondary"
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
              <div className="px-4 pb-3.5">
                <p className="mb-2.5 rounded-lg bg-secondary px-3 py-2.5 text-[12px] leading-relaxed text-secondary-foreground">
                  {LICENSE_TEXT[d.license]}
                </p>
                <a
                  href={d.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1.5 text-[12px] font-semibold text-[#2563eb] hover:underline"
                >
                  {t('topbar.viewProject')}
                  <ArrowUpRight className="size-3.5" />
                </a>
              </div>
            )}
          </div>
        )
      })}
      <div className="shrink-0 bg-secondary px-3 py-2.5 text-center text-[11.5px] font-semibold text-muted-foreground">
        {summary}
      </div>
    </div>
  )
}
