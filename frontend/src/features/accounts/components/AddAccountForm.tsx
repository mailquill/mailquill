import { Fragment, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  Check,
  ChevronRight,
  ChevronLeft,
  RefreshCw,
  ShieldAlert,
  AlertCircle,
  Inbox,
  Send,
  Users,
  Calendar,
} from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import {
  TlsCertificateDecisionDialog,
  tlsCertificateFromError,
  type TlsCertificateInfo,
  type TlsDecision,
} from '@/shared/components'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Select } from '@/shared/components/ui/select'
import { useCreateAccount } from '@/shared/hooks/useAccounts'
import { ApiError } from '@/shared/api'
import { startOAuthRedirect } from '@/shared/lib/oauth'
import {
  SETTINGS_EMAIL_RE,
  ACCT_COLORS,
  PROVIDER_PRESET_OPTIONS,
  discoverServersAsync,
  discoverServersForProvider,
  serverDefaults,
  type DiscoverResult,
  type ProviderPresetId,
  type Security,
} from '@/shared/lib/serverDiscovery'
import { FieldLabel, Swatches, ServerGroup, SrvField, type AccountFormState } from './AccountFields'
import type { Account } from '@/shared/types'

interface AddAccountFormProps {
  onCancel: () => void
  onCreated: () => void
}

function emptyAccountForm(): AccountFormState {
  return {
    name: '', short: '', email: '', initials: '', color: ACCT_COLORS[0],
    composeFormat: 'rich', hasJunk: true, signature: '',
    imapHost: '', imapPort: '', imapSecurity: 'ssl', imapUser: '', imapPass: '',
    smtpHost: '', smtpPort: '', smtpSecurity: 'starttls', smtpUser: '', smtpPass: '',
    separateCreds: false,
    carddavUrl: '', caldavUrl: '',
  }
}

export function AddAccountForm({ onCancel, onCreated }: AddAccountFormProps) {
  const { t } = useTranslation()
  const createAccount = useCreateAccount()
  const [step, setStep] = useState<1 | 2 | 3>(1)
  const [data, setData] = useState<AccountFormState>(emptyAccountForm)
  const [phase, setPhase] = useState<'idle' | 'detecting' | 'done'>('idle')
  const [result, setResult] = useState<DiscoverResult | null>(null)
  const [providerPreset, setProviderPreset] = useState<ProviderPresetId>('auto')
  const [oauthOk, setOauthOk] = useState(false)
  const [contactsEnabled, setContactsEnabled] = useState(true)
  const [created, setCreated] = useState<Account | null>(null)
  const detectSeq = useRef(0)

  const set = <K extends keyof AccountFormState>(k: K, v: AccountFormState[K]) =>
    setData((d) => ({ ...d, [k]: v }))

  const emailInvalid = !!data.email && !SETTINGS_EMAIL_RE.test(data.email)
  // Display name + short label are optional; only a valid email is required.
  const step1Valid = SETTINGS_EMAIL_RE.test(data.email)
  const known = !!result && result.source !== 'autoconfig'
  const oauthProvider: 'google' | 'microsoft' | null =
    result?.oauth && /google/i.test(result.provider) ? 'google' : result?.oauth ? 'microsoft' : null

  function applyDiscovery(d: DiscoverResult) {
    setData((prev) => ({
      ...prev,
      imapHost: d.imapHost, imapPort: String(d.imapPort), imapSecurity: d.imapSecurity, imapUser: d.imapUser,
      smtpHost: d.smtpHost, smtpPort: String(d.smtpPort), smtpSecurity: d.smtpSecurity, smtpUser: d.smtpUser,
      carddavUrl: d.carddavUrl, caldavUrl: d.caldavUrl,
      short: prev.short || data.email,
    }))
    setResult(d)
    setPhase('done')
  }

  async function runDetect() {
    setStep(2)
    setPhase('detecting')
    setResult(null)
    setOauthOk(false)
    const requested = ++detectSeq.current
    const d = providerPreset === 'auto'
      ? await discoverServersAsync(data.email)
      : discoverServersForProvider(data.email, providerPreset)
    // Ignore stale results when the user went back and re-ran detection.
    if (requested !== detectSeq.current) return
    applyDiscovery(d)
  }

  async function chooseProvider(preset: ProviderPresetId) {
    setProviderPreset(preset)
    if (!SETTINGS_EMAIL_RE.test(data.email)) return
    setPhase('detecting')
    const requested = ++detectSeq.current
    const d = preset === 'auto'
      ? await discoverServersAsync(data.email)
      : discoverServersForProvider(data.email, preset)
    if (requested !== detectSeq.current) return
    applyDiscovery(d)
  }

  const providerOptionLabel = (option: { id: ProviderPresetId; label: string }) => {
    if (option.id === 'auto') return t('wiz.autoProvider')
    if (option.id === 'imap') return t('wiz.regularImap')
    return option.label
  }

  function finish(trustCert?: TlsCertificateInfo, tlsDecision?: TlsDecision) {
    const def = serverDefaults(data.email)
    const authScheme = oauthOk ? 'xoauth2' : 'plain'
    const imapHost = data.imapHost || def.imapHost
    const smtpHost = data.smtpHost || def.smtpHost
    const email = data.email.trim()
    // IMAP credentials are the shared default; SMTP reuses them unless the user
    // chose to enter separate credentials.
    const imapUser = data.imapUser || email
    const smtpUser = data.separateCreds ? data.smtpUser || email : imapUser
    const smtpPass = data.separateCreds ? data.smtpPass : data.imapPass
    createAccount.mutate(
      {
        display_name: data.name.trim() || email,
        primary_email: email,
        imap_host: imapHost,
        imap_port: Number(data.imapPort || def.imapPort),
        imap_username: imapUser,
        imap_password: data.imapPass,
        imap_auth_scheme: authScheme,
        smtp_host: smtpHost,
        smtp_port: Number(data.smtpPort || def.smtpPort),
        smtp_username: smtpUser,
        smtp_password: smtpPass,
        smtp_auth_scheme: authScheme,
        body_sync_mode: 'lazy',
        carddav_url: data.carddavUrl || undefined,
        caldav_url: data.caldavUrl || undefined,
        // Trust exception accepted by the user; the SMTP side only gets it
        // when it talks to the same host the certificate came from.
        imap_tls_cert: trustCert?.der_base64,
        smtp_tls_cert: trustCert && smtpHost === trustCert.host ? trustCert.der_base64 : undefined,
        tls_decision: tlsDecision,
        contacts_enabled: contactsEnabled,
      },
      { onSuccess: setCreated },
    )
  }

  // Shared credentials need one password; separate mode needs both.
  const credsValid = data.separateCreds ? !!data.imapPass && !!data.smtpPass : !!data.imapPass
  const step3Valid = !!data.imapHost && !!data.smtpHost && credsValid
  const tlsCert = tlsCertificateFromError(createAccount.error)

  const secOpts: [Security, string][] = [
    ['ssl', t('settings.secSsl')],
    ['starttls', t('settings.secStarttls')],
    ['none', t('settings.secNone')],
  ]

  if (created) {
    const contactState = created.contacts?.state ?? 'unavailable'
    const needsAttention = ['consent_required', 'reauth_required', 'error', 'unavailable'].includes(contactState)
    return (
      <div className="mb-4 rounded-[10px] border border-[#16a34a]/30 bg-card p-5" role="status" aria-live="polite">
        <div className="flex items-start gap-3">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-full bg-[#16a34a] text-white">
            <Check className="size-5" />
          </span>
          <div>
            <h3 className="text-[16px] font-bold">{t('contacts.setupMailReady')}</h3>
            <p className="mt-1 text-[13px] text-muted-foreground">
              {needsAttention ? t('contacts.setupNeedsAttention') : contactsEnabled ? t('contacts.setupSyncStarting') : t('contacts.setupSkipped')}
            </p>
          </div>
        </div>
        <div className="mt-5 flex flex-wrap justify-end gap-2">
          <Button variant="ghost" onClick={onCreated}>{t('contacts.setUpLater')}</Button>
          {needsAttention && <Button variant="outline" onClick={onCreated}>{t('contacts.fixContacts')}</Button>}
          {contactsEnabled && !needsAttention && (
            <Button onClick={() => window.location.assign('/mail/contacts')}>{t('contacts.viewContacts')}</Button>
          )}
          {!contactsEnabled && <Button onClick={onCreated}>{t('action.done')}</Button>}
        </div>
      </div>
    )
  }

  return (
    <div className="mb-4 rounded-[10px] border border-[#3b82f6] bg-card p-5 shadow-[0_0_0_3px_rgba(59,130,246,0.1)]">
      <TlsCertificateDecisionDialog
        open={step === 3 && Boolean(tlsCert)}
        host={tlsCert?.host}
        port={tlsCert?.port}
        fingerprint={tlsCert?.fingerprint_sha256}
        pending={createAccount.isPending}
        onDecision={(decision) => {
          if (decision === 'deny') {
            createAccount.reset()
            return
          }
          if (tlsCert) finish(tlsCert, decision)
        }}
      />
      <WizStepper step={step} />

      {step === 1 && (
        <div>
          <div className="text-[16px] font-bold tracking-tight text-foreground">{t('wiz.title1')}</div>
          <div className="mb-[18px] mt-0.5 text-[13px] text-muted-foreground">{t('wiz.sub1')}</div>
          <div className="grid grid-cols-1 gap-x-[18px] gap-y-4 md:grid-cols-2">
            <div>
              <FieldLabel hint={t('settings.optional')}>{t('settings.displayName')}</FieldLabel>
              <Input value={data.name} onChange={(e) => set('name', e.currentTarget.value)} placeholder="Frank Gehann" />
            </div>
            <div>
              <FieldLabel>{t('settings.email')}</FieldLabel>
              <Input
                value={data.email}
                onChange={(e) => set('email', e.currentTarget.value)}
                placeholder="name@example.com"
                className={cn('font-mono', emailInvalid && 'border-destructive focus-visible:ring-destructive')}
              />
              {emailInvalid && <div className="mt-1 text-[11.5px] text-destructive">{t('settings.emailInvalid')}</div>}
            </div>
            <div>
              <FieldLabel hint={`${t('settings.sidebar')} · ${t('settings.optional')}`}>{t('settings.shortLabel')}</FieldLabel>
              <Input value={data.short} onChange={(e) => set('short', e.currentTarget.value)} placeholder={data.email || 'Personal'} />
            </div>
            <div className="md:col-span-2">
              <FieldLabel>{t('settings.accountColor')}</FieldLabel>
              <Swatches value={data.color} onChange={(v) => set('color', v)} />
            </div>
          </div>
          <div className="mt-5 flex justify-end gap-2.5">
            <Button type="button" variant="outline" onClick={onCancel}>{t('action.cancel')}</Button>
            <Button type="button" disabled={!step1Valid} onClick={runDetect}>
              {t('wiz.next')}
              <ChevronRight className="size-4" />
            </Button>
          </div>
        </div>
      )}

      {step === 2 && (
        <div>
          <div className="text-[16px] font-bold tracking-tight text-foreground">{t('wiz.title2')}</div>
          <div className="mb-[18px] mt-0.5 font-mono text-[13px] text-muted-foreground">{data.email}</div>

          {phase === 'detecting' && (
            <div className="flex items-center gap-3 rounded-[10px] bg-secondary px-[18px] py-[22px]">
              <RefreshCw className="size-5 animate-spin text-[#2563eb]" />
              <span className="text-[13.5px] font-semibold text-secondary-foreground">{t('wiz.checking')}</span>
            </div>
          )}

          {phase === 'done' && known && (
            <div className="flex flex-col gap-3.5">
              <div className="flex items-start gap-3 rounded-[10px] border border-[#16a34a]/30 bg-[#16a34a]/10 px-4 py-3.5">
                <span className="flex size-[34px] shrink-0 items-center justify-center rounded-full bg-[#16a34a]">
                  <Check className="size-[18px] text-white" strokeWidth={3} />
                </span>
                <div className="flex-1">
                  <div className="text-[14px] font-bold text-foreground">{t('wiz.found', { provider: result?.provider })}</div>
                  <div className="mt-0.5 text-[12.5px] leading-snug text-secondary-foreground">{t('wiz.foundSub')}</div>
                </div>
              </div>
              <SrvSummary data={data} />
              <div className="rounded-[9px] border border-border bg-card px-3.5 py-3">
                <FieldLabel>{t('wiz.manualProvider')}</FieldLabel>
                <Select value={providerPreset} onChange={(e) => chooseProvider(e.currentTarget.value as ProviderPresetId)}>
                  {PROVIDER_PRESET_OPTIONS.map((option) => (
                    <option key={option.id} value={option.id}>{providerOptionLabel(option)}</option>
                  ))}
                </Select>
              </div>
              {oauthProvider && (
                <div className="space-y-3 rounded-[9px] border border-[#bfdbfe] bg-[var(--mq-row-open)] px-3.5 py-3">
                  <label className="flex cursor-pointer items-start gap-3">
                    <input
                      type="checkbox"
                      className="mt-0.5 size-4 accent-[#2563eb]"
                      checked={contactsEnabled}
                      onChange={(event) => setContactsEnabled(event.currentTarget.checked)}
                    />
                    <span>
                      <span className="block text-[13px] font-bold">{t('contacts.syncContacts')}</span>
                      <span className="block text-[12px] leading-snug text-muted-foreground">{t('contacts.permissionExplanation')}</span>
                    </span>
                  </label>
                  <div className="flex items-center gap-3">
                    <ShieldAlert className="size-[18px] shrink-0 text-[#2563eb]" />
                    <span className="flex-1 text-[12.5px] leading-snug text-secondary-foreground">{t('wiz.oauthNeeded')}</span>
                    <button
                      type="button"
                      onClick={() => startOAuthRedirect(oauthProvider, undefined, contactsEnabled ? 'contacts' : undefined)}
                      className="h-8 shrink-0 rounded-[7px] bg-[#2563eb] px-3.5 text-[12.5px] font-bold text-white hover:bg-[#1d4ed8]"
                    >
                      {t('settings.oauthSignIn', { provider: result?.provider })}
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}

          {phase === 'done' && !known && (
            <div className="flex flex-col gap-3.5">
              <div className="flex items-start gap-3 rounded-[10px] border border-[#d97706]/30 bg-[#d97706]/10 px-4 py-3.5">
                <span className="flex size-[34px] shrink-0 items-center justify-center rounded-full bg-[#d97706]">
                  <AlertCircle className="size-[18px] text-white" />
                </span>
                <div className="flex-1">
                  <div className="text-[14px] font-bold text-foreground">{t('wiz.notFound')}</div>
                  <div className="mt-0.5 text-[12.5px] leading-snug text-secondary-foreground">{t('wiz.notFoundSub')}</div>
                </div>
              </div>
              <div className="rounded-[9px] border border-border bg-card px-3.5 py-3">
                <FieldLabel>{t('wiz.manualProvider')}</FieldLabel>
                <Select value={providerPreset} onChange={(e) => chooseProvider(e.currentTarget.value as ProviderPresetId)}>
                  {PROVIDER_PRESET_OPTIONS.map((option) => (
                    <option key={option.id} value={option.id}>{providerOptionLabel(option)}</option>
                  ))}
                </Select>
              </div>
            </div>
          )}

          {/* Credentials — shared for IMAP+SMTP by default; OAuth providers sign
              in via redirect and need none. */}
          {phase === 'done' && !oauthProvider && (
            <div className="mt-4 flex flex-col gap-3">
              {data.separateCreds && (
                <div className="text-[11.5px] font-bold uppercase tracking-wide text-muted-foreground">{t('settings.imap')}</div>
              )}
              <SrvField label={t('settings.username')}>
                <Input className="font-mono" value={data.imapUser} placeholder={data.email} onChange={(e) => set('imapUser', e.currentTarget.value)} />
              </SrvField>
              <SrvField label={t('settings.password')}>
                <Input type="password" autoComplete="new-password" value={data.imapPass} onChange={(e) => set('imapPass', e.currentTarget.value)} />
              </SrvField>
              <label className="flex cursor-pointer items-center gap-2.5 text-[12.5px] text-secondary-foreground">
                <input
                  type="checkbox"
                  className="size-4 accent-[#2563eb]"
                  checked={!!data.separateCreds}
                  onChange={(e) => set('separateCreds', e.currentTarget.checked)}
                />
                {t('wiz.separateCreds')}
              </label>
              {data.separateCreds && (
                <div className="flex flex-col gap-3 rounded-[9px] border border-border p-3.5">
                  <div className="text-[11.5px] font-bold uppercase tracking-wide text-muted-foreground">{t('settings.smtp')}</div>
                  <SrvField label={t('settings.username')}>
                    <Input className="font-mono" value={data.smtpUser} placeholder={data.email} onChange={(e) => set('smtpUser', e.currentTarget.value)} />
                  </SrvField>
                  <SrvField label={t('settings.password')}>
                    <Input type="password" autoComplete="new-password" value={data.smtpPass} onChange={(e) => set('smtpPass', e.currentTarget.value)} />
                  </SrvField>
                </div>
              )}
            </div>
          )}

          <div className="mt-5 flex items-center gap-2.5">
            <Button type="button" variant="outline" onClick={() => setStep(1)}>
              <ChevronLeft className="size-4" />
              {t('wiz.back')}
            </Button>
            {phase === 'done' && (
              <Button type="button" className="ml-auto" onClick={() => setStep(3)}>
                {t('wiz.next')}
                <ChevronRight className="size-4" />
              </Button>
            )}
          </div>
        </div>
      )}

      {step === 3 && (
        <div>
          <div className="text-[16px] font-bold tracking-tight text-foreground">{t('wiz.title3')}</div>
          <div className="mb-[18px] mt-0.5 text-[13px] text-muted-foreground">{t('wiz.sub3')}</div>

          <div className="flex flex-col gap-5">
            <div className="rounded-[10px] border border-border bg-secondary/40 p-4">
              <label className="flex cursor-pointer items-start gap-3">
                <input
                  type="checkbox"
                  className="mt-0.5 size-4 accent-[#2563eb]"
                  checked={contactsEnabled}
                  onChange={(event) => setContactsEnabled(event.currentTarget.checked)}
                />
                <span className="flex-1">
                  <span className="block text-[13.5px] font-bold">{t('contacts.syncContacts')}</span>
                  <span className="mt-0.5 block text-[12px] text-muted-foreground">
                    {data.carddavUrl ? t('contacts.carddavAvailable') : t('contacts.carddavWillDiscover')}
                  </span>
                </span>
              </label>
              {!contactsEnabled && <p className="mt-2 text-[12px] text-muted-foreground">{t('contacts.continueWithout')}</p>}
            </div>
            <ServerGroup title={t('settings.imap')} icon={Inbox}>
              <div className="flex gap-3">
                <SrvField label={t('settings.host')} w="flex-[2.2]">
                  <Input className="font-mono" value={data.imapHost} onChange={(e) => set('imapHost', e.currentTarget.value)} />
                </SrvField>
                <SrvField label={t('settings.port')} w="flex-[0.7]">
                  <Input className="font-mono" value={data.imapPort} onChange={(e) => set('imapPort', e.currentTarget.value)} />
                </SrvField>
                <SrvField label={t('settings.security')} w="flex-[1.1]">
                  <Select value={data.imapSecurity} onChange={(e) => set('imapSecurity', e.currentTarget.value as Security)}>
                    {secOpts.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                  </Select>
                </SrvField>
              </div>
            </ServerGroup>

            <ServerGroup title={t('settings.smtp')} icon={Send}>
              <div className="flex gap-3">
                <SrvField label={t('settings.host')} w="flex-[2.2]">
                  <Input className="font-mono" value={data.smtpHost} onChange={(e) => set('smtpHost', e.currentTarget.value)} />
                </SrvField>
                <SrvField label={t('settings.port')} w="flex-[0.7]">
                  <Input className="font-mono" value={data.smtpPort} onChange={(e) => set('smtpPort', e.currentTarget.value)} />
                </SrvField>
                <SrvField label={t('settings.security')} w="flex-[1.1]">
                  <Select value={data.smtpSecurity} onChange={(e) => set('smtpSecurity', e.currentTarget.value as Security)}>
                    {secOpts.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                  </Select>
                </SrvField>
              </div>
            </ServerGroup>

            <details className="rounded-[9px] border border-border p-3">
              <summary className="cursor-pointer text-[12.5px] font-semibold">{t('contacts.advancedCarddav')}</summary>
              <div className="mt-3">
                <ServerGroup title={t('settings.carddav')} icon={Users}>
                  <SrvField label={t('settings.url')}>
                    <Input className="font-mono" value={data.carddavUrl} onChange={(e) => set('carddavUrl', e.currentTarget.value)} />
                  </SrvField>
                </ServerGroup>
              </div>
            </details>
            <ServerGroup title={t('settings.caldav')} icon={Calendar}>
              <SrvField label={t('settings.url')}>
                <Input className="font-mono" value={data.caldavUrl} onChange={(e) => set('caldavUrl', e.currentTarget.value)} />
              </SrvField>
            </ServerGroup>
          </div>

          {createAccount.error && !tlsCert ? (
            <p className="mt-3 text-[12.5px] text-destructive">
              {t('settings.addFailed')}
              {createAccount.error instanceof ApiError && createAccount.error.detail ? (
                <span className="mt-1 block font-mono text-[11.5px] opacity-80">{createAccount.error.detail}</span>
              ) : null}
            </p>
          ) : null}

          <div className="mt-5 flex items-center gap-2.5">
            <Button type="button" variant="outline" onClick={() => setStep(2)}>
              <ChevronLeft className="size-4" />
              {t('wiz.back')}
            </Button>
            <Button type="button" className="ml-auto" disabled={!step3Valid || createAccount.isPending} onClick={() => finish()}>
              <Check className="size-4" />
              {createAccount.isPending ? t('settings.adding') : t('settings.addAccount')}
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}

function WizStepper({ step }: { step: 1 | 2 | 3 }) {
  const { t } = useTranslation()
  const steps: [number, string][] = [
    [1, t('wiz.step1')],
    [2, t('wiz.step2')],
    [3, t('wiz.step3')],
  ]
  return (
    <div className="mb-5 flex items-center">
      {steps.map(([n, label], i) => {
        const done = step > n
        const active = step === n
        return (
          <Fragment key={n}>
            <div className="flex items-center gap-2.5">
              <span
                className={cn(
                  'flex size-[26px] shrink-0 items-center justify-center rounded-full text-[12.5px] font-bold',
                  done ? 'bg-[#16a34a] text-white' : active ? 'bg-[#2563eb] text-white' : 'border border-border bg-secondary text-muted-foreground',
                )}
              >
                {done ? <Check className="size-3.5 text-white" strokeWidth={3} /> : n}
              </span>
              <span
                className={cn(
                  'whitespace-nowrap text-[12.5px]',
                  active ? 'font-bold text-foreground' : done ? 'font-bold text-secondary-foreground' : 'font-semibold text-muted-foreground',
                )}
              >
                {label}
              </span>
            </div>
            {i < steps.length - 1 && (
              <span className={cn('mx-3 h-0.5 flex-1 rounded', step > n ? 'bg-[#16a34a]' : 'bg-border')} />
            )}
          </Fragment>
        )
      })}
    </div>
  )
}

function SrvSummary({ data }: { data: AccountFormState }) {
  const { t } = useTranslation()
  const row = (label: string, host: string, port: string, sec: Security) => (
    <div className="flex items-baseline gap-2.5 text-[12.5px]">
      <span className="w-[70px] shrink-0 text-[11px] font-bold uppercase tracking-wide text-muted-foreground">{label}</span>
      <span className="font-mono text-[12.5px] text-foreground">
        {host}
        <span className="text-muted-foreground">:{port}</span>
      </span>
      <span className="ml-auto text-[11px] font-semibold text-muted-foreground">
        {sec === 'ssl' ? 'SSL/TLS' : sec === 'starttls' ? 'STARTTLS' : '—'}
      </span>
    </div>
  )
  return (
    <div className="flex flex-col gap-2.5 rounded-[9px] bg-secondary px-3.5 py-3">
      {row(t('wiz.incoming'), data.imapHost, data.imapPort, data.imapSecurity)}
      {row(t('wiz.outgoing'), data.smtpHost, data.smtpPort, data.smtpSecurity)}
    </div>
  )
}
