import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { Trash2 } from 'lucide-react'
import { z } from 'zod'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { Badge } from '@/shared/components/ui/badge'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { useAccounts, useDeleteAccount, useUpdateAccount } from '@/shared/hooks/useAccounts'
import {
  getStoredPushSubscriptionId,
  pushNotificationsSupported,
  useDisablePushNotifications,
  useEnablePushNotifications,
  useVapidPublicKey,
} from '@/shared/hooks/usePushNotifications'
import { useSettings, useUpdateSettings } from '@/shared/hooks/useSettings'
import type { Account } from '@/shared/types'

interface SettingsDialogProps {
  open: boolean
  onClose: () => void
}

export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const { data: accounts = [] } = useAccounts()
  const { data: settings } = useSettings()
  const updateSettings = useUpdateSettings()
  const { data: vapidPublicKey } = useVapidPublicKey()
  const enablePushNotifications = useEnablePushNotifications()
  const disablePushNotifications = useDisablePushNotifications()
  const [notificationsEnabled, setNotificationsEnabled] = useState(() => Boolean(getStoredPushSubscriptionId()))
  const [notificationMessage, setNotificationMessage] = useState<string | null>(null)

  async function handleNotificationsToggle(checked: boolean) {
    setNotificationMessage(null)

    if (!checked) {
      await disablePushNotifications.mutateAsync()
      setNotificationsEnabled(false)
      return
    }

    if (!pushNotificationsSupported()) {
      setNotificationsEnabled(false)
      setNotificationMessage('Desktop notifications are not supported by this browser.')
      return
    }

    const permission = await Notification.requestPermission()
    if (permission !== 'granted') {
      setNotificationsEnabled(false)
      setNotificationMessage('Permission was denied. Re-enable notifications in your browser site settings, then turn this on again.')
      return
    }

    if (!vapidPublicKey?.public_key) {
      setNotificationsEnabled(false)
      setNotificationMessage('Notification setup is not available yet.')
      return
    }

    await enablePushNotifications.mutateAsync(vapidPublicKey.public_key)
    setNotificationsEnabled(true)
  }

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(980px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
        </DialogHeader>

        <div className="grid gap-5 lg:grid-cols-[1.4fr_1fr]">
          <section className="flex flex-col gap-3">
            <div>
              <h3 className="text-sm font-semibold">Accounts</h3>
              <p className="text-sm text-muted-foreground">Edit server settings or remove accounts.</p>
            </div>
            <div className="flex max-h-[60vh] flex-col gap-3 overflow-y-auto pr-1">
              {accounts.map((account) => (
                <AccountEditor key={account.id} account={account} />
              ))}
              {!accounts.length ? (
                <p className="rounded-md border border-border p-4 text-sm text-muted-foreground">
                  No accounts connected yet.
                </p>
              ) : null}
            </div>
          </section>

          <section className="flex flex-col gap-4">
            <div>
              <h3 className="text-sm font-semibold">Privacy</h3>
              <p className="text-sm text-muted-foreground">
                Enabling PGP discovery can send email addresses to external servers.
              </p>
            </div>
            <ToggleRow
              id="pgp-wkd"
              label="WKD discovery"
              checked={Boolean(settings?.pgp_discovery_wkd_enabled)}
              disabled={!settings || updateSettings.isPending}
              onChange={(checked) => updateSettings.mutate({ pgp_discovery_wkd_enabled: checked })}
            />
            <ToggleRow
              id="pgp-keyserver"
              label="Keyserver discovery"
              checked={Boolean(settings?.pgp_discovery_keyserver_enabled)}
              disabled={!settings || updateSettings.isPending}
              onChange={(checked) => updateSettings.mutate({ pgp_discovery_keyserver_enabled: checked })}
            />
            <div className="border-t border-border pt-4">
              <h3 className="text-sm font-semibold">Notifications</h3>
              <p className="text-sm text-muted-foreground">Desktop notifications are sent directly with Web Push.</p>
            </div>
            <ToggleRow
              id="desktop-notifications"
              label="Desktop notifications"
              checked={notificationsEnabled}
              disabled={enablePushNotifications.isPending || disablePushNotifications.isPending}
              onChange={(checked) => {
                handleNotificationsToggle(checked).catch(() => {
                  setNotificationsEnabled(false)
                  setNotificationMessage('Notification setup failed. Check browser permissions and try again.')
                })
              }}
            />
            {notificationMessage ? <p className="text-sm text-muted-foreground">{notificationMessage}</p> : null}
          </section>
        </div>
      </DialogContent>
    </Dialog>
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
})

type AccountEditInput = z.input<typeof accountEditSchema>
type AccountEditData = z.output<typeof accountEditSchema>

function AccountEditor({ account }: { account: Account }) {
  const updateAccount = useUpdateAccount()
  const deleteAccount = useDeleteAccount()
  const {
    register,
    handleSubmit,
    reset,
    formState: { errors },
  } = useForm<AccountEditInput, unknown, AccountEditData>({
    resolver: zodResolver(accountEditSchema),
    defaultValues: accountToForm(account),
  })

  useEffect(() => {
    reset(accountToForm(account))
  }, [account, reset])

  function onSubmit(data: AccountEditData) {
    updateAccount.mutate({ id: account.id, data })
  }

  function onDelete() {
    if (window.confirm(`Delete ${account.display_name}?`)) {
      deleteAccount.mutate(account.id)
    }
  }

  return (
    <form className="rounded-md border border-border p-4" onSubmit={handleSubmit(onSubmit)}>
      <div className="mb-3 flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h4 className="truncate text-sm font-semibold">{account.display_name}</h4>
          <p className="truncate text-xs text-muted-foreground">{account.primary_email}</p>
        </div>
        <Badge variant="secondary">{account.body_sync_mode}</Badge>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <Field id={`${account.id}-display`} label="Display name" error={errors.display_name?.message}>
          <Input id={`${account.id}-display`} {...register('display_name')} />
        </Field>
        <Field id={`${account.id}-sync-interval`} label="Sync interval seconds" error={errors.sync_interval_secs?.message}>
          <Input id={`${account.id}-sync-interval`} type="number" {...register('sync_interval_secs')} />
        </Field>
        <Field id={`${account.id}-imap-host`} label="IMAP host" error={errors.imap_host?.message}>
          <Input id={`${account.id}-imap-host`} {...register('imap_host')} />
        </Field>
        <Field id={`${account.id}-imap-port`} label="IMAP port" error={errors.imap_port?.message}>
          <Input id={`${account.id}-imap-port`} type="number" {...register('imap_port')} />
        </Field>
        <Field id={`${account.id}-smtp-host`} label="SMTP host" error={errors.smtp_host?.message}>
          <Input id={`${account.id}-smtp-host`} {...register('smtp_host')} />
        </Field>
        <Field id={`${account.id}-smtp-port`} label="SMTP port" error={errors.smtp_port?.message}>
          <Input id={`${account.id}-smtp-port`} type="number" {...register('smtp_port')} />
        </Field>
        <Field id={`${account.id}-imap-auth`} label="IMAP auth" error={errors.imap_auth_scheme?.message}>
          <Input id={`${account.id}-imap-auth`} {...register('imap_auth_scheme')} />
        </Field>
        <Field id={`${account.id}-smtp-auth`} label="SMTP auth" error={errors.smtp_auth_scheme?.message}>
          <Input id={`${account.id}-smtp-auth`} {...register('smtp_auth_scheme')} />
        </Field>
        <Field id={`${account.id}-body-mode`} label="Body sync mode" error={errors.body_sync_mode?.message}>
          <Select id={`${account.id}-body-mode`} {...register('body_sync_mode')}>
            <option value="lazy">Load on open</option>
            <option value="full">Download during sync</option>
          </Select>
        </Field>
      </div>

      <div className="mt-4 flex justify-between gap-2">
        <Button type="button" variant="destructive" size="sm" onClick={onDelete} disabled={deleteAccount.isPending}>
          <Trash2 className="size-4" aria-hidden="true" />
          Delete
        </Button>
        <Button type="submit" size="sm" disabled={updateAccount.isPending}>
          {updateAccount.isPending ? 'Saving...' : 'Save changes'}
        </Button>
      </div>
    </form>
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
  }
}

interface ToggleRowProps {
  id: string
  label: string
  checked: boolean
  disabled: boolean
  onChange: (checked: boolean) => void
}

function ToggleRow({ id, label, checked, disabled, onChange }: ToggleRowProps) {
  return (
    <label className="flex items-center justify-between gap-3 rounded-md border border-border p-3 text-sm">
      <span>
        <span className="block font-medium">{label}</span>
        <span className="block text-xs text-muted-foreground">Off by default for privacy.</span>
      </span>
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.currentTarget.checked)}
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
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
    </div>
  )
}
