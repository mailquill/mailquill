import type { Rule, RuleCondition, RuleAction } from '@/shared/types'

const HEADER: Record<string, string> = { from: 'from', to: 'to', subject: 'subject' }

function sieveTest(c: RuleCondition): string {
  const val = JSON.stringify(c.value)
  if (c.field === 'body') {
    return c.op === 'notContains' ? `not body :contains ${val}` : `body :contains ${val}`
  }
  const header = JSON.stringify(HEADER[c.field] ?? c.field)
  if (c.op === 'is') return `header :is ${header} ${val}`
  if (c.op === 'notContains') return `not header :contains ${header} ${val}`
  return `header :contains ${header} ${val}`
}

function sieveAction(a: RuleAction): string {
  switch (a.type) {
    case 'move':
      return `    fileinto ${JSON.stringify(a.value ?? 'INBOX')};`
    case 'markRead':
      return '    setflag "\\\\Seen";'
    case 'star':
      return '    setflag "\\\\Flagged";'
    case 'delete':
      return '    discard;'
    case 'forward':
      return `    redirect ${JSON.stringify(a.value ?? '')};`
    default:
      return ''
  }
}

export function compileSieve(rule: Rule): string {
  const tests = rule.conditions.map(sieveTest)
  const join = rule.match_all ? 'allof' : 'anyof'
  const guard = tests.length ? `if ${join} (${tests.join(', ')}) {` : 'if true {'
  const body = rule.actions.map(sieveAction).filter(Boolean).join('\n')
  return [
    'require ["fileinto", "imap4flags"];',
    '',
    `# ${rule.name}`,
    guard,
    body,
    '    stop;',
    '}',
  ].join('\n')
}

export function compileExchange(rule: Rule): string {
  const parts = [`New-InboxRule -Name ${JSON.stringify(rule.name)}`]
  for (const c of rule.conditions) {
    if (c.field === 'from') parts.push(`-From ${JSON.stringify(c.value)}`)
    else if (c.field === 'to') parts.push(`-SentTo ${JSON.stringify(c.value)}`)
    else if (c.field === 'subject') parts.push(`-SubjectContainsWords ${JSON.stringify(c.value)}`)
    else if (c.field === 'body') parts.push(`-BodyContainsWords ${JSON.stringify(c.value)}`)
  }
  for (const a of rule.actions) {
    if (a.type === 'move') parts.push(`-MoveToFolder ${JSON.stringify(a.value ?? 'Inbox')}`)
    else if (a.type === 'markRead') parts.push('-MarkAsRead $true')
    else if (a.type === 'delete') parts.push('-DeleteMessage $true')
    else if (a.type === 'forward') parts.push(`-ForwardTo ${JSON.stringify(a.value ?? '')}`)
  }
  return parts.join(' `\n  ')
}

export function compileRule(rule: Rule): string {
  return rule.engine === 'exchange' ? compileExchange(rule) : compileSieve(rule)
}
