import { useState } from 'react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import {
  Settings as Cog,
  Inbox,
  Send,
  Users,
  Calendar,
  Globe,
  RefreshCw,
  ShieldAlert,
  Check,
  ChevronDown,
  ChevronRight,
  PenLine,
  FileText,
} from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { Input } from '@/shared/components/ui/input'
import { Select } from '@/shared/components/ui/select'
import { Textarea } from '@/shared/components/ui/textarea'
import {
  ACCT_COLORS,
  SETTINGS_EMAIL_RE,
  serverDefaults,
  providerInfo,
  discoverServersAsync,
  type Security,
} from '@/shared/lib/serverDiscovery'

export interface AccountFormState {
  name: string
  short: string
  email: string
  initials: string
  initialsTouched?: boolean
  color: string
  composeFormat: 'rich' | 'plain'
  hasJunk: boolean
  signature: string
  imapHost: string
  imapPort: string
  imapSecurity: Security
  imapUser: string
  imapPass: string
  smtpHost: string
  smtpPort: string
  smtpSecurity: Security
  smtpUser: string
  smtpPass: string
  /** Add wizard: when false, IMAP credentials are reused for SMTP. */
  separateCreds?: boolean
  carddavUrl: string
  caldavUrl: string
}

export type AccountField = keyof AccountFormState

// ── primitives ──────────────────────────────────────────────────────────────

export function FieldLabel({ children, hint }: { children: ReactNode; hint?: ReactNode }) {
  return (
    <div className="mb-1.5">
      <span className="text-[12.5px] font-semibold text-foreground">{children}</span>
      {hint && <span className="ml-2 text-[12px] font-normal text-muted-foreground">{hint}</span>}
    </div>
  )
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T
  options: [T, string, typeof PenLine][]
  onChange: (v: T) => void
}) {
  return (
    <div className="inline-flex gap-0.5 rounded-[7px] border border-border bg-secondary p-0.5">
      {options.map(([val, label, Icon]) => {
        const on = value === val
        return (
          <button
            key={val}
            type="button"
            onClick={() => onChange(val)}
            className={cn(
              'flex h-[30px] items-center gap-1.5 rounded-[5px] px-3 text-[12.5px] font-semibold whitespace-nowrap transition-colors',
              on ? 'bg-card text-[#1d4ed8] shadow-sm' : 'text-secondary-foreground hover:text-foreground',
            )}
          >
            <Icon className="size-3.5" />
            {label}
          </button>
        )
      })}
    </div>
  )
}

function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      onClick={() => onChange(!on)}
      role="switch"
      aria-checked={on}
      className={cn(
        'relative h-6 w-[42px] shrink-0 rounded-full transition-colors',
        on ? 'bg-[#2563eb]' : 'bg-input',
      )}
    >
      <span
        className={cn(
          'absolute top-[3px] size-[18px] rounded-full bg-white shadow transition-[left]',
          on ? 'left-[21px]' : 'left-[3px]',
        )}
      />
    </button>
  )
}

export function Swatches({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <div className="flex flex-wrap gap-2">
      {ACCT_COLORS.map((c) => (
        <button
          key={c}
          type="button"
          title={c}
          onClick={() => onChange(c)}
          className={cn(
            'size-[26px] rounded-[7px] transition-shadow',
            value === c
              ? 'ring-2 ring-offset-2 ring-offset-card'
              : 'shadow-[inset_0_0_0_1px_rgba(15,23,42,0.1)]',
          )}
          style={{ backgroundColor: c, ...(value === c ? { '--tw-ring-color': c } as Record<string, string> : {}) }}
        />
      ))}
    </div>
  )
}

export function SrvField({ label, children, w }: { label: string; children: ReactNode; w?: string }) {
  return (
    <div className={cn('min-w-0', w)}>
      <div className="mb-1.5 text-[11.5px] font-semibold text-secondary-foreground">{label}</div>
      {children}
    </div>
  )
}

export function ServerGroup({ title, icon: Icon, children }: { title: string; icon: typeof Inbox; children: ReactNode }) {
  return (
    <div>
      <div className="mb-3 flex items-center gap-2">
        <Icon className="size-[15px] text-[#2563eb]" />
        <span className="text-[12.5px] font-bold text-foreground">{title}</span>
      </div>
      <div className="flex flex-col gap-3 pl-[23px]">{children}</div>
    </div>
  )
}

// ── shared account editor (add + edit) ────────────────────────────────────────

interface AccountFieldsProps {
  data: AccountFormState
  onField: (k: AccountField, v: AccountFormState[AccountField]) => void
  /** Launch a provider OAuth sign-in (full-page redirect). */
  onOauth?: (provider: 'google' | 'microsoft') => void
}

const secOptKeys: [Security, string][] = [
  ['ssl', 'settings.secSsl'],
  ['starttls', 'settings.secStarttls'],
  ['none', 'settings.secNone'],
]

export function AccountFields({ data, onField, onOauth }: AccountFieldsProps) {
  const { t } = useTranslation()
  const [srvOpen, setSrvOpen] = useState(false)
  const [detecting, setDetecting] = useState(false)
  const [detected, setDetected] = useState<{ source: string; provider: string } | null>(null)

  const emailInvalid = !!data.email && !SETTINGS_EMAIL_RE.test(data.email)
  const pInfo = providerInfo(data.email || 'name@example.com')
  const def = serverDefaults(data.email || 'name@example.com')
  const gv = (k: AccountField, fallback: string): string => {
    const v = data[k]
    return v !== undefined && v !== '' ? String(v) : fallback
  }

  async function runAutodetect() {
    if (!data.email || detecting) return
    setDetecting(true)
    setDetected(null)
    const d = await discoverServersAsync(data.email)
    onField('imapHost', d.imapHost)
    onField('imapPort', String(d.imapPort))
    onField('imapSecurity', d.imapSecurity)
    onField('imapUser', d.imapUser)
    onField('smtpHost', d.smtpHost)
    onField('smtpPort', String(d.smtpPort))
    onField('smtpSecurity', d.smtpSecurity)
    onField('smtpUser', d.smtpUser)
    onField('carddavUrl', d.carddavUrl)
    onField('caldavUrl', d.caldavUrl)
    setDetected({ source: d.source, provider: d.provider })
    setDetecting(false)
  }

  const detectLabel =
    detected &&
    (detected.source === 'provider'
      ? t('settings.detectedProvider', { provider: detected.provider })
      : detected.source === 'exchange'
        ? t('settings.detectedExchange')
        : t('settings.detectedDns'))

  const oauthProvider: 'google' | 'microsoft' | null =
    pInfo.oauth && /google/i.test(pInfo.name) ? 'google' : pInfo.oauth ? 'microsoft' : null

  return (
    <div className="grid grid-cols-1 gap-x-[18px] gap-y-4 md:grid-cols-2">
      <div>
        <FieldLabel>{t('settings.displayName')}</FieldLabel>
        <Input value={data.name} onChange={(e) => onField('name', e.target.value)} placeholder="Frank Gehann" />
      </div>
      <div>
        <FieldLabel hint={t('settings.sidebar')}>{t('settings.shortLabel')}</FieldLabel>
        <Input value={data.short} onChange={(e) => onField('short', e.target.value)} placeholder={t('settings.shortLabelExample')} />
      </div>
      <div>
        <FieldLabel>{t('settings.email')}</FieldLabel>
        <Input
          value={data.email}
          onChange={(e) => onField('email', e.target.value)}
          placeholder="name@example.com"
          className={cn('font-mono', emailInvalid && 'border-destructive focus-visible:ring-destructive')}
        />
        {emailInvalid && <div className="mt-1 text-[11.5px] text-destructive">{t('settings.emailInvalid')}</div>}
      </div>
      <div>
        <FieldLabel>{t('settings.initials')}</FieldLabel>
        <div className="flex items-center gap-2.5">
          <span
            className="flex size-[38px] shrink-0 items-center justify-center rounded-lg text-[12.5px] font-extrabold text-white"
            style={{ backgroundColor: data.color }}
          >
            {(data.initials || '?').slice(0, 2)}
          </span>
          <Input
            value={data.initials}
            onChange={(e) => onField('initials', e.target.value.toUpperCase().slice(0, 2))}
            placeholder="FG"
          />
        </div>
      </div>
      <div className="md:col-span-2">
        <FieldLabel>{t('settings.accountColor')}</FieldLabel>
        <Swatches value={data.color} onChange={(v) => onField('color', v)} />
      </div>
      <div>
        <FieldLabel>{t('settings.composeFormat')}</FieldLabel>
        <Segmented
          value={data.composeFormat}
          onChange={(v) => onField('composeFormat', v)}
          options={[
            ['rich', t('settings.richText'), PenLine],
            ['plain', t('settings.plainText'), FileText],
          ]}
        />
      </div>
      <div>
        <FieldLabel>{t('settings.serverFeatures')}</FieldLabel>
        <label className="flex h-[38px] cursor-pointer items-center gap-2.5">
          <Toggle on={data.hasJunk} onChange={(v) => onField('hasJunk', v)} />
          <span className="text-[13px] text-secondary-foreground">{t('settings.junkFolder')}</span>
        </label>
      </div>
      <div className="md:col-span-2">
        <FieldLabel>{t('settings.signature')}</FieldLabel>
        <Textarea
          value={data.signature}
          onChange={(e) => onField('signature', e.target.value)}
          placeholder={t('settings.sigPlaceholder')}
          rows={3}
        />
      </div>

      {/* IMAP / SMTP / CardDAV / CalDAV — collapsible */}
      <div className="md:col-span-2">
        <button
          type="button"
          onClick={() => setSrvOpen((o) => !o)}
          className={cn(
            'flex w-full items-center gap-2.5 border border-border bg-secondary px-3.5 py-[11px]',
            srvOpen ? 'rounded-t-lg' : 'rounded-lg',
          )}
        >
          <Cog className="size-4 text-secondary-foreground" />
          <span className="text-[13px] font-bold text-foreground">{t('settings.serverSettings')}</span>
          <span className="text-[11.5px] text-muted-foreground">{t('settings.serverSettingsHint')}</span>
          {srvOpen ? (
            <ChevronDown className="ml-auto size-[15px] text-muted-foreground" />
          ) : (
            <ChevronRight className="ml-auto size-[15px] text-muted-foreground" />
          )}
        </button>

        {srvOpen && (
          <div className="flex flex-col gap-5 rounded-b-lg border border-t-0 border-border p-4">
            {/* OAuth hint */}
            {oauthProvider && (
              <div className="flex items-center gap-3 rounded-lg border border-[#bfdbfe] bg-[var(--mq-row-open)] px-3.5 py-3">
                <ShieldAlert className="size-[18px] shrink-0 text-[#2563eb]" />
                <span className="flex-1 text-[12.5px] leading-snug text-secondary-foreground">{t('settings.oauthBanner')}</span>
                <button
                  type="button"
                  onClick={() => onOauth?.(oauthProvider)}
                  className="h-8 shrink-0 rounded-[7px] bg-[#2563eb] px-3.5 text-[12.5px] font-bold text-white hover:bg-[#1d4ed8]"
                >
                  {t('settings.oauthSignIn', { provider: pInfo.name })}
                </button>
              </div>
            )}

            {/* autodiscovery */}
            <div className="flex flex-wrap items-center gap-2.5">
              <button
                type="button"
                onClick={runAutodetect}
                disabled={!data.email || detecting}
                className={cn(
                  'inline-flex h-[34px] items-center gap-2 rounded-[7px] px-3.5 text-[12.5px] font-bold transition-colors',
                  !data.email || detecting
                    ? 'cursor-default bg-secondary text-muted-foreground'
                    : 'bg-[#2563eb] text-white hover:bg-[#1d4ed8]',
                )}
              >
                {detecting ? (
                  <RefreshCw className="size-[15px] animate-spin" />
                ) : (
                  <Globe className="size-[15px]" />
                )}
                {detecting ? t('settings.detecting') : t('settings.autoDetect')}
              </button>
              {detected && !detecting && (
                <span className="inline-flex items-center gap-1.5 rounded-full bg-[#16a34a]/10 px-2.5 py-1 text-[12px] font-semibold text-[#16a34a]">
                  <Check className="size-3.5" strokeWidth={2.5} />
                  {detectLabel}
                </span>
              )}
              <span className="ml-auto text-[11.5px] text-muted-foreground">{t('settings.autoHint')}</span>
            </div>

            <ServerGroup title={t('settings.imap')} icon={Inbox}>
              <div className="flex gap-3">
                <SrvField label={t('settings.host')} w="flex-[2.2]">
                  <Input className="font-mono" value={gv('imapHost', def.imapHost)} onChange={(e) => onField('imapHost', e.target.value)} placeholder={def.imapHost} />
                </SrvField>
                <SrvField label={t('settings.port')} w="flex-[0.7]">
                  <Input className="font-mono" value={gv('imapPort', String(def.imapPort))} onChange={(e) => onField('imapPort', e.target.value)} />
                </SrvField>
                <SrvField label={t('settings.security')} w="flex-[1.1]">
                  <Select value={data.imapSecurity} onChange={(e) => onField('imapSecurity', e.target.value as Security)}>
                    {secOptKeys.map(([v, k]) => (
                      <option key={v} value={v}>{t(k)}</option>
                    ))}
                  </Select>
                </SrvField>
              </div>
              <SrvField label={t('settings.username')}>
                <Input className="font-mono" value={gv('imapUser', def.imapUser)} onChange={(e) => onField('imapUser', e.target.value)} />
              </SrvField>
              <SrvField label={t('settings.password')}>
                <Input type="password" autoComplete="new-password" value={data.imapPass} onChange={(e) => onField('imapPass', e.target.value)} />
              </SrvField>
            </ServerGroup>

            <ServerGroup title={t('settings.smtp')} icon={Send}>
              <div className="flex gap-3">
                <SrvField label={t('settings.host')} w="flex-[2.2]">
                  <Input className="font-mono" value={gv('smtpHost', def.smtpHost)} onChange={(e) => onField('smtpHost', e.target.value)} placeholder={def.smtpHost} />
                </SrvField>
                <SrvField label={t('settings.port')} w="flex-[0.7]">
                  <Input className="font-mono" value={gv('smtpPort', String(def.smtpPort))} onChange={(e) => onField('smtpPort', e.target.value)} />
                </SrvField>
                <SrvField label={t('settings.security')} w="flex-[1.1]">
                  <Select value={data.smtpSecurity} onChange={(e) => onField('smtpSecurity', e.target.value as Security)}>
                    {secOptKeys.map(([v, k]) => (
                      <option key={v} value={v}>{t(k)}</option>
                    ))}
                  </Select>
                </SrvField>
              </div>
              <SrvField label={t('settings.username')}>
                <Input className="font-mono" value={gv('smtpUser', def.smtpUser)} onChange={(e) => onField('smtpUser', e.target.value)} />
              </SrvField>
              <SrvField label={t('settings.password')}>
                <Input type="password" autoComplete="new-password" value={data.smtpPass} onChange={(e) => onField('smtpPass', e.target.value)} />
              </SrvField>
            </ServerGroup>

            <ServerGroup title={t('settings.carddav')} icon={Users}>
              <SrvField label={t('settings.url')}>
                <Input className="font-mono" value={data.carddavUrl} onChange={(e) => onField('carddavUrl', e.target.value)} placeholder={def.carddavUrl} />
              </SrvField>
            </ServerGroup>

            <ServerGroup title={t('settings.caldav')} icon={Calendar}>
              <SrvField label={t('settings.url')}>
                <Input className="font-mono" value={data.caldavUrl} onChange={(e) => onField('caldavUrl', e.target.value)} placeholder={def.caldavUrl} />
              </SrvField>
            </ServerGroup>
          </div>
        )}
      </div>
    </div>
  )
}
