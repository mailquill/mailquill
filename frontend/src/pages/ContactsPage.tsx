import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useOutletContext, useSearchParams } from 'react-router-dom'
import { Search, Star, Mail, Phone, Building2, Trash2, Plus, UserRound } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { accountColor, accountInitials } from '@/shared/lib/avatar'
import { Input } from '@/shared/components/ui/input'
import { Button } from '@/shared/components/ui/button'
import { Label } from '@/shared/components/ui/label'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { DavSyncButton } from '@/widgets/DavSyncButton'
import { useContacts, useCreateContact, useDeleteContact } from '@/shared/hooks/useContacts'
import { useModuleNav } from '@/shared/hooks/useModuleNav'
import type { Contact } from '@/shared/types'
import type { MailOutletContext } from './MailLayout'

export function ContactsPage() {
  const { t } = useTranslation()
  const { data: contacts = [], isLoading } = useContacts()
  const { contactGroup } = useModuleNav()
  const [search, setSearch] = useState('')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [searchParams, setSearchParams] = useSearchParams()
  const addOpen = searchParams.get('new') === '1'
  const openAdd = () => setSearchParams({ new: '1' })
  const closeAdd = () => setSearchParams({}, { replace: true })

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    return contacts
      .filter((c) => {
        if (contactGroup === 'fav') return c.favorite
        if (contactGroup !== 'all') return c.group_name === contactGroup
        return true
      })
      .filter((c) => !q || `${c.display_name} ${c.email ?? ''} ${c.company ?? ''}`.toLowerCase().includes(q))
      .sort((a, b) => a.display_name.localeCompare(b.display_name))
  }, [contacts, contactGroup, search])

  const sections = useMemo(() => groupByInitial(filtered), [filtered])
  const selected = contacts.find((c) => c.id === selectedId) ?? filtered[0] ?? null

  return (
    <section className="grid h-full min-h-0 grid-cols-[minmax(300px,380px)_1fr]">
      <div className="flex min-h-0 flex-col border-r border-border bg-card">
        <header className="flex items-center gap-2 border-b border-border px-4 py-3">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground" />
            <Input value={search} onChange={(e) => setSearch(e.currentTarget.value)} placeholder={t('contacts.search')} className="pl-9" />
          </div>
          <DavSyncButton />
          <Button size="icon" variant="outline" title={t('contacts.newContact')} onClick={openAdd}>
            <Plus className="size-4" />
          </Button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {isLoading ? (
            <p className="p-6 text-center text-sm text-muted-foreground">…</p>
          ) : filtered.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">{t('contacts.noContacts')}</p>
          ) : (
            sections.map(([letter, items]) => (
              <div key={letter}>
                <div className="sticky top-0 bg-secondary/70 px-4 py-1 text-[11px] font-bold uppercase tracking-wide text-muted-foreground backdrop-blur">
                  {letter}
                </div>
                {items.map((c) => (
                  <button
                    key={c.id}
                    onClick={() => setSelectedId(c.id)}
                    className={cn(
                      'flex w-full items-center gap-3 border-b border-secondary px-4 py-2.5 text-left transition-colors',
                      selected?.id === c.id ? 'bg-[var(--mq-row-open)]' : 'hover:bg-secondary/60',
                    )}
                  >
                    <Avatar contact={c} size={36} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13.5px] font-semibold text-foreground">{c.display_name}</div>
                      <div className="truncate text-[12px] text-muted-foreground">{c.email ?? c.company ?? ''}</div>
                    </div>
                    {c.favorite && <Star className="size-3.5 shrink-0 fill-primary text-primary" />}
                  </button>
                ))}
              </div>
            ))
          )}
        </div>
      </div>

      {selected ? <ContactDetail contact={selected} onDeleted={() => setSelectedId(null)} /> : <ContactsEmpty />}

      <AddContactDialog open={addOpen} onClose={closeAdd} />
    </section>
  )
}

function ContactsEmpty() {
  const { t } = useTranslation()
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 bg-background text-center text-muted-foreground">
      <UserRound className="size-10" />
      <p className="text-[15px] font-semibold">{t('contacts.selectContact')}</p>
    </div>
  )
}

function ContactDetail({ contact, onDeleted }: { contact: Contact; onDeleted: () => void }) {
  const { t } = useTranslation()
  const { openCompose } = useOutletContext<MailOutletContext>()
  const deleteContact = useDeleteContact()

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-background p-8">
      <div className="flex items-center gap-5">
        <Avatar contact={contact} size={84} className="text-[30px]" />
        <div className="min-w-0">
          <h1 className="text-[24px] font-bold tracking-tight">{contact.display_name}</h1>
          {(contact.job_title || contact.company) && (
            <p className="mt-1 text-[14px] text-muted-foreground">
              {[contact.job_title, contact.company].filter(Boolean).join(' · ')}
            </p>
          )}
        </div>
      </div>

      <div className="mt-6 flex gap-2.5">
        {contact.email && (
          <Button onClick={() => openCompose({ mode: 'new', to: [contact.email!] })}>
            <Mail className="size-4" />
            {t('action.sendEmail')}
          </Button>
        )}
        <Button
          variant="outline"
          className="text-destructive"
          onClick={() => window.confirm(`Delete ${contact.display_name}?`) && deleteContact.mutate(contact.id, { onSuccess: onDeleted })}
        >
          <Trash2 className="size-4" />
          {t('action.delete')}
        </Button>
      </div>

      <div className="mt-7 max-w-xl divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
        {contact.email && <DetailRow icon={Mail} label={t('contacts.email')} value={contact.email} />}
        {contact.phone && <DetailRow icon={Phone} label={t('contacts.phone')} value={contact.phone} />}
        {contact.company && <DetailRow icon={Building2} label={t('contacts.company')} value={contact.company} />}
        {contact.group_name && <DetailRow icon={UserRound} label={t('contacts.group')} value={contact.group_name} />}
      </div>

      {contact.notes && <p className="mt-5 max-w-xl whitespace-pre-wrap text-[13.5px] text-secondary-foreground">{contact.notes}</p>}
    </div>
  )
}

function DetailRow({ icon: Icon, label, value }: { icon: typeof Mail; label: string; value: string }) {
  return (
    <div className="flex items-center gap-3 px-4 py-3">
      <Icon className="size-4 text-muted-foreground" />
      <div className="min-w-0">
        <div className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{label}</div>
        <div className="truncate text-[14px] text-foreground">{value}</div>
      </div>
    </div>
  )
}

function Avatar({ contact, size, className }: { contact: Contact; size: number; className?: string }) {
  return (
    <span
      className={cn('flex shrink-0 items-center justify-center rounded-full font-bold uppercase text-white', className)}
      style={{ width: size, height: size, backgroundColor: accountColor(contact.id), fontSize: size / 2.6 }}
    >
      {accountInitials(contact.display_name)}
    </span>
  )
}

function AddContactDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation()
  const createContact = useCreateContact()
  const [form, setForm] = useState({ display_name: '', email: '', phone: '', company: '', job_title: '', group_name: '', favorite: false })

  function set<K extends keyof typeof form>(key: K, value: (typeof form)[K]) {
    setForm((f) => ({ ...f, [key]: value }))
  }

  function submit() {
    if (!form.display_name.trim()) return
    createContact.mutate(
      { ...form, email: form.email || null, phone: form.phone || null, company: form.company || null, job_title: form.job_title || null, group_name: form.group_name || null },
      {
        onSuccess: () => {
          setForm({ display_name: '', email: '', phone: '', company: '', job_title: '', group_name: '', favorite: false })
          onClose()
        },
      },
    )
  }

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(520px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>{t('contacts.newContact')}</DialogTitle>
        </DialogHeader>
        <div className="grid gap-3 sm:grid-cols-2">
          <FormField label={t('contacts.name')} className="sm:col-span-2">
            <Input value={form.display_name} onChange={(e) => set('display_name', e.currentTarget.value)} />
          </FormField>
          <FormField label={t('contacts.email')}>
            <Input type="email" value={form.email} onChange={(e) => set('email', e.currentTarget.value)} />
          </FormField>
          <FormField label={t('contacts.phone')}>
            <Input value={form.phone} onChange={(e) => set('phone', e.currentTarget.value)} />
          </FormField>
          <FormField label={t('contacts.company')}>
            <Input value={form.company} onChange={(e) => set('company', e.currentTarget.value)} />
          </FormField>
          <FormField label={t('contacts.jobTitle')}>
            <Input value={form.job_title} onChange={(e) => set('job_title', e.currentTarget.value)} />
          </FormField>
          <FormField label={t('contacts.group')}>
            <Input value={form.group_name} onChange={(e) => set('group_name', e.currentTarget.value)} />
          </FormField>
          <label className="flex items-center gap-2 self-end text-[13px]">
            <input type="checkbox" checked={form.favorite} onChange={(e) => set('favorite', e.currentTarget.checked)} className="size-4 accent-primary" />
            {t('contacts.favourite')}
          </label>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            {t('action.cancel')}
          </Button>
          <Button onClick={submit} disabled={createContact.isPending || !form.display_name.trim()}>
            {createContact.isPending ? t('settings.saving') : t('contacts.addContact')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function FormField({ label, className, children }: { label: string; className?: string; children: React.ReactNode }) {
  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      <Label className="text-[12px] font-semibold">{label}</Label>
      {children}
    </div>
  )
}

function groupByInitial(contacts: Contact[]): [string, Contact[]][] {
  const map = new Map<string, Contact[]>()
  for (const c of contacts) {
    const letter = (c.display_name.trim()[0] ?? '#').toUpperCase()
    const key = /[A-Z]/.test(letter) ? letter : '#'
    if (!map.has(key)) map.set(key, [])
    map.get(key)!.push(c)
  }
  return Array.from(map.entries()).sort(([a], [b]) => a.localeCompare(b))
}
