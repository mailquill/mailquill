import { useId } from 'react'
import type { ReactNode } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { Mail, ShieldCheck } from 'lucide-react'
import { z } from 'zod'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { useCreateAccount } from '@/shared/hooks/useAccounts'

const accountSchema = z.object({
  display_name: z.string().min(1, 'Display name is required'),
  primary_email: z.string().email('Enter a valid email address'),
  imap_host: z.string().min(1, 'IMAP host is required'),
  imap_port: z.coerce.number().int().positive(),
  imap_username: z.string().min(1, 'IMAP username is required'),
  imap_password: z.string().min(1, 'IMAP password is required'),
  imap_auth_scheme: z.enum(['plain', 'login', 'cram-md5', 'oauth2', 'xoauth2']),
  smtp_host: z.string().min(1, 'SMTP host is required'),
  smtp_port: z.coerce.number().int().positive(),
  smtp_username: z.string().min(1, 'SMTP username is required'),
  smtp_password: z.string().min(1, 'SMTP password is required'),
  smtp_auth_scheme: z.enum(['plain', 'login', 'cram-md5', 'oauth2', 'xoauth2']),
  body_sync_mode: z.enum(['lazy', 'full']),
})

type AccountFormInput = z.input<typeof accountSchema>
type AccountFormData = z.output<typeof accountSchema>

interface AddAccountDialogProps {
  open: boolean
  onClose: () => void
}

export function AddAccountDialog({ open, onClose }: AddAccountDialogProps) {
  const fieldPrefix = useId()
  const createAccount = useCreateAccount()
  const {
    register,
    handleSubmit,
    reset,
    formState: { errors },
  } = useForm<AccountFormInput, unknown, AccountFormData>({
    resolver: zodResolver(accountSchema),
    defaultValues: {
      imap_port: 993,
      imap_auth_scheme: 'plain',
      smtp_port: 587,
      smtp_auth_scheme: 'plain',
      body_sync_mode: 'lazy',
    },
  })

  function startOAuth(provider: 'google' | 'microsoft') {
    window.location.assign(`/api/auth/oauth/${provider}/start`)
  }

  function onSubmit(data: AccountFormData) {
    createAccount.mutate(data, {
      onSuccess: () => {
        reset()
        onClose()
      },
    })
  }

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(920px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>Add account</DialogTitle>
        </DialogHeader>

        <div className="mb-5 flex flex-wrap gap-2">
          <Button type="button" variant="outline" onClick={() => startOAuth('google')}>
            <Mail className="size-4" aria-hidden="true" />
            Connect Gmail
          </Button>
          <Button type="button" variant="outline" onClick={() => startOAuth('microsoft')}>
            <ShieldCheck className="size-4" aria-hidden="true" />
            Connect Outlook
          </Button>
        </div>

        <form className="flex flex-col gap-5" onSubmit={handleSubmit(onSubmit)}>
          <div className="grid gap-4 md:grid-cols-2">
            <Field id={`${fieldPrefix}-display-name`} label="Display name" error={errors.display_name?.message}>
              <Input id={`${fieldPrefix}-display-name`} {...register('display_name')} />
            </Field>
            <Field id={`${fieldPrefix}-primary-email`} label="Primary email" error={errors.primary_email?.message}>
              <Input id={`${fieldPrefix}-primary-email`} type="email" {...register('primary_email')} />
            </Field>
          </div>

          <div className="grid gap-5 lg:grid-cols-2">
            <section className="flex flex-col gap-4 rounded-md border border-border p-4">
              <h3 className="text-sm font-semibold">IMAP</h3>
              <Field id={`${fieldPrefix}-imap-host`} label="Host" error={errors.imap_host?.message}>
                <Input id={`${fieldPrefix}-imap-host`} {...register('imap_host')} />
              </Field>
              <div className="grid gap-4 sm:grid-cols-[120px_1fr]">
                <Field id={`${fieldPrefix}-imap-port`} label="Port" error={errors.imap_port?.message}>
                  <Input id={`${fieldPrefix}-imap-port`} type="number" {...register('imap_port')} />
                </Field>
                <Field id={`${fieldPrefix}-imap-auth`} label="Auth scheme" error={errors.imap_auth_scheme?.message}>
                  <Select id={`${fieldPrefix}-imap-auth`} {...register('imap_auth_scheme')}>
                    <AuthSchemeOptions />
                  </Select>
                </Field>
              </div>
              <Field id={`${fieldPrefix}-imap-username`} label="Username" error={errors.imap_username?.message}>
                <Input id={`${fieldPrefix}-imap-username`} autoComplete="username" {...register('imap_username')} />
              </Field>
              <Field id={`${fieldPrefix}-imap-password`} label="Password" error={errors.imap_password?.message}>
                <Input
                  id={`${fieldPrefix}-imap-password`}
                  type="password"
                  autoComplete="current-password"
                  {...register('imap_password')}
                />
              </Field>
            </section>

            <section className="flex flex-col gap-4 rounded-md border border-border p-4">
              <h3 className="text-sm font-semibold">SMTP</h3>
              <Field id={`${fieldPrefix}-smtp-host`} label="Host" error={errors.smtp_host?.message}>
                <Input id={`${fieldPrefix}-smtp-host`} {...register('smtp_host')} />
              </Field>
              <div className="grid gap-4 sm:grid-cols-[120px_1fr]">
                <Field id={`${fieldPrefix}-smtp-port`} label="Port" error={errors.smtp_port?.message}>
                  <Input id={`${fieldPrefix}-smtp-port`} type="number" {...register('smtp_port')} />
                </Field>
                <Field id={`${fieldPrefix}-smtp-auth`} label="Auth scheme" error={errors.smtp_auth_scheme?.message}>
                  <Select id={`${fieldPrefix}-smtp-auth`} {...register('smtp_auth_scheme')}>
                    <AuthSchemeOptions />
                  </Select>
                </Field>
              </div>
              <Field id={`${fieldPrefix}-smtp-username`} label="Username" error={errors.smtp_username?.message}>
                <Input id={`${fieldPrefix}-smtp-username`} autoComplete="username" {...register('smtp_username')} />
              </Field>
              <Field id={`${fieldPrefix}-smtp-password`} label="Password" error={errors.smtp_password?.message}>
                <Input
                  id={`${fieldPrefix}-smtp-password`}
                  type="password"
                  autoComplete="current-password"
                  {...register('smtp_password')}
                />
              </Field>
            </section>
          </div>

          <Field id={`${fieldPrefix}-body-sync-mode`} label="Body sync mode" error={errors.body_sync_mode?.message}>
            <Select id={`${fieldPrefix}-body-sync-mode`} {...register('body_sync_mode')}>
              <option value="lazy">Load on open</option>
              <option value="full">Download during sync</option>
            </Select>
          </Field>

          {createAccount.error ? (
            <p className="text-sm text-destructive">Account setup failed. Check the server details and credentials.</p>
          ) : null}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={createAccount.isPending}>
              {createAccount.isPending ? 'Testing connection...' : 'Add account'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function AuthSchemeOptions() {
  return (
    <>
      <option value="plain">Plain</option>
      <option value="login">Login</option>
      <option value="cram-md5">CRAM-MD5</option>
      <option value="oauth2">OAuth2</option>
      <option value="xoauth2">XOAUTH2</option>
    </>
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
