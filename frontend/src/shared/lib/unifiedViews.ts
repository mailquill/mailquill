// Cross-account ("unified") mail views shown in the sidebar's ÜBERGREIFEND
// section. Shared so the sidebar nav and the mailbox page agree on ids/labels.
/** Identifier of a cross-account mail view, also used as the route segment. */
export type UnifiedView = 'inbox' | 'starred' | 'sent' | 'drafts' | 'archive' | 'spam' | 'trash'

/** Sidebar badge counts per unified view. All views count unread messages except `starred`, which counts flagged ones. */
export interface UnifiedCounts {
  inbox: number
  starred: number
  sent: number
  drafts: number
  archive: number
  spam: number
  trash: number
}

/** Unified views in sidebar display order, each with its i18n label key. */
export const UNIFIED_VIEWS: { id: UnifiedView; labelKey: string }[] = [
  { id: 'inbox', labelKey: 'sidebar.unifiedInbox' },
  { id: 'starred', labelKey: 'sidebar.unifiedStarred' },
  { id: 'sent', labelKey: 'sidebar.unifiedSent' },
  { id: 'drafts', labelKey: 'sidebar.unifiedDrafts' },
  { id: 'archive', labelKey: 'sidebar.unifiedArchive' },
  { id: 'spam', labelKey: 'sidebar.unifiedSpam' },
  { id: 'trash', labelKey: 'sidebar.unifiedTrash' },
]

/** Lookup from unified view id to its i18n label key. */
export const UNIFIED_LABEL_KEY = Object.fromEntries(
  UNIFIED_VIEWS.map((v) => [v.id, v.labelKey]),
) as Record<UnifiedView, string>

/**
 * Type guard for route parameters that may name a unified view.
 * @param value - Candidate view id, e.g. from the URL.
 * @returns `true` when the value is one of the known unified view ids.
 */
export function isUnifiedView(value: string | undefined): value is UnifiedView {
  return !!value && UNIFIED_VIEWS.some((v) => v.id === value)
}
