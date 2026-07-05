import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Download, KeyRound, LockKeyhole, Plus, ShieldCheck, Trash2, Upload } from 'lucide-react'
import { Button } from '@/shared/components/ui/button'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Textarea } from '@/shared/components/ui/textarea'
import { Badge } from '@/shared/components/ui/badge'
import { cn } from '@/shared/lib/utils'
import { apiGet } from '@/shared/api'
import {
  useCreateContactKey,
  useCreatePgpKey,
  useDeletePgpKey,
  usePgpKeys,
  useSetPrimaryPgpKey,
  type PgpKey,
} from '@/shared/hooks/usePgp'
import {
  cacheUnlockedKey,
  clearUnlockedKey,
  generatePgpKey,
  getUnlockedKey,
  importPgpPrivateKey,
  unlockPrivateKey,
} from '@/shared/lib/pgpCrypto'

type Mode = 'generate' | 'import'

export function PgpKeyManagement() {
  const { t } = useTranslation()
  const { data: keys = [] } = usePgpKeys()
  const createKey = useCreatePgpKey()
  const deleteKey = useDeletePgpKey()
  const setPrimary = useSetPrimaryPgpKey()
  const createContactKey = useCreateContactKey()
  const [mode, setMode] = useState<Mode>('generate')
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [passphrase, setPassphrase] = useState('')
  const [privateKey, setPrivateKey] = useState('')
  const [contactEmail, setContactEmail] = useState('')
  const [contactPublicKey, setContactPublicKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const primary = useMemo(() => keys.find((key) => key.is_primary) ?? keys[0], [keys])

  async function submitKey() {
    setBusy(true)
    setMessage(null)
    try {
      const generated =
        mode === 'generate'
          ? await generatePgpKey(name.trim(), email.trim(), passphrase)
          : await importPgpPrivateKey(privateKey.trim(), passphrase)
      await createKey.mutateAsync({
        fingerprint: generated.fingerprint,
        uid: generated.uid,
        public_key_armored: generated.publicKeyArmored,
        private_key_encrypted_blob: generated.privateKeyEncryptedBlob,
        is_primary: keys.length === 0,
      })
      if (mode === 'generate') {
        const unlocked = await unlockPrivateKey(
          generated.fingerprint,
          generated.privateKeyEncryptedBlob,
          passphrase,
        )
        cacheUnlockedKey(unlocked)
      }
      setName('')
      setEmail('')
      setPassphrase('')
      setPrivateKey('')
      setMessage(t('pgp.keySaved'))
    } catch (err) {
      setMessage(t(errorKey(err)))
    } finally {
      setBusy(false)
    }
  }

  async function unlock(key: PgpKey) {
    const phrase = window.prompt(t('pgp.unlockPrompt'))
    if (!phrase) return
    setBusy(true)
    setMessage(null)
    try {
      const blob = await apiGet<{ private_key_encrypted_blob: string }>(`/pgp-keys/${key.id}/blob`)
      await unlockPrivateKey(key.fingerprint, blob.private_key_encrypted_blob, phrase)
      setMessage(t('pgp.unlocked'))
    } catch {
      setMessage(t('pgp.unlockFailed'))
    } finally {
      setBusy(false)
    }
  }

  async function exportPrivate(key: PgpKey) {
    const cached = getUnlockedKey(key.fingerprint)
    if (!cached) {
      await unlock(key)
      return
    }
    downloadText(`${key.fingerprint}.private.asc`, cached.privateKeyArmored)
  }

  async function importContact() {
    if (!contactEmail.trim() || !contactPublicKey.trim()) return
    await createContactKey.mutateAsync({
      email: contactEmail.trim(),
      public_key_data: contactPublicKey.trim(),
    })
    setContactEmail('')
    setContactPublicKey('')
    setMessage(t('pgp.contactSaved'))
  }

  function remove(key: PgpKey) {
    if (!window.confirm(t('pgp.deleteWarning'))) return
    clearUnlockedKey(key.fingerprint)
    deleteKey.mutate(key.id)
  }

  return (
    <section className="flex flex-col gap-4 rounded-lg border border-border bg-card p-4">
      <div className="flex items-center gap-2">
        <KeyRound className="size-4 text-muted-foreground" aria-hidden="true" />
        <h3 className="text-sm font-bold">{t('pgp.title')}</h3>
        {primary && <Badge variant="secondary">{t('pgp.primaryReady')}</Badge>}
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <div className="rounded-md border border-border p-3">
          <div className="mb-3 inline-flex gap-0.5 rounded-md bg-secondary p-0.5">
            {(['generate', 'import'] as Mode[]).map((item) => (
              <button
                key={item}
                type="button"
                onClick={() => setMode(item)}
                className={cn(
                  'h-8 rounded px-3 text-xs font-semibold transition-colors',
                  mode === item ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
                )}
              >
                {t(`pgp.${item}`)}
              </button>
            ))}
          </div>
          {mode === 'generate' ? (
            <div className="grid gap-2">
              <Field id="pgp-name" label={t('pgp.name')}>
                <Input id="pgp-name" value={name} onChange={(e) => setName(e.currentTarget.value)} />
              </Field>
              <Field id="pgp-email" label={t('pgp.email')}>
                <Input id="pgp-email" value={email} onChange={(e) => setEmail(e.currentTarget.value)} />
              </Field>
            </div>
          ) : (
            <Field id="pgp-private-key" label={t('pgp.privateKey')}>
              <Textarea
                id="pgp-private-key"
                value={privateKey}
                onChange={(e) => setPrivateKey(e.currentTarget.value)}
                className="min-h-32 font-mono"
              />
            </Field>
          )}
          <Field id="pgp-passphrase" label={t('pgp.passphrase')}>
            <Input
              id="pgp-passphrase"
              type="password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.currentTarget.value)}
            />
          </Field>
          <Button
            type="button"
            className="mt-3"
            onClick={submitKey}
            disabled={busy || createKey.isPending || passphrase.length < 12 || (mode === 'generate' && (!name || !email))}
          >
            {mode === 'generate' ? <Plus className="size-4" /> : <Upload className="size-4" />}
            {mode === 'generate' ? t('pgp.generateKey') : t('pgp.importKey')}
          </Button>
        </div>

        <div className="rounded-md border border-border p-3">
          <h4 className="mb-2 text-xs font-semibold uppercase tracking-widest text-muted-foreground">
            {t('pgp.contactKeys')}
          </h4>
          <div className="grid gap-2">
            <Field id="pgp-contact-email" label={t('pgp.contactEmail')}>
              <Input
                id="pgp-contact-email"
                value={contactEmail}
                onChange={(e) => setContactEmail(e.currentTarget.value)}
              />
            </Field>
            <Field id="pgp-contact-public-key" label={t('pgp.publicKey')}>
              <Textarea
                id="pgp-contact-public-key"
                value={contactPublicKey}
                onChange={(e) => setContactPublicKey(e.currentTarget.value)}
                className="min-h-28 font-mono"
              />
            </Field>
            <Button type="button" variant="outline" onClick={importContact} disabled={createContactKey.isPending}>
              <ShieldCheck className="size-4" />
              {t('pgp.saveContactKey')}
            </Button>
          </div>
        </div>
      </div>

      <div className="overflow-hidden rounded-md border border-border">
        <div className="grid grid-cols-[1fr_130px_220px] gap-3 bg-muted/30 px-3 py-2 text-xs font-semibold uppercase tracking-widest text-muted-foreground">
          <span>{t('pgp.key')}</span>
          <span>{t('pgp.status')}</span>
          <span>{t('pgp.actions')}</span>
        </div>
        {keys.map((key) => (
          <div key={key.id} className="grid grid-cols-[1fr_130px_220px] gap-3 border-t border-border px-3 py-2 text-sm">
            <span className="min-w-0">
              <span className="block truncate font-medium">{key.uid}</span>
              <span className="block truncate font-mono text-xs text-muted-foreground">{key.fingerprint}</span>
            </span>
            <span>{key.is_primary ? <Badge>{t('pgp.primary')}</Badge> : <Badge variant="outline">{t('pgp.secondary')}</Badge>}</span>
            <span className="flex flex-wrap gap-1.5">
              <IconButton label={t('pgp.unlock')} onClick={() => unlock(key)} disabled={busy}>
                <LockKeyhole className="size-3.5" />
              </IconButton>
              <IconButton label={t('pgp.setPrimary')} onClick={() => setPrimary.mutate(key.id)} disabled={key.is_primary}>
                <ShieldCheck className="size-3.5" />
              </IconButton>
              <IconButton label={t('pgp.exportPublic')} onClick={() => downloadText(`${key.fingerprint}.public.asc`, key.public_key_armored)}>
                <Download className="size-3.5" />
              </IconButton>
              <IconButton label={t('pgp.exportPrivate')} onClick={() => exportPrivate(key)}>
                <KeyRound className="size-3.5" />
              </IconButton>
              <IconButton label={t('action.delete')} onClick={() => remove(key)} danger>
                <Trash2 className="size-3.5" />
              </IconButton>
            </span>
          </div>
        ))}
        {!keys.length && <div className="px-3 py-5 text-sm text-muted-foreground">{t('pgp.noKeys')}</div>}
      </div>
      {message && <p className="text-sm text-muted-foreground">{message}</p>}
    </section>
  )
}

function Field({ id, label, children }: { id: string; label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={id} className="text-xs font-semibold text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  )
}

function IconButton({
  label,
  onClick,
  children,
  disabled,
  danger,
}: {
  label: string
  onClick: () => void
  children: React.ReactNode
  disabled?: boolean
  danger?: boolean
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'flex size-8 items-center justify-center rounded-md border border-border text-secondary-foreground transition-colors hover:bg-secondary disabled:opacity-50',
        danger && 'text-destructive',
      )}
    >
      {children}
    </button>
  )
}

function downloadText(filename: string, text: string) {
  const blob = new Blob([text], { type: 'application/pgp-keys' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

function errorKey(err: unknown): string {
  return err instanceof Error && err.message.startsWith('pgp.') ? err.message : 'pgp.keyFailed'
}
