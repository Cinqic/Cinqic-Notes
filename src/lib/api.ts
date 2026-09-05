import { invoke } from '@tauri-apps/api/core'
import type {
  BacklinkItem,
  ConflictInfo,
  GraphData,
  LibraryInfo,
  NoteDocument,
  NoteFormat,
  NoteSummary,
  RevisionItem,
  TaskItem,
} from '../types'

export const notesApi = {
  getLastLibrary: () => invoke<string | null>('get_last_library'),
  setLastLibrary: (path: string) => invoke<void>('set_last_library', { path }),
  createLibrary: (path: string) => invoke<LibraryInfo>('create_library', { path }),
  openLibrary: (path: string) => invoke<LibraryInfo>('open_library', { path }),
  listNotes: (includeTrashed = false) => invoke<NoteSummary[]>('list_notes', { includeTrashed }),
  searchNotes: (query: string, includeTrashed = false) =>
    invoke<NoteSummary[]>('search_notes', { query, includeTrashed }),
  getNote: (path: string) => invoke<NoteDocument>('get_note', { path }),
  createNote: (title: string, format: NoteFormat = 'markdown', folder = '') =>
    invoke<NoteDocument>('create_note', { title, format, folder }),
  updateNote: (path: string, content: string, expectedHash: string | null = null) =>
    invoke<NoteDocument>('update_note', { path, content, expectedHash }),
  renameNote: (path: string, title: string) => invoke<NoteDocument>('rename_note', { path, title }),
  trashNote: (path: string) => invoke<void>('trash_note', { path }),
  restoreNote: (path: string) => invoke<NoteDocument>('restore_note', { path }),
  listTasks: (includeComplete = true) => invoke<TaskItem[]>('list_tasks', { includeComplete }),
  toggleTask: (taskId: string, checked: boolean) =>
    invoke<NoteDocument>('toggle_task', { taskId, checked }),
  getBacklinks: (path: string) => invoke<BacklinkItem[]>('get_backlinks', { path }),
  getGraph: () => invoke<GraphData>('get_graph'),
  rebuildIndex: () => invoke<LibraryInfo>('rebuild_index'),
  importFiles: (paths: string[]) => invoke<NoteSummary[]>('import_files', { paths }),
  exportNote: (path: string, destination: string, format: 'md' | 'txt' | 'html') =>
    invoke<void>('export_note', { path, destination, format }),
  getConflict: (path: string) => invoke<ConflictInfo | null>('get_conflict', { path }),
  resolveConflict: (path: string, resolution: 'local' | 'disk') =>
    invoke<NoteDocument>('resolve_conflict', { path, resolution }),
  listRevisions: (path: string) => invoke<RevisionItem[]>('list_revisions', { path }),
  restoreRevision: (path: string, revisionId: string) =>
    invoke<NoteDocument>('restore_revision', { path, revisionId }),
}
