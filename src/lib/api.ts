import { invoke } from '@tauri-apps/api/core'
import type {
  BacklinkItem,
  AttachmentItem,
  ConflictInfo,
  GraphData,
  IntegrityInfo,
  LinkItem,
  LibraryInfo,
  NoteDocument,
  NoteFormat,
  NoteSummary,
  RecoveryDraftInfo,
  RevisionItem,
  TagItem,
  TaskItem,
} from '../types'

export const notesApi = {
  getLastLibrary: () => invoke<string | null>('get_last_library'),
  setLastLibrary: (path: string) => invoke<void>('set_last_library', { path }),
  createLibrary: (path: string) => invoke<LibraryInfo>('create_library', { path }),
  openLibrary: (path: string) => invoke<LibraryInfo>('open_library', { path }),
  listNotes: (includeTrashed = false) => invoke<NoteSummary[]>('list_notes', { includeTrashed }),
  listNotesFiltered: (includeTrashed = false, includeArchived = false) =>
    invoke<NoteSummary[]>('list_notes_filtered', { includeTrashed, includeArchived }),
  searchNotes: (query: string, includeTrashed = false) =>
    invoke<NoteSummary[]>('search_notes', { query, includeTrashed }),
  searchNotesFiltered: (query: string, includeTrashed = false, includeArchived = false) =>
    invoke<NoteSummary[]>('search_notes_filtered', { query, includeTrashed, includeArchived }),
  getNote: (path: string) => invoke<NoteDocument>('get_note', { path }),
  createNote: (title: string, format: NoteFormat = 'markdown', folder = '') =>
    invoke<NoteDocument>('create_note', { title, format, folder }),
  createDailyNote: (date: string) => invoke<NoteDocument>('create_daily_note', { date }),
  updateNote: (path: string, content: string, expectedHash: string | null = null) =>
    invoke<NoteDocument>('update_note', { path, content, expectedHash }),
  renameNote: (path: string, title: string) => invoke<NoteDocument>('rename_note', { path, title }),
  moveNote: (path: string, folder: string) => invoke<NoteDocument>('move_note', { path, folder }),
  archiveNote: (path: string, archived: boolean) =>
    invoke<NoteDocument>('archive_note', { path, archived }),
  trashNote: (path: string) => invoke<void>('trash_note', { path }),
  restoreNote: (path: string) => invoke<NoteDocument>('restore_note', { path }),
  listTasks: (includeComplete = true) => invoke<TaskItem[]>('list_tasks', { includeComplete }),
  toggleTask: (taskId: string, checked: boolean) =>
    invoke<NoteDocument>('toggle_task', { taskId, checked }),
  getBacklinks: (path: string) => invoke<BacklinkItem[]>('get_backlinks', { path }),
  getGraph: () => invoke<GraphData>('get_graph'),
  getOutgoingLinks: (path: string) => invoke<LinkItem[]>('get_outgoing_links', { path }),
  listTags: () => invoke<TagItem[]>('list_tags'),
  listAttachments: () => invoke<AttachmentItem[]>('list_attachments'),
  importAttachment: (source: string) => invoke<AttachmentItem>('import_attachment', { source }),
  rebuildIndex: () => invoke<LibraryInfo>('rebuild_index'),
  importFiles: (paths: string[]) => invoke<NoteSummary[]>('import_files', { paths }),
  exportNote: (path: string, destination: string, format: 'md' | 'txt' | 'html') =>
    invoke<void>('export_note', { path, destination, format }),
  backupLibrary: (destination: string, includeInternal = false) =>
    invoke<void>('backup_library', { destination, includeInternal }),
  restoreBackup: (archivePath: string, destination: string) =>
    invoke<void>('restore_backup', { archivePath, destination }),
  emptyTrash: () => invoke<number>('empty_trash'),
  verifyIntegrity: () => invoke<IntegrityInfo>('verify_integrity'),
  saveRecoveryDraft: (notePath: string, content: string) =>
    invoke<RecoveryDraftInfo>('save_recovery_draft', { notePath, content }),
  listRecoveryDrafts: () => invoke<RecoveryDraftInfo[]>('list_recovery_drafts'),
  readRecoveryDraft: (path: string) => invoke<string>('read_recovery_draft', { path }),
  removeRecoveryDraft: (path: string) => invoke<void>('remove_recovery_draft', { path }),
  getConflict: (path: string) => invoke<ConflictInfo | null>('get_conflict', { path }),
  resolveConflict: (path: string, resolution: 'local' | 'disk') =>
    invoke<NoteDocument>('resolve_conflict', { path, resolution }),
  listRevisions: (path: string) => invoke<RevisionItem[]>('list_revisions', { path }),
  restoreRevision: (path: string, revisionId: string) =>
    invoke<NoteDocument>('restore_revision', { path, revisionId }),
}
