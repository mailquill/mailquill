import type { ElementType } from 'react'
import {
  Inbox,
  Send,
  FileText,
  Archive,
  ShieldAlert,
  Trash2,
  Star,
  FolderIcon,
} from 'lucide-react'
import type { Folder } from '@/shared/types'

// Standard folders pinned to the top in this order; everything else follows,
// sorted alphabetically by path. Shared by the sidebar and the move-to menu so
// both show the same ordering.
const FOLDER_ORDER: Record<string, number> = {
  INBOX: 0,
  SENT: 1,
  DRAFTS: 2,
  SPAM: 3,
  JUNK: 3,
  TRASH: 4,
}

// Localised label per standard folder type; custom folders keep their IMAP name.
const FOLDER_TYPE_LABEL: Record<string, string> = {
  INBOX: 'sidebar.unifiedInbox',
  SENT: 'sidebar.unifiedSent',
  DRAFTS: 'sidebar.unifiedDrafts',
  SPAM: 'sidebar.unifiedSpam',
  JUNK: 'sidebar.unifiedSpam',
  TRASH: 'sidebar.unifiedTrash',
  ARCHIVE: 'sidebar.unifiedArchive',
}

// Icon per standard folder type; custom folders fall back to a generic folder.
const FOLDER_ICONS: Record<string, ElementType> = {
  INBOX: Inbox,
  STARRED: Star,
  SENT: Send,
  DRAFTS: FileText,
  ARCHIVE: Archive,
  SPAM: ShieldAlert,
  JUNK: ShieldAlert,
  TRASH: Trash2,
  CUSTOM: FolderIcon,
}

export function folderIcon(folder: Folder): ElementType {
  return FOLDER_ICONS[folder.folder_type] ?? FolderIcon
}

export function folderRank(folder: Folder): number {
  return FOLDER_ORDER[folder.folder_type] ?? 5
}

export function sortFolders(folders: Folder[]): Folder[] {
  return [...folders].sort(
    (a, b) => folderRank(a) - folderRank(b) || folderTreePath(a).localeCompare(folderTreePath(b)),
  )
}

/** Hierarchy separator inside `full_path` (IMAP folder delimiter). */
const PATH_SEP = '/'

/** Display label for a folder: the translated name for standard folders, the
 *  server-decoded leaf name (`folder_name`) for custom ones — nested folders
 *  read as "Congstar", not "Mobilfunk/Congstar". The IMAP-encoded `full_path`
 *  is never shown. */
export function folderLeafLabel(folder: Folder, t: (key: string) => string): string {
  const key = FOLDER_TYPE_LABEL[folder.folder_type]
  return key ? t(key) : folder.folder_name
}

export interface FolderNode {
  folder: Folder
  depth: number
  children: FolderNode[]
}

/** Build a hierarchy from `full_path`, splitting on the IMAP delimiter. Nodes
 *  keep the sidebar ordering (standard folders first, then alphabetical).
 *  A child whose parent folder doesn't exist falls back to a root. */
export function buildFolderTree(folders: Folder[]): FolderNode[] {
  const sorted = sortFolders(folders)
  const byPath = new Map<string, FolderNode>()
  for (const folder of sorted) {
    byPath.set(folderTreePath(folder), { folder, depth: 0, children: [] })
  }
  const roots: FolderNode[] = []
  // `sorted` lists every parent before its children (a path sorts before its
  // own sub-paths), so a parent's depth is set by the time a child reads it.
  for (const folder of sorted) {
    const path = folderTreePath(folder)
    const node = byPath.get(path)!
    const i = path.lastIndexOf(PATH_SEP)
    const parent = i > 0 ? byPath.get(path.slice(0, i)) : undefined
    if (parent) {
      node.depth = parent.depth + 1
      parent.children.push(node)
    } else {
      roots.push(node)
    }
  }
  return roots
}

function folderTreePath(folder: Folder): string {
  return folder.folder_display_path || folder.full_path
}

/** Pre-order flatten, parents before children — for renderers that indent by
 *  `depth` instead of nesting DOM (e.g. the move-to menu). */
export function flattenFolderTree(nodes: FolderNode[]): FolderNode[] {
  const out: FolderNode[] = []
  const walk = (ns: FolderNode[]) => {
    for (const n of ns) {
      out.push(n)
      walk(n.children)
    }
  }
  walk(nodes)
  return out
}
