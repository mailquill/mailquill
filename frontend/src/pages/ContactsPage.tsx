import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useOutletContext, useSearchParams } from 'react-router-dom'
import { Building2, Edit3, Mail, MapPin, Phone, Plus, Search, Trash2, UserRound, Users } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { accountColor, accountInitials } from '@/shared/lib/avatar'
import { Input } from '@/shared/components/ui/input'
import { Button } from '@/shared/components/ui/button'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { Textarea } from '@/shared/components/ui/textarea'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/shared/components/ui/dialog'
import { Badge } from '@/shared/components/ui/badge'
import {
  useContactAccounts,
  useContacts,
  useCreateContact,
  useCreateContactAccount,
  useDeleteContact,
  useUpdateContact,
  useContactBooks,
} from '@/shared/hooks/useContacts'
import { useAccounts, useEnableMailboxContacts, useDiscoverMailboxContacts } from '@/shared/hooks/useAccounts'
import { startOAuthRedirect } from '@/shared/lib/oauth'
import { useModuleNav } from '@/shared/hooks/useModuleNav'
import type { Contact, ContactAccount, LabeledValue, NewContact, PostalAddress } from '@/shared/types'
import type { MailOutletContext } from './MailLayout'

type ContactDraft = {
  account_id: string
  book_id: string
  display_name: string
  given_name: string
  family_name: string
  org: string
  title: string
  email: string
  email_label: string
  phone: string
  phone_label: string
  street: string
  city: string
  region: string
  postal_code: string
  country: string
  notes: string
}

const EMPTY_DRAFT: ContactDraft = {
  account_id: '',
  book_id: '',
  display_name: '',
  given_name: '',
  family_name: '',
  org: '',
  title: '',
  email: '',
  email_label: 'work',
  phone: '',
  phone_label: 'mobile',
  street: '',
  city: '',
  region: '',
  postal_code: '',
  country: '',
  notes: '',
}

export function ContactsPage() {
  const { t } = useTranslation()
  const { contactGroup } = useModuleNav()
  const { data: accounts = [] } = useContactAccounts()
  const { data: mailboxes = [] } = useAccounts()
  const [search, setSearch] = useState('')
  const groupFilter = contactGroup.startsWith('group:') ? contactGroup.slice('group:'.length) : undefined
  const accountFilter = contactGroup === 'all' || contactGroup === 'fav' || groupFilter ? undefined : contactGroup
  const { data: contacts = [], isLoading } = useContacts(accountFilter, search, undefined, undefined, groupFilter)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [editing, setEditing] = useState<Contact | null>(null)
  const [accountDialogOpen, setAccountDialogOpen] = useState(false)
  const [searchParams, setSearchParams] = useSearchParams()
  const addOpen = searchParams.get('new') === '1'
  const openAdd = () => setSearchParams({ new: '1' })
  const closeAdd = () => setSearchParams({}, { replace: true })
  const sections = useMemo(() => groupByInitial(contacts), [contacts])
  const selected = contacts.find((c) => c.id === selectedId) ?? contacts[0] ?? null

  return (
    <section aria-label={t('contacts.contactsCapability')} className="grid h-full min-h-0 grid-cols-[minmax(320px,400px)_1fr]">
      <div className="flex min-h-0 flex-col border-r border-border bg-card">
        <header className="flex flex-col gap-3 border-b border-border px-4 py-3">
          <div className="flex items-center gap-2">
            <div className="relative flex-1">
              <Search className="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground" />
              <Input
                value={search}
                onChange={(event) => setSearch(event.currentTarget.value)}
                placeholder={t('contacts.search')}
                className="pl-9"
              />
            </div>
            <Button size="icon" variant="outline" title={t('contacts.newAccount')} onClick={() => setAccountDialogOpen(true)}>
              <Users className="size-4" />
            </Button>
            <Button size="icon" variant="outline" title={t('contacts.newContact')} onClick={openAdd}>
              <Plus className="size-4" />
            </Button>
          </div>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {isLoading ? (
            <p className="p-6 text-center text-sm text-muted-foreground">...</p>
          ) : contacts.length === 0 ? (
            <EligibleMailboxCards mailboxes={mailboxes} />
          ) : (
            sections.map(([letter, items]) => (
              <div key={letter}>
                <div className="sticky top-0 bg-secondary/70 px-4 py-1 text-[11px] font-bold uppercase tracking-wide text-muted-foreground backdrop-blur">
                  {letter}
                </div>
                {items.map((contact) => (
                  <button
                    key={contact.id}
                    onClick={() => setSelectedId(contact.id)}
                    className={cn(
                      'flex w-full items-center gap-3 border-b border-secondary px-4 py-2.5 text-left transition-colors',
                      selected?.id === contact.id ? 'bg-[var(--mq-row-open)]' : 'hover:bg-secondary/60',
                    )}
                  >
                    <Avatar contact={contact} size={36} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13.5px] font-semibold text-foreground">{contactName(contact)}</div>
                      <div className="truncate text-[12px] text-muted-foreground">{primaryEmail(contact) ?? contact.org ?? ''}</div>
                      {contact.groups.length > 0 && <div className="truncate text-[10.5px] text-muted-foreground">{contact.groups.map((group) => group.name).join(' · ')}</div>}
                    </div>
                  </button>
                ))}
              </div>
            ))
          )}
        </div>
      </div>

      {selected ? (
        <ContactDetail contact={selected} onEdit={() => setEditing(selected)} onDeleted={() => setSelectedId(null)} />
      ) : (
        <ContactsEmpty />
      )}

      <ContactDialog key={`add-${addOpen}`} open={addOpen} accounts={accounts} onClose={closeAdd} />
      <ContactDialog key={`edit-${editing?.id ?? 'closed'}`} open={Boolean(editing)} accounts={accounts} contact={editing} onClose={() => setEditing(null)} />
      <ContactAccountDialog open={accountDialogOpen} onClose={() => setAccountDialogOpen(false)} />
    </section>
  )
}

function EligibleMailboxCards({ mailboxes }: { mailboxes: import('@/shared/types').Account[] }) {
  const { t } = useTranslation()
  const enable = useEnableMailboxContacts()
  const discover = useDiscoverMailboxContacts()
  if (!mailboxes.length) return <p className="p-6 text-center text-sm text-muted-foreground">{t('contacts.noContacts')}</p>
  return (
    <div className="space-y-3 p-4" aria-live="polite">
      <div><h3 className="text-[14px] font-bold">{t('contacts.emptyTitle')}</h3><p className="text-[12px] text-muted-foreground">{t('contacts.emptyHelp')}</p></div>
      {mailboxes.map((mailbox) => {
        const capability = mailbox.contacts
        const action = capability?.state === 'consent_required' || capability?.state === 'reauth_required'
          ? () => startOAuthRedirect(capability.provider === 'google' ? 'google' : 'microsoft', mailbox.id, 'contacts')
          : capability?.provider === 'cardav' && (capability.state === 'error' || capability.state === 'unavailable')
            ? () => discover.mutate({ accountId: mailbox.id })
            : () => enable.mutate(mailbox.id)
        return (
          <div key={mailbox.id} className="rounded-lg border border-border p-3">
            <div className="text-[13px] font-bold">{mailbox.display_name}</div>
            <div className="mt-0.5 text-[11.5px] text-muted-foreground">{capability ? t(`contacts.state.${capability.state}`) : t('contacts.state.disabled')}</div>
            {capability?.reason === 'provider_configuration_required' ? (
              <div role="alert" className="mt-1 text-[11.5px] text-destructive">
                <span>{t('contacts.providerConfigurationRequired', {
                  provider: capability.provider === 'google' ? 'Google' : capability.provider === 'graph' ? 'Microsoft' : 'CardDAV',
                })}</span>{' '}
                {capability.provider === 'google' ? (
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
            <Button className="mt-2" size="sm" variant="outline" onClick={action} disabled={enable.isPending || discover.isPending}>
              {capability?.reason === 'provider_configuration_required' ? t('contacts.tryAgain') : capability?.state === 'consent_required' ? t('contacts.grantAccess') : capability?.state === 'reauth_required' ? t('contacts.reconnect') : capability?.state === 'error' ? t('contacts.fixContacts') : t('contacts.enable')}
            </Button>
          </div>
        )
      })}
    </div>
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

function ContactDetail({ contact, onEdit, onDeleted }: { contact: Contact; onEdit: () => void; onDeleted: () => void }) {
  const { t } = useTranslation()
  const { openCompose } = useOutletContext<MailOutletContext>()
  const deleteContact = useDeleteContact()
  const email = primaryEmail(contact)

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-background p-8">
      <div className="flex items-center gap-5">
        <Avatar contact={contact} size={84} className="text-[30px]" />
        <div className="min-w-0">
          <h1 className="text-[24px] font-bold tracking-tight">{contactName(contact)}</h1>
          <Badge variant="secondary" className="mt-2">
            {t(`contacts.providers.${contact.source_provider}`)}
          </Badge>
          {(contact.title || contact.org) && (
            <p className="mt-1 text-[14px] text-muted-foreground">
              {[contact.title, contact.org].filter(Boolean).join(' · ')}
            </p>
          )}
        </div>
      </div>

      <div className="mt-6 flex gap-2.5">
        {email && (
          <Button onClick={() => openCompose({ mode: 'new', to: [formatRecipient(contact, email)] })}>
            <Mail className="size-4" />
            {t('action.sendEmail')}
          </Button>
        )}
        <Button variant="outline" onClick={onEdit} disabled={!contact.source_writable}>
          <Edit3 className="size-4" />
          {t('action.edit')}
        </Button>
        <Button
          variant="outline"
          className="text-destructive"
          disabled={!contact.source_writable}
          onClick={() => window.confirm(t('contacts.deleteConfirm', { name: contactName(contact) })) && deleteContact.mutate(contact.id, { onSuccess: onDeleted })}
        >
          <Trash2 className="size-4" />
          {t('action.delete')}
        </Button>
      </div>
      {!contact.source_writable && (
        <p className="mt-3 rounded-md border border-border bg-secondary/50 px-3 py-2 text-[12px] text-muted-foreground">
          {contact.source_state === 'disabled' ? t('contacts.cacheReadOnly') : t('contacts.readOnlySource')}
        </p>
      )}

      <div className="mt-7 max-w-2xl divide-y divide-border overflow-hidden rounded-lg border border-border bg-card">
        {contact.emails.map((item) => (
          <DetailRow key={`email-${item.value}`} icon={Mail} label={item.label ?? t('contacts.email')} value={item.value} />
        ))}
        {contact.phones.map((item) => (
          <DetailRow key={`phone-${item.value}`} icon={Phone} label={item.label ?? t('contacts.phone')} value={item.value} />
        ))}
        {contact.addresses.map((item, index) => (
          <DetailRow key={`address-${index}`} icon={MapPin} label={item.label ?? t('contacts.address')} value={addressText(item)} />
        ))}
        {contact.org && <DetailRow icon={Building2} label={t('contacts.company')} value={contact.org} />}
        {contact.groups.length > 0 && <DetailRow icon={Users} label={t('contacts.groups')} value={contact.groups.map((group) => group.name).join(', ')} />}
      </div>

      {contact.notes && <p className="mt-5 max-w-2xl whitespace-pre-wrap text-[13.5px] text-secondary-foreground">{contact.notes}</p>}
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
  const name = contactName(contact)
  const [failed, setFailed] = useState(false)
  if (!failed && (contact.photo_reference || contact.photo_blob_key)) {
    return <img src={`/api/contacts/${contact.id}/photo`} alt="" onError={() => setFailed(true)} className={cn('shrink-0 rounded-full object-cover', className)} style={{ width: size, height: size }} />
  }
  return (
    <span
      className={cn('flex shrink-0 items-center justify-center rounded-full font-bold uppercase text-white', className)}
      style={{ width: size, height: size, backgroundColor: accountColor(contact.id), fontSize: size / 2.6 }}
    >
      {accountInitials(name)}
    </span>
  )
}

function ContactDialog({ open, accounts, contact, onClose }: { open: boolean; accounts: ContactAccount[]; contact?: Contact | null; onClose: () => void }) {
  const { t } = useTranslation()
  const createContact = useCreateContact()
  const updateContact = useUpdateContact()
  const firstWritableAccount = accounts.find((account) => account.enabled && !['disabled', 'consent_required', 'reauth_required', 'unavailable'].includes(account.capability_state))
  const [draft, setDraft] = useState<ContactDraft>(() => contactToDraft(contact, firstWritableAccount?.id ?? ''))
  const { data: books = [] } = useContactBooks(draft.account_id)

  function set<K extends keyof ContactDraft>(key: K, value: ContactDraft[K]) {
    setDraft((current) => ({ ...current, [key]: value }))
  }

  function submit() {
    const payload = draftToPayload(draft)
    if (!payload.account_id || (!payload.display_name && !payload.given_name && !payload.family_name)) return
    const options = {
      onSuccess: () => {
        setDraft(contactToDraft(null, firstWritableAccount?.id ?? ''))
        onClose()
      },
    }
    if (contact) {
      updateContact.mutate({ id: contact.id, data: payload }, options)
    } else {
      createContact.mutate(payload, options)
    }
  }

  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(680px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>{contact ? t('contacts.editContact') : t('contacts.newContact')}</DialogTitle>
        </DialogHeader>
        <div className="grid gap-3 sm:grid-cols-2">
          <FormField id="contact-account" label={t('contacts.account')}>
            <Select id="contact-account" value={draft.account_id} onChange={(event) => setDraft((current) => ({ ...current, account_id: event.currentTarget.value, book_id: '' }))}>
              <option value="">{t('contacts.chooseAccount')}</option>
              {accounts.filter((account) => account.enabled && !['disabled', 'consent_required', 'reauth_required', 'unavailable'].includes(account.capability_state)).map((account) => (
                <option key={account.id} value={account.id}>
                  {account.display_name}
                </option>
              ))}
            </Select>
          </FormField>
          <FormField id="contact-book" label={t('contacts.book')}>
            <Select id="contact-book" value={draft.book_id} onChange={(event) => set('book_id', event.currentTarget.value)} disabled={!draft.account_id}>
              <option value="">{t('contacts.defaultBook')}</option>
              {books.filter((book) => book.is_writable).map((book) => <option key={book.id} value={book.id}>{book.display_name}</option>)}
            </Select>
          </FormField>
          <FormField id="contact-display" label={t('contacts.name')}>
            <Input id="contact-display" value={draft.display_name} onChange={(event) => set('display_name', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-given" label={t('contacts.givenName')}>
            <Input id="contact-given" value={draft.given_name} onChange={(event) => set('given_name', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-family" label={t('contacts.familyName')}>
            <Input id="contact-family" value={draft.family_name} onChange={(event) => set('family_name', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-email" label={t('contacts.email')}>
            <Input id="contact-email" type="email" value={draft.email} onChange={(event) => set('email', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-email-label" label={t('contacts.label')}>
            <Input id="contact-email-label" value={draft.email_label} onChange={(event) => set('email_label', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-phone" label={t('contacts.phone')}>
            <Input id="contact-phone" value={draft.phone} onChange={(event) => set('phone', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-phone-label" label={t('contacts.label')}>
            <Input id="contact-phone-label" value={draft.phone_label} onChange={(event) => set('phone_label', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-org" label={t('contacts.company')}>
            <Input id="contact-org" value={draft.org} onChange={(event) => set('org', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-title" label={t('contacts.jobTitle')}>
            <Input id="contact-title" value={draft.title} onChange={(event) => set('title', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-street" label={t('contacts.address')} className="sm:col-span-2">
            <Input id="contact-street" value={draft.street} onChange={(event) => set('street', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-city" label={t('contacts.city')}>
            <Input id="contact-city" value={draft.city} onChange={(event) => set('city', event.currentTarget.value)} />
          </FormField>
          <FormField id="contact-notes" label={t('contacts.notes')} className="sm:col-span-2">
            <Textarea id="contact-notes" value={draft.notes} onChange={(event) => set('notes', event.currentTarget.value)} />
          </FormField>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t('action.cancel')}</Button>
          <Button onClick={submit} disabled={createContact.isPending || updateContact.isPending || !draft.account_id}>
            {createContact.isPending || updateContact.isPending ? t('settings.saving') : t('action.save')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function ContactAccountDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation()
  const createAccount = useCreateContactAccount()
  const [form, setForm] = useState({ display_name: '', base_url: '', username: '', password: '' })
  function submit() {
    createAccount.mutate(
      {
        display_name: form.display_name,
        type: 'cardav',
        base_url: form.base_url || null,
        auth_scheme: 'basic',
        username: form.username || null,
        password: form.password || null,
      },
      { onSuccess: onClose },
    )
  }
  return (
    <Dialog open={open} onClose={onClose}>
      <DialogContent className="w-[min(520px,calc(100vw-2rem))] max-w-none">
        <DialogHeader>
          <DialogTitle>{t('contacts.newAccount')}</DialogTitle>
        </DialogHeader>
        <p className="mb-4 text-[12.5px] text-muted-foreground">{t('contacts.independentCarddavHelp')}</p>
        <div className="grid gap-3">
          <FormField id="contact-account-name" label={t('contacts.accountName')}>
            <Input id="contact-account-name" value={form.display_name} onChange={(event) => setForm({ ...form, display_name: event.currentTarget.value })} />
          </FormField>
          <FormField id="contact-account-url" label={t('contacts.baseUrl')}>
            <Input id="contact-account-url" value={form.base_url} onChange={(event) => setForm({ ...form, base_url: event.currentTarget.value })} />
          </FormField>
          <FormField id="contact-account-user" label={t('settings.username')}>
            <Input id="contact-account-user" value={form.username} onChange={(event) => setForm({ ...form, username: event.currentTarget.value })} />
          </FormField>
          <FormField id="contact-account-password" label={t('settings.password')}>
            <Input id="contact-account-password" type="password" value={form.password} onChange={(event) => setForm({ ...form, password: event.currentTarget.value })} />
          </FormField>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t('action.cancel')}</Button>
          <Button onClick={submit} disabled={createAccount.isPending || !form.display_name.trim()}>
            {createAccount.isPending ? t('settings.saving') : t('action.save')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function FormField({ id, label, className, children }: { id: string; label: string; className?: string; children: React.ReactNode }) {
  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      <Label htmlFor={id} className="text-[12px] font-semibold">{label}</Label>
      {children}
    </div>
  )
}

function contactName(contact: Contact): string {
  return contact.display_name || [contact.given_name, contact.family_name].filter(Boolean).join(' ') || primaryEmail(contact) || 'Contact'
}

function primaryEmail(contact: Contact): string | null {
  return contact.emails[0]?.value ?? null
}

function formatRecipient(contact: Contact, email: string): string {
  return `${contactName(contact)} <${email}>`
}

function addressText(address: PostalAddress): string {
  return [address.street, address.locality, address.region, address.postal_code, address.country].filter(Boolean).join(', ')
}

function contactToDraft(contact: Contact | null | undefined, accountId: string): ContactDraft {
  if (!contact) return { ...EMPTY_DRAFT, account_id: accountId }
  const email = contact.emails[0]
  const phone = contact.phones[0]
  const address = contact.addresses[0]
  return {
    account_id: contact.account_id,
    book_id: contact.book_id ?? '',
    display_name: contact.display_name ?? '',
    given_name: contact.given_name ?? '',
    family_name: contact.family_name ?? '',
    org: contact.org ?? '',
    title: contact.title ?? '',
    email: email?.value ?? '',
    email_label: email?.label ?? 'work',
    phone: phone?.value ?? '',
    phone_label: phone?.label ?? 'mobile',
    street: address?.street ?? '',
    city: address?.locality ?? '',
    region: address?.region ?? '',
    postal_code: address?.postal_code ?? '',
    country: address?.country ?? '',
    notes: contact.notes ?? '',
  }
}

function draftToPayload(draft: ContactDraft): NewContact {
  const emails: LabeledValue[] = draft.email ? [{ label: draft.email_label || null, value: draft.email, primary: true }] : []
  const phones: LabeledValue[] = draft.phone ? [{ label: draft.phone_label || null, value: draft.phone, primary: true }] : []
  const addresses: PostalAddress[] = draft.street || draft.city ? [{
    label: null,
    primary: true,
    street: draft.street || null,
    locality: draft.city || null,
    region: draft.region || null,
    postal_code: draft.postal_code || null,
    country: draft.country || null,
  }] : []
  return {
    account_id: draft.account_id,
    book_id: draft.book_id || null,
    display_name: draft.display_name || null,
    given_name: draft.given_name || null,
    family_name: draft.family_name || null,
    org: draft.org || null,
    title: draft.title || null,
    emails,
    phones,
    addresses,
    notes: draft.notes || null,
  }
}

function groupByInitial(contacts: Contact[]): [string, Contact[]][] {
  const map = new Map<string, Contact[]>()
  for (const contact of contacts) {
    const letter = (contactName(contact).trim()[0] ?? '#').toUpperCase()
    const key = /[A-Z]/.test(letter) ? letter : '#'
    if (!map.has(key)) map.set(key, [])
    map.get(key)!.push(contact)
  }
  return Array.from(map.entries()).sort(([a], [b]) => a.localeCompare(b))
}
