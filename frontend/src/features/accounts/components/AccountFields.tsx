import type { ReactNode } from 'react'
import type { Inbox } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { ACCT_COLORS, type Security } from '@/shared/lib/serverDiscovery'

/**
 * Editable fields of the account editor, shared by the add wizard and the settings card. Ports are kept as strings because they come straight from text inputs.
 * @property short - Short label shown in the sidebar instead of the full name.
 * @property initials - One or two characters for the account badge.
 * @property initialsTouched - Set once the user edits the initials, so autodetection stops overwriting them.
 * @property composeFormat - Default editor mode for new messages from this account.
 * @property hasJunk - Whether the server provides a Junk/Spam folder.
 * @property signature - Plain-text signature appended to new messages.
 */
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

// ── primitives ──────────────────────────────────────────────────────────────

/**
 * Label row above a form control in the account editor, with an optional muted hint to its right.
 * @param props - `children` is the label text; `hint` is secondary text such as "optional".
 * @returns The label block.
 */
export function FieldLabel({ children, hint }: { children: ReactNode; hint?: ReactNode }) {
  return (
    <div className="mb-1.5">
      <span className="text-[12.5px] font-semibold text-foreground">{children}</span>
      {hint && <span className="ml-2 text-[12px] font-normal text-muted-foreground">{hint}</span>}
    </div>
  )
}

/**
 * Row of colour swatches for choosing the account colour from `ACCT_COLORS`.
 * @param props - `value` is the selected hex colour (highlighted with a ring); `onChange` receives the clicked colour.
 * @returns A flex-wrapped row of swatch buttons.
 */
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

/**
 * Labelled wrapper for a single server-settings input (host, port, security, ...), with an optional width class and inline error text.
 * @param props - `label` is the caption; `children` the control; `w` an optional width utility class; `error` a validation message rendered below; `htmlFor` makes the caption a `<label>` bound to the control id.
 * @returns The labelled field.
 */
export function SrvField({
  label,
  children,
  w,
  error,
  htmlFor,
}: {
  label: string
  children: ReactNode
  w?: string
  error?: string
  htmlFor?: string
}) {
  const LabelTag = htmlFor ? 'label' : 'div'
  return (
    <div className={cn('min-w-0', w)}>
      <LabelTag htmlFor={htmlFor} className="mb-1.5 block text-[11.5px] font-semibold text-secondary-foreground">
        {label}
      </LabelTag>
      {children}
      {error && <p className="mt-1 text-[11px] text-destructive">{error}</p>}
    </div>
  )
}

/**
 * Titled group of server fields (IMAP, SMTP, Contacts, Calendar) with an icon heading and indented body.
 * @param props - `title` is the heading; `icon` the lucide icon shown before it; `children` the fields.
 * @returns The group block.
 */
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

