import { useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Plus, Trash2, Pencil, ChevronLeft, X, Code2, UploadCloud } from 'lucide-react'
import { cn } from '@/shared/lib/utils'
import { Input } from '@/shared/components/ui/input'
import { Label } from '@/shared/components/ui/label'
import { Select } from '@/shared/components/ui/select'
import { Button } from '@/shared/components/ui/button'
import { useAccounts } from '@/shared/hooks/useAccounts'
import { useRules, useCreateRule, useUpdateRule, useDeleteRule, useApplySieve } from '@/shared/hooks/useRules'
import { compileRule } from '@/shared/lib/ruleScript'
import type {
  Rule,
  RuleInput,
  RuleField,
  RuleOp,
  RuleActionType,
  RuleEngine,
} from '@/shared/types'

const FIELDS: RuleField[] = ['from', 'to', 'subject', 'body']
const OPS: RuleOp[] = ['contains', 'notContains', 'is']
const ACTION_TYPES: RuleActionType[] = ['move', 'markRead', 'star', 'delete', 'forward']
const VALUE_ACTIONS: RuleActionType[] = ['move', 'forward']

function emptyDraft(from?: string): RuleInput {
  return {
    account_id: null,
    name: from ? `From ${from}` : 'New rule',
    enabled: true,
    engine: 'sieve',
    match_all: true,
    conditions: [{ field: 'from', op: 'contains', value: from ?? '' }],
    actions: [{ type: 'move', value: '' }],
  }
}

function summarise(rule: Rule): string {
  const conds = rule.conditions.map((c) => `${c.field} ${c.op} "${c.value}"`).join(rule.match_all ? ' and ' : ' or ')
  const acts = rule.actions.map((a) => (a.value ? `${a.type} → ${a.value}` : a.type)).join(', ')
  return `If ${conds || '…'} then ${acts || '…'}`
}

export function RulesSection() {
  const { t } = useTranslation()
  const { data: rules = [] } = useRules()
  const { data: accounts = [] } = useAccounts()
  const deleteRule = useDeleteRule()
  const updateRule = useUpdateRule()
  const applySieve = useApplySieve()
  const [applyMsg, setApplyMsg] = useState<string | null>(null)
  const [searchParams, setSearchParams] = useSearchParams()
  const fromParam = searchParams.get('from') ?? undefined
  const [editing, setEditing] = useState<{ id?: string; draft: RuleInput } | null>(() =>
    fromParam ? { draft: emptyDraft(fromParam) } : null,
  )

  function clearFromParam() {
    if (searchParams.has('from')) {
      const next = new URLSearchParams(searchParams)
      next.delete('from')
      setSearchParams(next, { replace: true })
    }
  }

  function close() {
    setEditing(null)
    clearFromParam()
  }

  if (editing) {
    return (
      <RuleEditor
        initial={editing.draft}
        ruleId={editing.id}
        onClose={close}
      />
    )
  }

  async function applyAll() {
    setApplyMsg(null)
    const results = await Promise.allSettled(accounts.map((a) => applySieve.mutateAsync(a.id)))
    const ok = results.filter((r) => r.status === 'fulfilled').length
    const failed = results.length - ok
    setApplyMsg(`Uploaded to ${ok} account(s)${failed ? `, ${failed} failed (non-Sieve servers are skipped)` : ''}.`)
  }

  return (
    <div>
      <div className="mb-5 flex items-start justify-between">
        <div>
          <h2 className="text-[18px] font-bold tracking-tight">{t('settings.rules')}</h2>
          <p className="mt-1 text-[13px] text-muted-foreground">{t('settings.rulesDesc')}</p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={applyAll} disabled={applySieve.isPending || !accounts.length}>
            <UploadCloud className="size-4" />
            {applySieve.isPending ? t('settings.applying') : t('settings.applyToServer')}
          </Button>
          <Button onClick={() => setEditing({ draft: emptyDraft() })}>
            <Plus className="size-4" />
            {t('rules.newRule')}
          </Button>
        </div>
      </div>
      {applyMsg && <p className="mb-4 text-[12.5px] text-muted-foreground">{applyMsg}</p>}

      <div className="flex flex-col gap-2.5">
        {rules.map((rule) => (
          <div key={rule.id} className="flex items-center gap-3 rounded-lg border border-border bg-card p-3.5">
            <input
              type="checkbox"
              checked={rule.enabled}
              onChange={(e) => updateRule.mutate({ id: rule.id, data: { ...rule, enabled: e.currentTarget.checked } })}
              className="size-4 accent-primary"
              title={t('rules.enabled')}
            />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="truncate text-[14px] font-semibold">{rule.name}</span>
                <span
                  className={cn(
                    'rounded-md px-2 py-0.5 text-[10.5px] font-bold uppercase',
                    rule.engine === 'exchange' ? 'bg-[#2563eb]/10 text-[#2563eb]' : 'bg-secondary text-secondary-foreground',
                  )}
                >
                  {rule.engine}
                </span>
              </div>
              <p className="mt-0.5 truncate text-[12.5px] text-muted-foreground">{summarise(rule)}</p>
            </div>
            <button
              onClick={() => setEditing({ id: rule.id, draft: rule })}
              className="flex size-8 items-center justify-center rounded-md text-secondary-foreground hover:bg-secondary"
              title={t('rules.edit')}
            >
              <Pencil className="size-4" />
            </button>
            <button
              onClick={() => window.confirm(t('rules.deleteConfirm', { name: rule.name })) && deleteRule.mutate(rule.id)}
              className="flex size-8 items-center justify-center rounded-md text-destructive hover:bg-secondary"
              title={t('action.delete')}
            >
              <Trash2 className="size-4" />
            </button>
          </div>
        ))}
        {!rules.length && (
          <p className="rounded-lg border border-border p-6 text-center text-sm text-muted-foreground">
            {t('rules.noRules')}
          </p>
        )}
      </div>
    </div>
  )
}

function RuleEditor({ initial, ruleId, onClose }: { initial: RuleInput; ruleId?: string; onClose: () => void }) {
  const { t } = useTranslation()
  const { data: accounts = [] } = useAccounts()
  const createRule = useCreateRule()
  const updateRule = useUpdateRule()
  const [draft, setDraft] = useState<RuleInput>(initial)
  const [showScript, setShowScript] = useState(false)

  function patch(p: Partial<RuleInput>) {
    setDraft((d) => ({ ...d, ...p }))
  }

  function save() {
    if (!draft.name.trim()) return
    if (ruleId) updateRule.mutate({ id: ruleId, data: draft }, { onSuccess: onClose })
    else createRule.mutate(draft, { onSuccess: onClose })
  }

  const preview = compileRule({ id: ruleId ?? 'preview', ...draft })
  const pending = createRule.isPending || updateRule.isPending

  return (
    <div className="max-w-3xl">
      <button onClick={onClose} className="mb-4 inline-flex items-center gap-1.5 text-[13px] font-semibold text-[#2563eb] hover:underline">
        <ChevronLeft className="size-4" />
        {t('settings.rules')}
      </button>

      <div className="flex flex-col gap-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t('rules.name')}>
            <Input value={draft.name} onChange={(e) => patch({ name: e.currentTarget.value })} />
          </Field>
          <Field label={t('rules.account')}>
            <Select
              value={draft.account_id ?? ''}
              onChange={(e) => patch({ account_id: e.currentTarget.value || null })}
            >
              <option value="">{t('rules.allAccounts')}</option>
              {accounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.display_name}
                </option>
              ))}
            </Select>
          </Field>
          <Field label={t('rules.engine')}>
            <Select value={draft.engine} onChange={(e) => patch({ engine: e.currentTarget.value as RuleEngine })}>
              <option value="sieve">{t('rules.sieveImap')}</option>
              <option value="exchange">{t('rules.exchange')}</option>
            </Select>
          </Field>
          <Field label={t('rules.match')}>
            <Select value={draft.match_all ? 'all' : 'any'} onChange={(e) => patch({ match_all: e.currentTarget.value === 'all' })}>
              <option value="all">{t('rules.allConditions')}</option>
              <option value="any">{t('rules.anyCondition')}</option>
            </Select>
          </Field>
        </div>

        {/* conditions */}
        <Group
          title={t('rules.conditions')}
          onAdd={() => patch({ conditions: [...draft.conditions, { field: 'subject', op: 'contains', value: '' }] })}
        >
          {draft.conditions.map((c, i) => (
            <div key={i} className="flex items-center gap-2">
              <Select
                value={c.field}
                onChange={(e) => patch({ conditions: replace(draft.conditions, i, { ...c, field: e.currentTarget.value as RuleField }) })}
                className="w-32"
              >
                {FIELDS.map((f) => (
                  <option key={f} value={f}>
                    {f}
                  </option>
                ))}
              </Select>
              <Select
                value={c.op}
                onChange={(e) => patch({ conditions: replace(draft.conditions, i, { ...c, op: e.currentTarget.value as RuleOp }) })}
                className="w-40"
              >
                {OPS.map((o) => (
                  <option key={o} value={o}>
                    {o}
                  </option>
                ))}
              </Select>
              <Input
                value={c.value}
                onChange={(e) => patch({ conditions: replace(draft.conditions, i, { ...c, value: e.currentTarget.value }) })}
                className="flex-1"
              />
              <RemoveBtn onClick={() => patch({ conditions: draft.conditions.filter((_, j) => j !== i) })} />
            </div>
          ))}
        </Group>

        {/* actions */}
        <Group
          title={t('rules.actions')}
          onAdd={() => patch({ actions: [...draft.actions, { type: 'markRead' }] })}
        >
          {draft.actions.map((a, i) => (
            <div key={i} className="flex items-center gap-2">
              <Select
                value={a.type}
                onChange={(e) => patch({ actions: replace(draft.actions, i, { ...a, type: e.currentTarget.value as RuleActionType }) })}
                className="w-44"
              >
                {ACTION_TYPES.map((tp) => (
                  <option key={tp} value={tp}>
                    {tp}
                  </option>
                ))}
              </Select>
              {VALUE_ACTIONS.includes(a.type) && (
                <Input
                  value={a.value ?? ''}
                  placeholder={a.type === 'move' ? t('rules.folder') : t('rules.address')}
                  onChange={(e) => patch({ actions: replace(draft.actions, i, { ...a, value: e.currentTarget.value }) })}
                  className="flex-1"
                />
              )}
              <RemoveBtn onClick={() => patch({ actions: draft.actions.filter((_, j) => j !== i) })} />
            </div>
          ))}
        </Group>

        {/* generated script */}
        <div className="rounded-lg border border-border bg-card">
          <button
            onClick={() => setShowScript((s) => !s)}
            className="flex w-full items-center gap-2 px-4 py-2.5 text-left text-[13px] font-semibold text-secondary-foreground"
          >
            <Code2 className="size-4 text-muted-foreground" />
            {t('rules.generatedScript', { engine: draft.engine === 'exchange' ? 'Exchange' : 'Sieve' })}
            <span className="ml-auto text-[11px] text-muted-foreground">{showScript ? t('rules.hide') : t('rules.show')}</span>
          </button>
          {showScript && (
            <pre className="overflow-x-auto border-t border-border bg-background px-4 py-3 font-mono text-[11.5px] leading-relaxed text-secondary-foreground">
              {preview}
            </pre>
          )}
        </div>

        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            {t('action.cancel')}
          </Button>
          <Button onClick={save} disabled={pending || !draft.name.trim()}>
            {pending ? t('settings.saving') : t('settings.saveChanges')}
          </Button>
        </div>
      </div>
    </div>
  )
}

function replace<T>(list: T[], index: number, value: T): T[] {
  return list.map((item, i) => (i === index ? value : item))
}

function Group({ title, onAdd, children }: { title: string; onAdd: () => void; children: React.ReactNode }) {
  const { t } = useTranslation()
  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <div className="mb-3 flex items-center justify-between">
        <span className="text-[13px] font-bold uppercase tracking-wide text-muted-foreground">{title}</span>
        <Button variant="outline" size="sm" onClick={onAdd}>
          <Plus className="size-3.5" />
          {t('rules.add')}
        </Button>
      </div>
      <div className="flex flex-col gap-2">{children}</div>
    </div>
  )
}

function RemoveBtn({ onClick }: { onClick: () => void }) {
  const { t } = useTranslation()
  return (
    <button onClick={onClick} className="flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-secondary" title={t('rules.remove')}>
      <X className="size-4" />
    </button>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label className="text-[12px] font-semibold">{label}</Label>
      {children}
    </div>
  )
}
