import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { useNavigate, Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useMutation } from '@tanstack/react-query'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { useAuthStore } from '@/app/store'
import { apiPost, ApiError } from '@/shared/api'
import { usePublicConfig } from '@/shared/hooks/useSettings'

const schema = z.object({
  email: z.string().email(),
  password: z.string().min(8, 'auth.passwordMin'),
  confirmPassword: z.string(),
}).refine((d) => d.password === d.confirmPassword, {
  message: 'auth.passwordsNoMatch',
  path: ['confirmPassword'],
})
type FormData = z.infer<typeof schema>

export function RegisterPage() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const setAuth = useAuthStore((s) => s.setAuth)
  const { data: publicConfig } = usePublicConfig()
  const registrationDisabled = publicConfig?.registration_enabled === false

  const { register, handleSubmit, formState: { errors } } = useForm<FormData>({
    resolver: zodResolver(schema),
  })

  const registerMutation = useMutation({
    mutationFn: (data: FormData) =>
      apiPost<{ access_token: string; user_id: string; email: string }>('/auth/register', {
        email: data.email,
        password: data.password,
      }),
    onSuccess: (data) => {
      setAuth(data.access_token, data.user_id, data.email)
      navigate('/mail/unified')
    },
  })

  return (
    <div className="flex min-h-screen items-center justify-center bg-background">
      <div className="w-full max-w-sm space-y-6 rounded-xl border border-border bg-card p-8 shadow-sm">
        <div className="space-y-1 text-center">
          <h1 className="text-2xl font-bold">Mailquill</h1>
          <p className="text-sm text-muted-foreground">{t('auth.createTitle')}</p>
        </div>

        {registrationDisabled && (
          <p className="rounded-md border border-border bg-secondary p-3 text-center text-sm text-secondary-foreground">
            {t('auth.registrationDisabled')}
          </p>
        )}

        {!registrationDisabled && (
        <form onSubmit={handleSubmit((d) => registerMutation.mutate(d))} className="space-y-4">
          <div className="space-y-1">
            <Label htmlFor="email">{t('auth.email')}</Label>
            <Input id="email" type="email" autoComplete="email" {...register('email')} />
            {errors.email && <p className="text-xs text-destructive">{t(errors.email.message ?? '')}</p>}
          </div>

          <div className="space-y-1">
            <Label htmlFor="password">{t('auth.password')}</Label>
            <Input id="password" type="password" autoComplete="new-password" {...register('password')} />
            {errors.password && <p className="text-xs text-destructive">{t(errors.password.message ?? '')}</p>}
          </div>

          <div className="space-y-1">
            <Label htmlFor="confirmPassword">{t('auth.confirmPassword')}</Label>
            <Input id="confirmPassword" type="password" autoComplete="new-password" {...register('confirmPassword')} />
            {errors.confirmPassword && <p className="text-xs text-destructive">{t(errors.confirmPassword.message ?? '')}</p>}
          </div>

          {registerMutation.error && (
            <p className="text-xs text-destructive">
              {registerMutation.error instanceof ApiError && registerMutation.error.status === 403
                ? t('auth.registrationDisabled')
                : t('auth.registerFailed')}
            </p>
          )}

          <Button type="submit" className="w-full" disabled={registerMutation.isPending}>
            {registerMutation.isPending ? t('auth.creatingAccount') : t('auth.createAccount')}
          </Button>
        </form>
        )}

        <p className="text-center text-sm text-muted-foreground">
          {t('auth.haveAccount')}{' '}
          <Link to="/login" className="text-primary underline underline-offset-4">
            {t('auth.signIn')}
          </Link>
        </p>
      </div>
    </div>
  )
}
