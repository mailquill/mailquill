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
  email: z.string().email('auth.emailInvalid'),
  password: z.string().min(1, 'auth.passwordRequired'),
  remember: z.boolean(),
})
type FormData = z.infer<typeof schema>

export function LoginPage() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const setAuth = useAuthStore((s) => s.setAuth)
  const { data: publicConfig } = usePublicConfig()

  const { register, handleSubmit, formState: { errors } } = useForm<FormData>({
    resolver: zodResolver(schema),
    defaultValues: { remember: false },
  })

  const loginMutation = useMutation({
    mutationFn: (data: FormData) =>
      apiPost<{ access_token: string; user_id: string; email: string }>('/auth/login', data),
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
          <p className="text-sm text-muted-foreground">{t('auth.signInTitle')}</p>
        </div>

        <form onSubmit={handleSubmit((d) => loginMutation.mutate(d))} className="space-y-4">
          <div className="space-y-1">
            <Label htmlFor="email">{t('auth.email')}</Label>
            <Input id="email" type="email" autoComplete="email" {...register('email')} />
            {errors.email && <p className="text-xs text-destructive">{t(errors.email.message ?? '')}</p>}
          </div>

          <div className="space-y-1">
            <Label htmlFor="password">{t('auth.password')}</Label>
            <Input id="password" type="password" autoComplete="current-password" {...register('password')} />
            {errors.password && <p className="text-xs text-destructive">{t(errors.password.message ?? '')}</p>}
          </div>

          <label className="flex cursor-pointer items-center gap-2 text-sm text-secondary-foreground">
            <input type="checkbox" className="size-4 accent-primary" {...register('remember')} />
            {t('auth.rememberMe')}
          </label>

          {loginMutation.error && (
            <p className="text-xs text-destructive">
              {loginMutation.error instanceof ApiError && loginMutation.error.status === 401
                ? t('auth.invalidCredentials')
                : t('auth.loginFailed')}
            </p>
          )}

          <Button type="submit" className="w-full" disabled={loginMutation.isPending}>
            {loginMutation.isPending ? t('auth.signingIn') : t('auth.signIn')}
          </Button>
        </form>

        {publicConfig?.registration_enabled !== false && (
          <p className="text-center text-sm text-muted-foreground">
            {t('auth.noAccount')}{' '}
            <Link to="/register" className="text-primary underline underline-offset-4">
              {t('auth.register')}
            </Link>
          </p>
        )}
      </div>
    </div>
  )
}
