import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { useSearchParams, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { z } from 'zod'
import {
  ArrowLeft,
  Users,
  Palette,
  PenLine,
  Bell,
  Filter,
  Plus,
  Trash2,
  ChevronDown,
  ShieldCheck,
  CalendarDays,
  Check,
  Monitor,
  Sun,
  Moon,
  RefreshCw,
  Wrench,
} from 'lucide-react'
import { ApiError } from '@/shared/api'
import { cn } from '@/shared/lib/utils'
import { accountInitials, resolveAccountColor } from '@/shared/lib/avatar'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { Button } from '@/shared/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import {
  TlsCertificateDecisionDialog,
  tlsCertificateFromError,
  type TlsDecision,
} from '@/shared/components'
import { AddAccountForm, Swatches } from '@/features/accounts'
import { RulesSection } from '@/widgets/RulesSection'
import { PgpKeyManagement } from '@/widgets/PgpKeyManagement'
import { davDefaults } from '@/shared/lib/dav'
import {
  useAccounts,
  useDeleteAccount,
  useUpdateAccount,
  useFolders,
  useSetFolderSync,
  useSetAllFoldersSync,
  useEnableMailboxContacts,
  useDisableMailboxContacts,
  useDiscoverMailboxContacts,
} from '@/shared/hooks/useAccounts'
import { useSyncContactAccount } from '@/shared/hooks/useContacts'
import { startOAuthRedirect } from '@/shared/lib/oauth'
import { useThemeStore, type ThemePref } from '@/shared/hooks/useTheme'
import { useUiPrefs, type Density, type CalendarGrouping } from '@/shared/hooks/useUiPrefs'
import { getLangPref, setLangPref, type LangPref } from '@/shared/i18n'
import {
  useAddBrandEntry,
  useBrandEntries,
  useDeleteBrandEntry,
  useImageAllowlist,
  useRemoveAllowedImageSender,
  useResetPhishingAnalysis,
  useSettings,
  useUpdateSettings,
} from '@/shared/hooks/useSettings'
import { useCalendars } from '@/shared/hooks/useCalendar'
import {
  pushNotificationsSupported,
  useDisablePushNotifications,
  useEnablePushNotifications,
  useVapidPublicKey,
} from '@/shared/hooks/usePushNotifications'
import type { Account, DiscoveredContactBook } from '@/shared/types'

type Section = 'accounts' | 'calendar' | 'appearance' | 'composing' | 'notifications' | 'rules' | 'privacy'

const NAV: { id: Section; icon: typeof Users }[] = [
  { id: 'accounts', icon: Users },
  { id: 'calendar', icon: CalendarDays },
  { id: 'appearance', icon: Palette },
  { id: 'composing', icon: PenLine },
  { id: 'rules', icon: Filter },
  { id: 'notifications', icon: Bell },
  { id: 'privacy', icon: ShieldCheck },
]

function providerOf(account: Account): string {
  const host = `${account.imap_host} ${account.smtp_host}`.toLowerCase()
  if (host.includes('gmail') || host.includes('google')) return 'Google'
  if (host.includes('office365') || host.includes('outlook') || host.includes('microsoft')) return 'Outlook'
  if (host.includes('gmx')) return 'GMX'
  if (host.includes('icloud') || host.includes('apple')) return 'iCloud'
  return 'IMAP'
}

export function SettingsPage() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const [searchParams] = useSearchParams()
  const [section, setSection] = useState<Section>(() => {
    const requested = searchParams.get('section')
    return (NAV.find((n) => n.id === requested)?.id as Section) ?? 'accounts'
  })

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border bg-card px-5">
        <button
          onClick={() => navigate('/mail/unified')}
          title={t('settings.backToMail')}
          className="flex size-9 items-center justify-center rounded-md border border-border text-secondary-foreground hover:bg-secondary"
        >
          <ArrowLeft className="size-4" />
        </button>
        <h1 className="text-[17px] font-bold tracking-tight">{t('settings.title')}</h1>
      </header>

      <div className="grid min-h-0 flex-1 grid-cols-[220px_1fr]">
        <nav className="space-y-0.5 border-r border-border bg-card p-3">
          {NAV.map(({ id, icon: Icon }) => {
            const on = section === id
            return (
              <button
                key={id}
                onClick={() => setSection(id)}
                className={cn(
                  'flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-[13.5px] font-semibold transition-colors',
                  on ? 'bg-[var(--mq-row-open)] text-[#1d4ed8]' : 'text-secondary-foreground hover:bg-secondary',
                )}
              >
                <Icon className="size-4" />
                {t(`settings.${id}`)}
              </button>
            )
          })}
        </nav>

        <div className="min-h-0 overflow-y-auto">
          <div className="mx-auto max-w-[1180px] p-6">
            {section === 'accounts' && <AccountsSection />}
            {section === 'calendar' && <CalendarSettingsSection />}
            {section === 'appearance' && <AppearanceSection />}
            {section === 'composing' && <ComposingSection />}
            {section === 'rules' && <RulesSection />}
            {section === 'notifications' && <NotificationsSection />}
            {section === 'privacy' && <PrivacySection />}
          </div>
        </div>
      </div>
    </div>
  )
}

function CalendarSettingsSection() {
  const { t } = useTranslation()
  const { data: accounts = [] } = useAccounts()
  const { data: calendars = [] } = useCalendars()
  const { data: settings } = useSettings()
  const updateSettings = useUpdateSettings()
  const mailboxByAccountId = new Map(accounts.map((account) => [account.id, account.primary_email]))
  return (
    <div>
      <SectionHeader title={t('settings.calendar')} description={t('settings.calendarDesc')} />
      <div className="max-w-md rounded-lg border border-border bg-card p-4">
        <Field id="default-calendar" label={t('settings.defaultCalendar')}>
          <Select
            id="default-calendar"
            value={settings?.default_calendar_id ?? ''}
            onChange={(event) => updateSettings.mutate({ default_calendar_id: event.currentTarget.value || null })}
          >
            <option value="">{t('settings.noDefaultCalendar')}</option>
            {calendars.map((calendar) => {
              const mailbox = calendar.account_id ? mailboxByAccountId.get(calendar.account_id) : undefined
              return (
                <option key={calendar.id} value={calendar.id}>
                  {mailbox ? `${calendar.name} (${mailbox})` : calendar.name}
                </option>
              )
            })}
          </Select>
        </Field>
      </div>
    </div>
  )
}

function SectionHeader({ title, description }: { title: string; description: string }) {
  return (
    <div className="mb-5">
      <h2 className="text-[18px] font-bold tracking-tight">{title}</h2>
      <p className="mt-1 text-[13px] text-muted-foreground">{description}</p>
    </div>
  )
}

function AccountsSection() {
  const { t } = useTranslation()
  const { data: accounts = [] } = useAccounts()
  const [searchParams, setSearchParams] = useSearchParams()
  const [adding, setAdding] = useState(() => searchParams.get('add') === '1')
  const connected = searchParams.get('connected')
  const contactsReturn = searchParams.get('contacts')

  useEffect(() => {
    if (!connected || !contactsReturn) return
    window.requestAnimationFrame(() => document.getElementById(`contact-capability-${connected}`)?.focus())
  }, [accounts, connected, contactsReturn])

  function closeAdd() {
    setAdding(false)
    if (searchParams.get('add')) {
      const next = new URLSearchParams(searchParams)
      next.delete('add')
      setSearchParams(next, { replace: true })
    }
  }

  return (
    <div>
      <SectionHeader title={t('settings.accounts')} description={t('settings.accountsDesc')} />

      {connected && (
        <div className="mb-4 flex items-center gap-2 rounded-lg border border-[#16a34a]/30 bg-[#16a34a]/10 px-4 py-3 text-[13px] font-medium text-[#15803d]">
          <Check className="size-4" />
          {contactsReturn ? t('contacts.oauthReturnComplete') : t('settings.connected')}
          <button
            onClick={() => setSearchParams({}, { replace: true })}
            className="ml-auto text-[12px] font-semibold underline"
          >
            {t('settings.dismiss')}
          </button>
        </div>
      )}

      {!adding && (
        <div className="mb-5">
          <Button type="button" onClick={() => setAdding(true)}>
            <Plus className="size-4" />
            {t('settings.addAccount')}
          </Button>
        </div>
      )}

      {adding && <AddAccountForm onCancel={closeAdd} onCreated={closeAdd} />}

      <div className="flex flex-col gap-3">
        {accounts.map((account) => (
          <AccountCard key={account.id} account={account} />
        ))}
        {!accounts.length && !adding && (
          <p className="rounded-lg border border-border p-6 text-center text-sm text-muted-foreground">
            {t('settings.noAccounts')}
          </p>
        )}
      </div>
    </div>
  )
}

const accountEditSchema = z.object({
  display_name: z.string().min(1),
  imap_host: z.string().min(1),
  imap_port: z.coerce.number().int().positive(),
  imap_auth_scheme: z.string().min(1),
  smtp_host: z.string().min(1),
  smtp_port: z.coerce.number().int().positive(),
  smtp_auth_scheme: z.string().min(1),
  body_sync_mode: z.enum(['lazy', 'full']),
  sync_interval_secs: z.coerce.number().int().positive(),
  sync_mode: z.enum(['idle', 'interval']),
  carddav_url: z.string().optional(),
  caldav_url: z.string().optional(),
  color: z.string().regex(/^#[0-9A-Fa-f]{6}$/).optional(),
})
type AccountEditInput = z.input<typeof accountEditSchema>
type AccountEditData = z.output<typeof accountEditSchema>

function AccountCard({ account }: { account: Account }) {
  const { t } = useTranslation()
  const [expanded, setExpanded] = useState(false)
  const [tlsRetryData, setTlsRetryData] = useState<AccountEditData | null>(null)
  const updateAccount = useUpdateAccount()
  const deleteAccount = useDeleteAccount()
  const color = resolveAccountColor(account)
  const {
    register,
    handleSubmit,
    reset,
    setValue,
    watch,
    formState: { errors },
  } = useForm<AccountEditInput, unknown, AccountEditData>({
    resolver: zodResolver(accountEditSchema),
    defaultValues: accountToForm(account),
  })

  useEffect(() => {
    reset(accountToForm(account))
  }, [account, reset])

  function autodetectDav() {
    const { carddav_url, caldav_url } = davDefaults(account.primary_email, account.imap_host)
    setValue('carddav_url', carddav_url, { shouldDirty: true })
    setValue('caldav_url', caldav_url, { shouldDirty: true })
  }

  function saveAccount(data: AccountEditData, decision?: TlsDecision) {
    setTlsRetryData(data)
    const certificate = tlsCertificateFromError(updateAccount.error)
    updateAccount.mutate({
      id: account.id,
      data: {
        ...data,
        imap_tls_cert: certificate?.der_base64,
        smtp_tls_cert:
          certificate && data.smtp_host === certificate.host ? certificate.der_base64 : undefined,
        tls_decision: decision,
      },
    })
  }

  const tlsCertificate = tlsCertificateFromError(updateAccount.error)

  return (
    <div className="overflow-hidden rounded-lg border border-border bg-card">
      <TlsCertificateDecisionDialog
        open={Boolean(tlsCertificate)}
        host={tlsCertificate?.host}
        port={tlsCertificate?.port}
        fingerprint={tlsCertificate?.fingerprint_sha256}
        pending={updateAccount.isPending}
        onDecision={(decision) => {
          if (decision === 'deny') {
            updateAccount.reset()
            return
          }
          if (tlsRetryData) saveAccount(tlsRetryData, decision)
        }}
      />
      <button
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center gap-3 px-4 py-3.5 text-left"
      >
        <span
          className="flex size-9 shrink-0 items-center justify-center rounded-lg text-[12px] font-extrabold uppercase text-white"
          style={{ backgroundColor: color }}
        >
          {accountInitials(account.display_name)}
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[14px] font-bold">{account.display_name}</div>
          <div className="truncate text-[12.5px] text-muted-foreground">{account.primary_email}</div>
        </div>
        <span className="shrink-0 rounded-full bg-secondary px-2.5 py-1 text-[11px] font-bold text-secondary-foreground">
          {providerOf(account)}
        </span>
        <ChevronDown className={cn('size-4 shrink-0 text-muted-foreground transition-transform', expanded && 'rotate-180')} />
      </button>

      <ContactCapabilityRow account={account} />

      {expanded && (
        <form className="border-t border-border p-4" onSubmit={handleSubmit((data) => saveAccount(data))}>
          <div className="grid gap-3 md:grid-cols-2">
            <Field id={`${account.id}-display`} label={t('settings.displayName')} error={errors.display_name?.message}>
              <Input id={`${account.id}-display`} {...register('display_name')} />
            </Field>
            <Field id={`${account.id}-sync-mode`} label={t('settings.syncMode')} error={errors.sync_mode?.message}>
              <Select id={`${account.id}-sync-mode`} {...register('sync_mode')}>
                <option value="idle">{t('settings.syncModeIdle')}</option>
                <option value="interval">{t('settings.syncModeInterval')}</option>
              </Select>
            </Field>
            <Field id={`${account.id}-sync`} label={t('settings.syncInterval')} error={errors.sync_interval_secs?.message}>
              <Input id={`${account.id}-sync`} type="number" {...register('sync_interval_secs')} />
            </Field>
            <Field id={`${account.id}-imap-host`} label={t('settings.imapHost')} error={errors.imap_host?.message}>
              <Input id={`${account.id}-imap-host`} {...register('imap_host')} />
            </Field>
            <Field id={`${account.id}-imap-port`} label={t('settings.imapPort')} error={errors.imap_port?.message}>
              <Input id={`${account.id}-imap-port`} type="number" {...register('imap_port')} />
            </Field>
            <Field id={`${account.id}-smtp-host`} label={t('settings.smtpHost')} error={errors.smtp_host?.message}>
              <Input id={`${account.id}-smtp-host`} {...register('smtp_host')} />
            </Field>
            <Field id={`${account.id}-smtp-port`} label={t('settings.smtpPort')} error={errors.smtp_port?.message}>
              <Input id={`${account.id}-smtp-port`} type="number" {...register('smtp_port')} />
            </Field>
            <Field id={`${account.id}-imap-auth`} label={t('settings.imapAuth')} error={errors.imap_auth_scheme?.message}>
              <Input id={`${account.id}-imap-auth`} {...register('imap_auth_scheme')} />
            </Field>
            <Field id={`${account.id}-smtp-auth`} label={t('settings.smtpAuth')} error={errors.smtp_auth_scheme?.message}>
              <Input id={`${account.id}-smtp-auth`} {...register('smtp_auth_scheme')} />
            </Field>
            <Field id={`${account.id}-body`} label={t('settings.bodySyncMode')} error={errors.body_sync_mode?.message}>
              <Select id={`${account.id}-body`} {...register('body_sync_mode')}>
                <option value="lazy">{t('settings.loadOnOpen')}</option>
                <option value="full">{t('settings.downloadDuringSync')}</option>
              </Select>
            </Field>
          </div>

          <div className="mt-4 border-t border-border pt-4">
            <div className="mb-2 text-[12px] font-bold uppercase tracking-wide text-muted-foreground">
              {t('settings.accountColor')}
            </div>
            <Swatches
              value={watch('color') ?? color}
              onChange={(v) => setValue('color', v, { shouldDirty: true })}
            />
          </div>

          {/* CalDAV stays part of the normal mailbox settings. Contact server
              overrides are intentionally kept in advanced setup. */}
          <div className="mt-4 border-t border-border pt-4">
            <div className="mb-3 flex items-center justify-between">
              <span className="text-[12px] font-bold uppercase tracking-wide text-muted-foreground">
                {t('settings.davSection')}
              </span>
              <Button type="button" variant="outline" size="sm" onClick={autodetectDav}>
                {t('settings.autoDetect')}
              </Button>
            </div>
            <div className="grid gap-3 md:grid-cols-2">
              <Field id={`${account.id}-caldav`} label={t('settings.caldavUrl')}>
                <Input id={`${account.id}-caldav`} placeholder="https://…/.well-known/caldav" {...register('caldav_url')} />
              </Field>
            </div>
            <details className="mt-3 rounded-md border border-border p-3">
              <summary className="cursor-pointer text-[12.5px] font-semibold">{t('contacts.advancedCarddav')}</summary>
              <div className="mt-3 max-w-xl">
                <Field id={`${account.id}-carddav`} label={t('settings.carddavUrl')}>
                  <Input id={`${account.id}-carddav`} placeholder="https://…/.well-known/carddav" {...register('carddav_url')} />
                </Field>
              </div>
            </details>
          </div>

          {/* Folder selection — pick which mailboxes get synced */}
          <FolderSyncList accountId={account.id} />

          <div className="mt-4 flex justify-between gap-2">
            <Button
              type="button"
              variant="destructive"
              size="sm"
              onClick={() => window.confirm(t('settings.deleteAccountConfirm', { name: account.display_name })) && deleteAccount.mutate(account.id)}
              disabled={deleteAccount.isPending}
            >
              <Trash2 className="size-4" />
              {t('settings.delete')}
            </Button>
            <Button type="submit" size="sm" disabled={updateAccount.isPending}>
              {updateAccount.isPending ? t('settings.saving') : t('settings.saveChanges')}
            </Button>
          </div>
        </form>
      )}
    </div>
  )
}

export function ContactCapabilityRow({ account }: { account: Account }) {
  const { t } = useTranslation()
  const enable = useEnableMailboxContacts()
  const disable = useDisableMailboxContacts()
  const discover = useDiscoverMailboxContacts()
  const updateAccount = useUpdateAccount()
  const sync = useSyncContactAccount()
  const [disableOpen, setDisableOpen] = useState(false)
  const [setupOpen, setSetupOpen] = useState(false)
  const [keepCache, setKeepCache] = useState(true)
  const [cardDavUrl, setCardDavUrl] = useState(account.carddav_url ?? '')
  const [selectedBooks, setSelectedBooks] = useState<string[]>([])
  const [tlsDecision, setTlsDecision] = useState<'accept' | 'accept_always'>()
  const capability = account.contacts
  const state = capability?.state ?? 'disabled'
  const provider = capability?.provider === 'google' ? 'Google' : capability?.provider === 'graph' ? 'Microsoft' : 'CardDAV'
  const providerConfigurationRequired = capability?.reason === 'provider_configuration_required'
  const busy = enable.isPending || disable.isPending || discover.isPending || sync.isPending
  const cardDavTlsError = discover.error instanceof ApiError && discover.error.code === 'carddav_tls_certificate_invalid'
  const discoveryErrorMessage = discover.error instanceof ApiError
    ? discover.error.detail ?? discover.error.message
    : String(discover.error)

  function rememberDefaultBooks({ books }: { books: DiscoveredContactBook[] }) {
    setSelectedBooks(books.filter((book) => book.is_default || books.length === 1).map((book) => book.remote_id))
  }

  function primaryAction() {
    if (!capability || state === 'disabled') {
      if (capability?.provider === 'cardav') {
        setTlsDecision(undefined)
        setSetupOpen(true)
        discover.mutate(
          { accountId: account.id },
          { onSuccess: rememberDefaultBooks },
        )
      } else {
        enable.mutate(account.id)
      }
    } else if (state === 'consent_required' || state === 'reauth_required') {
      startOAuthRedirect(capability.provider === 'google' ? 'google' : 'microsoft', account.id, 'contacts')
    } else if (providerConfigurationRequired) {
      enable.mutate(account.id)
    } else if (state === 'error' || state === 'unavailable') {
      if (capability.provider === 'cardav') {
        setTlsDecision(undefined)
        setSetupOpen(true)
        discover.mutate(
          { accountId: account.id },
          { onSuccess: rememberDefaultBooks },
        )
      }
      else startOAuthRedirect(capability.provider === 'google' ? 'google' : 'microsoft', account.id, 'contacts')
    } else if (state !== 'syncing' && state !== 'pending') {
      sync.mutate(capability.source_id)
    }
  }

  const actionLabel = providerConfigurationRequired
    ? t('contacts.tryAgain')
    : !capability || state === 'disabled'
    ? t('contacts.enable')
    : state === 'consent_required'
      ? t('contacts.grantAccess')
      : state === 'reauth_required'
        ? t('contacts.reconnect')
        : state === 'error' || state === 'unavailable'
          ? t('contacts.fixContacts')
          : state === 'syncing' || state === 'pending'
            ? t('contacts.syncing')
            : t('contacts.syncNow')

  return (
    <div id={`contact-capability-${account.id}`} tabIndex={-1} className="border-t border-border bg-secondary/25 px-4 py-3 outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-live="polite">
      <TlsCertificateDecisionDialog
        open={setupOpen && cardDavTlsError}
        pending={discover.isPending}
        onDecision={(decision) => {
          if (decision === 'deny') {
            discover.reset()
            return
          }
          setTlsDecision(decision)
          discover.mutate(
            { accountId: account.id, tlsDecision: decision },
            { onSuccess: rememberDefaultBooks },
          )
        }}
      />
      <div className="flex flex-wrap items-center gap-3">
        <Users className="size-4 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-[13px] font-bold">
            {t('contacts.contactsCapability')}
            <span className="rounded-full bg-secondary px-2 py-0.5 text-[10.5px] text-secondary-foreground">{provider}</span>
          </div>
          <p className="mt-0.5 text-[11.5px] text-muted-foreground">
            {t(`contacts.state.${state}`)}
            {capability?.last_synced_at ? ` · ${t('contacts.lastSync', { value: new Date(capability.last_synced_at).toLocaleString() })}` : ''}
            {state === 'disabled' && capability?.cache_retained ? ` · ${t('contacts.cacheReadOnly')}` : ''}
          </p>
          {providerConfigurationRequired ? (
            <div role="alert" className="mt-1 text-[11.5px] text-destructive">
              <span>{t('contacts.providerConfigurationRequired', { provider })}</span>{' '}
              {capability?.provider === 'google' ? (
                <a
                  className="font-semibold underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  href="https://console.cloud.google.com/apis/library/people.googleapis.com"
                  target="_blank"
                  rel="noreferrer"
                >
                  {t('contacts.openProviderConsole')}
                </a>
              ) : null}
            </div>
          ) : null}
        </div>
        <Button size="sm" variant={state === 'idle' ? 'outline' : 'default'} onClick={primaryAction} disabled={busy || state === 'syncing' || state === 'pending'}>
          {(busy || state === 'syncing' || state === 'pending') && <RefreshCw className="size-3.5 animate-spin motion-reduce:animate-none" />}
          {(state === 'error' || state === 'unavailable') && !busy && <Wrench className="size-3.5" />}
          {actionLabel}
        </Button>
        {capability?.enabled && (
          <Button size="sm" variant="ghost" onClick={() => setDisableOpen(true)}>{t('contacts.disable')}</Button>
        )}
      </div>
      {discover.data ? <p className="mt-2 text-[11.5px] text-muted-foreground">{t('contacts.discoverySucceeded')}</p> : null}
      {discover.error && !cardDavTlsError ? <p role="alert" className="mt-2 text-[11.5px] text-destructive">{discoveryErrorMessage}</p> : null}

      <Dialog open={setupOpen && !cardDavTlsError} onClose={() => setSetupOpen(false)}>
        <DialogContent className="w-[min(560px,calc(100vw-2rem))] max-w-none">
          <DialogHeader><DialogTitle>{t('contacts.carddavSetupTitle')}</DialogTitle></DialogHeader>
          <p className="text-[13px] text-muted-foreground">{t('contacts.carddavSetupHelp')}</p>
          {discover.isPending ? (
            <p role="status" className="mt-4 flex items-center gap-2 text-[13px]"><RefreshCw className="size-4 animate-spin motion-reduce:animate-none" />{t('contacts.discovering')}</p>
          ) : discover.data ? (
            <fieldset className="mt-4 space-y-2">
              <legend className="text-[12px] font-bold uppercase tracking-wide text-muted-foreground">{t('contacts.selectBooks')}</legend>
              {discover.data.books.map((book) => (
                <label key={book.remote_id} className="flex items-center gap-3 rounded-md border border-border p-3 text-[13px]">
                  <input
                    type="checkbox"
                    checked={selectedBooks.includes(book.remote_id)}
                    onChange={(event) => setSelectedBooks((current) => event.currentTarget.checked ? [...current, book.remote_id] : current.filter((id) => id !== book.remote_id))}
                  />
                  <span className="font-semibold">{book.display_name}</span>
                  {book.is_default ? <span className="ml-auto text-[11px] text-muted-foreground">{t('contacts.defaultBook')}</span> : null}
                </label>
              ))}
            </fieldset>
          ) : null}
          {discover.error ? (
            <p role="alert" className="mt-4 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-[12px] text-destructive">{discoveryErrorMessage}</p>
          ) : null}
          <details className="mt-4 rounded-md border border-border p-3">
            <summary className="cursor-pointer text-[12.5px] font-semibold">{t('contacts.advancedCarddav')}</summary>
            <Field id={`${account.id}-setup-carddav`} label={t('settings.carddavUrl')}>
              <Input id={`${account.id}-setup-carddav`} className="mt-3" value={cardDavUrl} onChange={(event) => setCardDavUrl(event.currentTarget.value)} placeholder="https://…/.well-known/carddav" />
            </Field>
            <Button
              className="mt-3"
              type="button"
              variant="outline"
              disabled={!cardDavUrl.trim() || updateAccount.isPending || discover.isPending}
              onClick={() => updateAccount.mutate(
                { id: account.id, data: { carddav_url: cardDavUrl.trim() } },
                { onSuccess: () => discover.mutate({ accountId: account.id }) },
              )}
            >{t('contacts.tryDiscovery')}</Button>
          </details>
          <div className="mt-5 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setSetupOpen(false)}>{t('action.cancel')}</Button>
            <Button
              disabled={!discover.data || selectedBooks.length === 0 || discover.isPending || enable.isPending}
              onClick={() => discover.mutate(
                { accountId: account.id, selectedBookRemoteIds: selectedBooks, tlsDecision },
                { onSuccess: () => enable.mutate(account.id, { onSuccess: () => setSetupOpen(false) }) },
              )}
            >{t('contacts.enableSelectedBooks')}</Button>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={disableOpen} onClose={() => setDisableOpen(false)}>
        <DialogContent className="w-[min(520px,calc(100vw-2rem))] max-w-none">
          <DialogHeader><DialogTitle>{t('contacts.disableTitle')}</DialogTitle></DialogHeader>
          <p className="text-[13px] text-muted-foreground">{t('contacts.disableExplanation')}</p>
          <label className="mt-3 flex items-start gap-3 rounded-md border border-border p-3">
            <input type="radio" checked={keepCache} onChange={() => setKeepCache(true)} className="mt-0.5" />
            <span><strong className="block text-[13px]">{t('contacts.keepCache')}</strong><span className="text-[12px] text-muted-foreground">{t('contacts.keepCacheHelp')}</span></span>
          </label>
          <label className="mt-2 flex items-start gap-3 rounded-md border border-border p-3">
            <input type="radio" checked={!keepCache} onChange={() => setKeepCache(false)} className="mt-0.5" />
            <span><strong className="block text-[13px]">{t('contacts.removeCache')}</strong><span className="text-[12px] text-muted-foreground">{t('contacts.removeCacheHelp')}</span></span>
          </label>
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setDisableOpen(false)}>{t('action.cancel')}</Button>
            <Button
              variant={keepCache ? 'default' : 'destructive'}
              onClick={() => {
                if (!keepCache && !window.confirm(t('contacts.removeCacheConfirm'))) return
                disable.mutate({ accountId: account.id, keepDownloadedContacts: keepCache }, { onSuccess: () => setDisableOpen(false) })
              }}
              disabled={disable.isPending}
            >{t('contacts.disable')}</Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function accountToForm(account: Account): AccountEditInput {
  return {
    display_name: account.display_name,
    imap_host: account.imap_host,
    imap_port: account.imap_port,
    imap_auth_scheme: account.imap_auth_scheme,
    smtp_host: account.smtp_host,
    smtp_port: account.smtp_port,
    smtp_auth_scheme: account.smtp_auth_scheme,
    body_sync_mode: account.body_sync_mode === 'full' ? 'full' : 'lazy',
    sync_interval_secs: account.sync_interval_secs,
    sync_mode: account.sync_mode === 'interval' ? 'interval' : 'idle',
    carddav_url: account.carddav_url ?? '',
    caldav_url: account.caldav_url ?? '',
    color: account.color ?? undefined,
  }
}

function FolderSyncList({ accountId }: { accountId: string }) {
  const { t } = useTranslation()
  const { data: folders = [] } = useFolders(accountId)
  const setSync = useSetFolderSync(accountId)
  const setAllSync = useSetAllFoldersSync(accountId)

  if (!folders.length) return null

  return (
    <div className="mt-4 border-t border-border pt-4">
      <div className="flex items-center justify-between">
        <span className="text-[12px] font-bold uppercase tracking-wide text-muted-foreground">
          {t('settings.syncedFolders')}
        </span>
        <div className="flex gap-1.5">
          <Button type="button" variant="outline" size="sm" disabled={setAllSync.isPending} onClick={() => setAllSync.mutate(true)}>
            {t('settings.selectAllFolders')}
          </Button>
          <Button type="button" variant="outline" size="sm" disabled={setAllSync.isPending} onClick={() => setAllSync.mutate(false)}>
            {t('settings.selectNoFolders')}
          </Button>
        </div>
      </div>
      <p className="mb-3 mt-1 text-[12px] text-muted-foreground">{t('settings.syncedFoldersDesc')}</p>
      <div className="grid gap-1.5 sm:grid-cols-2">
        {folders.map((folder) => {
          const enabled = folder.sync_enabled !== false
          return (
            <label
              key={folder.id}
              className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-[13px] hover:bg-secondary/60"
            >
              <input
                type="checkbox"
                checked={enabled}
                onChange={(e) => setSync.mutate({ folderPath: folder.full_path, syncEnabled: e.target.checked })}
                className="size-4 accent-[#2563eb]"
              />
              <span className="truncate">{folder.full_path}</span>
            </label>
          )
        })}
      </div>
    </div>
  )
}

function AppearanceSection() {
  const { t } = useTranslation()
  const { pref, setPref } = useThemeStore()
  const { density, setDensity, calendarGrouping, setCalendarGrouping } = useUiPrefs()
  const [lang, setLang] = useState<LangPref>(() => getLangPref())

  function changeLang(next: LangPref) {
    setLang(next)
    setLangPref(next)
  }

  return (
    <div>
      <SectionHeader title={t('settings.appearance')} description={t('settings.appearanceDesc')} />
      <div className="flex flex-col gap-6">
        <PrefGroup label={t('settings.theme')}>
          {(
            [
              ['system', t('settings.system'), Monitor],
              ['light', t('settings.light'), Sun],
              ['dark', t('settings.dark'), Moon],
            ] as [ThemePref, string, typeof Monitor][]
          ).map(([value, label, Icon]) => (
            <Choice key={value} active={pref === value} onClick={() => setPref(value)}>
              <Icon className="size-4" />
              {label}
            </Choice>
          ))}
        </PrefGroup>

        <PrefGroup label={t('settings.language')}>
          {(
            [
              ['system', t('settings.system')],
              ['en', t('settings.english')],
              ['de', t('settings.german')],
            ] as [LangPref, string][]
          ).map(([value, label]) => (
            <Choice key={value} active={lang === value} onClick={() => changeLang(value)}>
              {label}
            </Choice>
          ))}
        </PrefGroup>

        <PrefGroup label={t('settings.listDensity')}>
          {(['compact', 'comfortable', 'roomy'] as Density[]).map((value) => (
            <Choice key={value} active={density === value} onClick={() => setDensity(value)}>
              {t(`settings.${value}`)}
            </Choice>
          ))}
        </PrefGroup>

        <PrefGroup label={t('settings.calendarGrouping')}>
          {(
            [
              ['account', t('settings.groupByAccount')],
              ['flat', t('settings.flatList')],
            ] as [CalendarGrouping, string][]
          ).map(([value, label]) => (
            <Choice key={value} active={calendarGrouping === value} onClick={() => setCalendarGrouping(value)}>
              {label}
            </Choice>
          ))}
        </PrefGroup>
      </div>
    </div>
  )
}

function ComposingSection() {
  const { t } = useTranslation()
  const { maxRecipients, setMaxRecipients } = useUiPrefs()
  return (
    <div>
      <SectionHeader title={t('settings.composing')} description={t('settings.composingDesc')} />
      <div className="max-w-md rounded-lg border border-border bg-card p-4">
        <Label htmlFor="max-recipients" className="text-[13px] font-semibold">
          {t('settings.recipientsPerField')}
        </Label>
        <p className="mb-2.5 mt-1 text-[12.5px] text-muted-foreground">{t('settings.recipientsHint')}</p>
        <Input
          id="max-recipients"
          type="number"
          min={1}
          value={maxRecipients}
          onChange={(e) => setMaxRecipients(Number(e.currentTarget.value) || 1)}
          className="w-32"
        />
      </div>
    </div>
  )
}

/** Detect Brave so we can point users at its push-service setting. */
async function isBrave(): Promise<boolean> {
  const nav = navigator as Navigator & { brave?: { isBrave?: () => Promise<boolean> } }
  try {
    return (await nav.brave?.isBrave?.()) ?? false
  } catch {
    return false
  }
}

/** iOS Safari only supports web push when installed to the home screen. */
function isIosSafariNonStandalone(): boolean {
  const ua = navigator.userAgent
  const ios =
    /iPad|iPhone|iPod/.test(ua) ||
    (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1)
  const standalone =
    (navigator as Navigator & { standalone?: boolean }).standalone === true ||
    window.matchMedia('(display-mode: standalone)').matches
  return ios && !standalone
}

/** Map a push subscribe failure to a user-facing i18n key. A push-service
 *  registration error means the browser's push backend is unavailable: Brave
 *  disables Google's FCM by default; de-googled Chromium builds (Ungoogled,
 *  some Vivaldi/Kiwi setups) have no push service at all. */
async function classifyPushFailure(err: unknown): Promise<string> {
  const e = err instanceof Error ? err : null
  const serviceError = !e || e.name === 'AbortError' || /push service|permission denied/i.test(e.message)
  if (serviceError) {
    if (await isBrave()) return 'settings.pushBraveBlocked'
    return 'settings.pushServiceUnavailable'
  }
  return 'settings.pushForegroundOnly'
}

function NotificationsSection() {
  const { t } = useTranslation()
  const { data: vapidPublicKey } = useVapidPublicKey()
  const enablePush = useEnablePushNotifications()
  const disablePush = useDisablePushNotifications()
  const enabled = useUiPrefs((s) => s.notificationsEnabled)
  const setEnabled = useUiPrefs((s) => s.setNotificationsEnabled)
  const [message, setMessage] = useState<string | null>(null)

  async function toggleNotifications(checked: boolean) {
    setMessage(null)
    if (!checked) {
      setEnabled(false)
      await disablePush.mutateAsync().catch(() => {})
      return
    }
    // The Notification API is the baseline for both the background (web push)
    // and foreground (SSE) paths.
    if (!('Notification' in window)) {
      setMessage(t('settings.pushUnsupported'))
      return
    }
    // Push/notifications require a secure context (HTTPS; localhost counts).
    if (!window.isSecureContext) {
      setMessage(t('settings.pushInsecure'))
      return
    }
    if ((await Notification.requestPermission()) !== 'granted') {
      setMessage(t('settings.pushPermissionDenied'))
      return
    }
    // Notifications are on from here: the in-app (SSE) path works without a
    // browser push service.
    setEnabled(true)

    // Background web push needs a service worker + PushManager. iOS Safari only
    // provides them once installed to the home screen; otherwise it's
    // foreground-only.
    if (!pushNotificationsSupported()) {
      setMessage(t(isIosSafariNonStandalone() ? 'settings.pushIosPwa' : 'settings.pushForegroundOnly'))
      return
    }
    if (!vapidPublicKey?.public_key) {
      setMessage(t('settings.pushNotConfigured'))
      return
    }
    try {
      await enablePush.mutateAsync(vapidPublicKey.public_key)
      setMessage(t('settings.pushEnabled'))
    } catch (err) {
      // Background push unavailable (push service blocked/missing). Tell the
      // user the specifics; foreground notifications keep working meanwhile.
      setMessage(t(await classifyPushFailure(err)))
    }
  }

  return (
    <div>
      <SectionHeader title={t('settings.notifications')} description={t('settings.notificationsDesc')} />
      <div className="flex max-w-2xl flex-col gap-3">
        <Toggle
          label={t('settings.desktopNotifications')}
          hint={t('settings.desktopNotificationsHint')}
          checked={enabled}
          disabled={enablePush.isPending || disablePush.isPending}
          onChange={(c) =>
            toggleNotifications(c).catch((err) => {
              console.error('notification setup failed:', err)
              const detail = err instanceof Error ? err.message : String(err)
              setMessage(`${t('settings.pushSetupFailed')} (${detail})`)
            })
          }
        />
        {message && <p className="text-[12.5px] text-muted-foreground">{message}</p>}
      </div>
    </div>
  )
}

function PrivacySection() {
  const { t } = useTranslation()
  const { data: settings } = useSettings()
  const updateSettings = useUpdateSettings()

  return (
    <div>
      <SectionHeader title={t('settings.privacy')} description={t('settings.privacyDesc')} />
      <div className="flex max-w-2xl flex-col gap-3">
        <Toggle
          label={t('settings.externalImages')}
          hint={t('settings.externalImagesHint')}
          checked={Boolean(settings?.load_external_images)}
          disabled={!settings || updateSettings.isPending}
          onChange={(c) => updateSettings.mutate({ load_external_images: c })}
        />
        <ImageAllowlist />
        <CustomBrandsSettings />
        <Toggle
          label={t('settings.pgpWkd')}
          hint={t('settings.pgpHint')}
          checked={Boolean(settings?.pgp_discovery_wkd_enabled)}
          disabled={!settings || updateSettings.isPending}
          onChange={(c) => updateSettings.mutate({ pgp_discovery_wkd_enabled: c })}
        />
        <Toggle
          label={t('settings.pgpKeyserver')}
          hint={t('settings.pgpHint')}
          checked={Boolean(settings?.pgp_discovery_keyserver_enabled)}
          disabled={!settings || updateSettings.isPending}
          onChange={(c) => updateSettings.mutate({ pgp_discovery_keyserver_enabled: c })}
        />
        <PgpKeyManagement />
      </div>
    </div>
  )
}

function CustomBrandsSettings() {
  const { t } = useTranslation()
  const { data: brands = [] } = useBrandEntries()
  const addBrand = useAddBrandEntry()
  const deleteBrand = useDeleteBrandEntry()
  const resetPhishing = useResetPhishingAnalysis()
  const [brandName, setBrandName] = useState('')
  const [domain, setDomain] = useState('')

  function submit() {
    if (!brandName.trim() || !domain.trim()) return
    addBrand.mutate(
      { brand_name: brandName.trim(), domain: domain.trim() },
      {
        onSuccess: () => {
          setBrandName('')
          setDomain('')
        },
      },
    )
  }

  return (
    <div className="rounded-lg border border-border bg-card p-3.5">
      <div className="text-sm font-semibold">{t('settings.customBrands')}</div>
      <p className="mb-3 mt-0.5 text-[12.5px] text-muted-foreground">{t('settings.customBrandsHint')}</p>
      <div className="grid gap-2 sm:grid-cols-[1fr_1fr_auto]">
        <Input
          value={brandName}
          onChange={(event) => setBrandName(event.currentTarget.value)}
          placeholder={t('settings.brandName')}
        />
        <Input
          value={domain}
          onChange={(event) => setDomain(event.currentTarget.value)}
          placeholder={t('settings.brandDomain')}
        />
        <Button type="button" onClick={submit} disabled={addBrand.isPending || !brandName.trim() || !domain.trim()}>
          <Plus className="size-4" />
          {t('settings.add')}
        </Button>
      </div>
      {brands.length > 0 && (
        <div className="mt-3 divide-y divide-border rounded-md border border-border">
          {brands.map((brand) => (
            <div key={brand.id} className="flex items-center gap-3 px-3 py-2 text-sm">
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{brand.brand_name}</span>
                <span className="block truncate font-mono text-xs text-muted-foreground">{brand.domain}</span>
              </span>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                onClick={() => deleteBrand.mutate(brand.id)}
                disabled={deleteBrand.isPending}
                title={t('action.delete')}
              >
                <Trash2 className="size-4" />
              </Button>
            </div>
          ))}
        </div>
      )}
      <div className="mt-3 flex items-center justify-between gap-3 rounded-md bg-secondary/40 p-3 text-sm">
        <span>
          <span className="block font-semibold">{t('settings.phishingReset')}</span>
          <span className="block text-[12.5px] text-muted-foreground">{t('settings.phishingResetHint')}</span>
        </span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => resetPhishing.mutate()}
          disabled={resetPhishing.isPending}
        >
          {resetPhishing.isPending
            ? t('settings.phishingResetting')
            : resetPhishing.isSuccess
              ? t('settings.phishingResetDone')
              : t('settings.phishingResetAction')}
        </Button>
      </div>
    </div>
  )
}

function ImageAllowlist() {
  const { t } = useTranslation()
  const { data: allowlist = [] } = useImageAllowlist()
  const removeSender = useRemoveAllowedImageSender()

  if (!allowlist.length) return null

  return (
    <div className="rounded-lg border border-border bg-card p-3.5">
      <div className="text-sm font-semibold">{t('settings.imageAllowlist')}</div>
      <p className="mb-2.5 mt-0.5 text-[12.5px] text-muted-foreground">{t('settings.imageAllowlistHint')}</p>
      <ul className="flex flex-col gap-1.5">
        {allowlist.map((entry) => (
          <li
            key={entry.sender}
            className="flex items-center justify-between gap-2 rounded-md border border-border px-3 py-1.5"
          >
            <span className="truncate font-mono text-[12px] text-secondary-foreground">{entry.sender}</span>
            <button
              type="button"
              title={t('action.delete')}
              aria-label={t('action.delete')}
              onClick={() => removeSender.mutate(entry.sender)}
              disabled={removeSender.isPending}
              className="text-destructive transition-opacity hover:opacity-80 disabled:opacity-50"
            >
              <Trash2 className="size-4" />
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}

function PrefGroup({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <div className="mb-2 text-[11px] font-bold uppercase tracking-[0.08em] text-muted-foreground">{label}</div>
      <div className="flex flex-wrap gap-2">{children}</div>
    </div>
  )
}

function Choice({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'inline-flex h-9 items-center gap-2 rounded-lg border px-4 text-[13px] font-semibold transition-colors',
        active
          ? 'border-[#3b82f6] bg-[var(--mq-row-open)] text-[#1d4ed8]'
          : 'border-border bg-card text-secondary-foreground hover:bg-secondary',
      )}
    >
      {children}
    </button>
  )
}

function Toggle({
  label,
  hint,
  checked,
  disabled,
  onChange,
}: {
  label: string
  hint: string
  checked: boolean
  disabled: boolean
  onChange: (checked: boolean) => void
}) {
  return (
    <label className="flex items-center justify-between gap-3 rounded-lg border border-border bg-card p-3.5 text-sm">
      <span>
        <span className="block font-semibold">{label}</span>
        <span className="block text-[12.5px] text-muted-foreground">{hint}</span>
      </span>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.currentTarget.checked)}
        className="size-4 accent-primary"
      />
    </label>
  )
}

interface FieldProps {
  id: string
  label: string
  error?: string
  children: ReactNode
}

function Field({ id, label, error, children }: FieldProps) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={id} className="text-[12px] font-semibold">
        {label}
      </Label>
      {children}
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  )
}
