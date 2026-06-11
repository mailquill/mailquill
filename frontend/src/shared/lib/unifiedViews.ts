// Cross-account ("unified") mail views shown in the sidebar's ÜBERGREIFEND
// section. Shared so the sidebar nav and the mailbox page agree on ids/labels.
export type UnifiedView = 'inbox' | 'starred' | 'sent' | 'drafts' | 'archive' | 'spam' | 'trash'

export interface UnifiedCounts {
  inbox: number
  starred: number
  sent: number
  drafts: number
  archive: number
  spam: number
  trash: number
}

export const UNIFIED_VIEWS: { id: UnifiedView; labelKey: string }[] = [
  { id: 'inbox', labelKey: 'sidebar.unifiedInbox' },
  { id: 'starred', labelKey: 'sidebar.unifiedStarred' },
  { id: 'sent', labelKey: 'sidebar.unifiedSent' },
  { id: 'drafts', labelKey: 'sidebar.unifiedDrafts' },
  { id: 'archive', labelKey: 'sidebar.unifiedArchive' },
  { id: 'spam', labelKey: 'sidebar.unifiedSpam' },
  { id: 'trash', labelKey: 'sidebar.unifiedTrash' },
]

export const UNIFIED_LABEL_KEY = Object.fromEntries(
  UNIFIED_VIEWS.map((v) => [v.id, v.labelKey]),
) as Record<UnifiedView, string>

export function isUnifiedView(value: string | undefined): value is UnifiedView {
  return !!value && UNIFIED_VIEWS.some((v) => v.id === value)
}
