export type NoteFormat = 'markdown' | 'text'

export type Theme = 'system' | 'light' | 'dark'

export type SaveState = 'saved' | 'saving' | 'dirty' | 'conflict' | 'error'

export interface NoteSummary {
  id: string
  path: string
  title: string
  format: NoteFormat
  preview: string
  hash: string
  modifiedAt: string
  createdAt: string
  archived: boolean
  trashed: boolean
  project: boolean
  tags: string[]
  taskCount: number
  openTaskCount: number
}

export interface NoteDocument extends NoteSummary {
  content: string
}

export interface LibraryInfo {
  path: string
  noteCount: number
  lastIndexedAt: string
}

export interface TaskItem {
  id: string
  notePath: string
  noteTitle: string
  line: number
  checked: boolean
  text: string
  project: boolean
}

export interface BacklinkItem {
  sourcePath: string
  sourceTitle: string
  kind: string
  label: string
}

export interface LinkItem {
  sourcePath: string
  sourceTitle: string
  targetPath: string
  targetTitle?: string | null
  kind: string
  label: string
  resolved: boolean
}

export interface TagItem {
  tag: string
  noteCount: number
}

export interface AttachmentItem {
  path: string
  size: number
  modifiedAt: string
}

export interface RecoveryDraftInfo {
  path: string
  notePath: string
  createdAt: string
  size: number
}

export interface IntegrityInfo {
  ok: boolean
  message: string
}

export interface GraphData {
  nodes: Array<{ path: string; title: string; project: boolean }>
  edges: Array<{ source: string; target: string; kind: string }>
}

export interface ConflictInfo {
  path: string
  localContent: string
  diskContent: string
  message: string
}

export interface RevisionItem {
  id: string
  notePath: string
  actor: string
  reason: string
  createdAt: string
  content: string
}
