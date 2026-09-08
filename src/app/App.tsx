import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent } from 'react'
import { AutosaveController } from './autosave'
import { open, save } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { notesApi } from '../lib/api'
import { readTheme, writeTheme } from '../lib/preferences'
import {
  formatRelativeDate,
  localCalendarDate,
  renderSafeMarkdown,
  toPlainText,
} from '../lib/markdown'
import type {
  BacklinkItem,
  ConflictInfo,
  GraphData,
  LinkItem,
  LibraryInfo,
  NoteDocument,
  NoteSummary,
  RecoveryDraftInfo,
  RevisionItem,
  SaveState,
  Theme,
  TagItem,
  TaskItem,
} from '../types'

type View = 'all' | 'today' | 'projects' | 'tasks' | 'graph' | 'archive' | 'trash' | 'settings'

const isDesktop = () => '__TAURI_INTERNALS__' in window

const displayError = (error: unknown) => {
  if (typeof error === 'string') return error
  if (error && typeof error === 'object' && 'message' in error) return String(error.message)
  return 'Something went wrong. Your original file was not intentionally discarded.'
}

const relativeLibraryPath = (notePath: string, targetPath: string) => {
  const noteDirectory = notePath.split('/').slice(0, -1)
  const target = targetPath.split('/')
  while (noteDirectory.length && target.length && noteDirectory[0] === target[0]) {
    noteDirectory.shift()
    target.shift()
  }
  return [...noteDirectory.map(() => '..'), ...target].join('/') || targetPath
}

/**
 * The shortcut that actually focuses the note search box.
 *
 * The label used to read "⌘ K" on every platform, which was wrong twice: Ctrl/
 * Cmd+K opens the command palette, and Windows and Linux are supported targets
 * that do not have a Command key.
 */
const searchShortcutLabel = () => {
  const platform =
    typeof navigator === 'undefined'
      ? ''
      : ((navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData
          ?.platform ??
        navigator.platform ??
        navigator.userAgent)
  return /mac|iphone|ipad|ipod/i.test(platform) ? '⌘F' : 'Ctrl F'
}

export default function App() {
  const [library, setLibrary] = useState<LibraryInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [bootError, setBootError] = useState('')
  // Appearance is remembered locally; a Settings control that silently reset on
  // every launch was telling the user something untrue.
  const [theme, setTheme] = useState<Theme>(readTheme)

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    writeTheme(theme)
  }, [theme])

  useEffect(() => {
    if (!isDesktop()) {
      setLoading(false)
      return
    }
    notesApi
      .getLastLibrary()
      .then((path) => (path ? notesApi.openLibrary(path).then(setLibrary) : null))
      .catch((error: unknown) => setBootError(displayError(error)))
      .finally(() => setLoading(false))
  }, [])

  const chooseLibrary = useCallback(async (mode: 'create' | 'open') => {
    setBootError('')
    try {
      const chosen = await open({ directory: true, multiple: false })
      const path = Array.isArray(chosen) ? chosen[0] : chosen
      if (!path) return
      const info =
        mode === 'create' ? await notesApi.createLibrary(path) : await notesApi.openLibrary(path)
      await notesApi.setLastLibrary(path)
      setLibrary(info)
    } catch (error: unknown) {
      setBootError(displayError(error))
    }
  }, [])

  if (loading) return <div className="boot-screen">Opening your Library…</div>
  if (!library) {
    return (
      <Onboarding
        error={bootError}
        desktop={isDesktop()}
        onCreate={() => void chooseLibrary('create')}
        onOpen={() => void chooseLibrary('open')}
      />
    )
  }

  return <NotesWorkspace initialLibrary={library} theme={theme} setTheme={setTheme} />
}

function Onboarding({
  error,
  desktop,
  onCreate,
  onOpen,
}: {
  error: string
  desktop: boolean
  onCreate: () => void
  onOpen: () => void
}) {
  return (
    <main className="onboarding-shell">
      <section className="onboarding-card" aria-labelledby="onboarding-title">
        <div className="brand-mark large-mark" aria-hidden="true">
          <span>cn</span>
        </div>
        <p className="kicker">CINQIC NOTES</p>
        <h1 id="onboarding-title">Your notes, close at hand.</h1>
        <p className="onboarding-copy">
          A quiet, dependable writing space. Your Library stays on this device as ordinary Markdown
          and text files.
        </p>
        <div className="onboarding-actions">
          <button className="button primary" onClick={onCreate} disabled={!desktop}>
            <span aria-hidden="true">＋</span> Create Notes Library
          </button>
          <button className="button secondary" onClick={onOpen} disabled={!desktop}>
            Open Existing Folder
          </button>
        </div>
        {!desktop && (
          <p className="hint">
            Run this app through the Tauri desktop shell to choose a local folder.
          </p>
        )}
        {error && (
          <p className="error-message" role="alert">
            {error}
          </p>
        )}
        <div className="onboarding-footnote">
          <span className="privacy-dot" aria-hidden="true" /> No account · No cloud · No AI required
        </div>
      </section>
    </main>
  )
}

function NotesWorkspace({
  initialLibrary,
  theme,
  setTheme,
}: {
  initialLibrary: LibraryInfo
  theme: Theme
  setTheme: (value: Theme) => void
}) {
  const [library, setLibrary] = useState(initialLibrary)
  const [notes, setNotes] = useState<NoteSummary[]>([])
  const [activeDocument, setActiveDocument] = useState<NoteDocument | null>(null)
  const [content, setContent] = useState('')
  const [titleDraft, setTitleDraft] = useState('')
  const [query, setQuery] = useState('')
  const [tagFilter, setTagFilter] = useState('')
  const [formatFilter, setFormatFilter] = useState<'all' | 'markdown' | 'text'>('all')
  const [taskFilter, setTaskFilter] = useState<'all' | 'tasks' | 'open'>('all')
  const [view, setView] = useState<View>('all')
  const [saveState, setSaveState] = useState<SaveState>('saved')
  const [message, setMessage] = useState('')
  const [error, setError] = useState('')
  const [showSidebar, setShowSidebar] = useState(true)
  const [showList, setShowList] = useState(true)
  const [showPreview, setShowPreview] = useState(false)
  const [showCommandPalette, setShowCommandPalette] = useState(false)
  const [showShare, setShowShare] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const [backlinks, setBacklinks] = useState<BacklinkItem[]>([])
  const [outgoingLinks, setOutgoingLinks] = useState<LinkItem[]>([])
  const [revisions, setRevisions] = useState<RevisionItem[]>([])
  const [tasks, setTasks] = useState<TaskItem[]>([])
  const [tags, setTags] = useState<TagItem[]>([])
  const [graph, setGraph] = useState<GraphData | null>(null)
  const [conflict, setConflict] = useState<ConflictInfo | null>(null)
  const editorRef = useRef<HTMLTextAreaElement>(null)
  const activePathRef = useRef<string | null>(null)
  activePathRef.current = activeDocument?.path ?? null

  // Autosave is owned by a serialized controller rather than a bare debounce so
  // that a late save completion can never overwrite a newer edit, and so that
  // pending work is flushed — not cancelled — when the user navigates away.
  const autosaveRef = useRef<AutosaveController | null>(null)
  if (autosaveRef.current === null) {
    autosaveRef.current = new AutosaveController({
      save: async ({ path, content: next, expectedHash }) => {
        const doc = await notesApi.updateNote(path, next, expectedHash)
        return { path: doc.path, content: doc.content, hash: doc.hash }
      },
      onChange: (snapshot) => setSaveState(snapshot.state),
      onSaved: (result) => {
        setActiveDocument((current) =>
          current && current.path === result.path ? { ...current, hash: result.hash } : current,
        )
        setNotes((current) =>
          current.map((note) =>
            note.path === result.path ? { ...note, hash: result.hash } : note,
          ),
        )
        setMessage('Saved locally')
        window.setTimeout(() => setMessage(''), 1800)
      },
      onError: (nextError, state) => {
        setError(displayError(nextError))
        if (state !== 'conflict') return
        const path = activePathRef.current
        if (!path) return
        void notesApi
          .getConflict(path)
          .then(setConflict)
          .catch(() => undefined)
      },
    })
  }
  const autosave = autosaveRef.current

  /** Point the editor at a document whose content already matches disk. */
  const activateDocument = useCallback(
    (doc: NoteDocument, override?: string) => {
      const next = override ?? doc.content
      setActiveDocument(doc)
      setContent(next)
      setTitleDraft(doc.title)
      activePathRef.current = doc.path
      autosave.activate(doc.path, doc.content, doc.hash)
      if (override !== undefined && override !== doc.content) autosave.edit(override)
    },
    [autosave],
  )

  /** Record a user edit in both the rendered buffer and the save controller. */
  const editContent = useCallback(
    (next: string) => {
      setContent(next)
      autosave.edit(next)
    },
    [autosave],
  )

  /**
   * Cinqic Notes does not navigate to remote pages in this milestone, so an
   * external link is surfaced as a copyable address rather than being opened.
   * Letting the WebView follow the link would take the application window to a
   * remote origin and leave the local-first boundary entirely.
   */
  const openExternalLink = useCallback(async (url: string) => {
    if (!navigator.clipboard?.writeText) {
      setMessage(`Link: ${url}`)
      window.setTimeout(() => setMessage(''), 4000)
      return
    }
    try {
      await navigator.clipboard.writeText(url)
      setMessage('Link address copied — Cinqic Notes does not open remote pages')
    } catch {
      setMessage(`Link: ${url}`)
    }
    window.setTimeout(() => setMessage(''), 4000)
  }, [])

  /**
   * Persist any pending edit before leaving the current buffer. Returns false
   * when the note is still dirty, so callers can block the navigation instead
   * of discarding the user's text.
   */
  const flushPending = useCallback(async () => {
    if (!autosave.isDirty) return true
    return autosave.flush()
  }, [autosave])

  // Search requests can resolve out of order. Only the newest request is
  // allowed to publish its results.
  const searchGenerationRef = useRef(0)
  const loadNotes = useCallback(
    async (nextQuery = query, includeTrashed = view === 'trash') => {
      const generation = (searchGenerationRef.current += 1)
      const result = nextQuery.trim()
        ? await notesApi.searchNotesFiltered(nextQuery.trim(), includeTrashed, view === 'archive')
        : await notesApi.listNotesFiltered(includeTrashed, view === 'archive')
      if (generation !== searchGenerationRef.current) return result
      setNotes(result)
      return result
    },
    [query, view],
  )

  const selectNote = useCallback(
    async (path: string) => {
      // Never drop an unsaved buffer to load another note.
      if (!(await flushPending())) return
      try {
        const doc = await notesApi.getNote(path)
        activateDocument(doc)
        setError('')
        setConflict(null)
        const [nextBacklinks, nextRevisions, nextOutgoingLinks] = await Promise.all([
          notesApi.getBacklinks(path),
          notesApi.listRevisions(path),
          notesApi.getOutgoingLinks(path),
        ])
        setBacklinks(nextBacklinks)
        setRevisions(nextRevisions)
        setOutgoingLinks(nextOutgoingLinks)
      } catch (nextError: unknown) {
        setError(displayError(nextError))
      }
    },
    [activateDocument, flushPending],
  )

  const refresh = useCallback(
    async (preferredPath?: string) => {
      try {
        const nextNotes = await loadNotes('', view === 'trash')
        const path = preferredPath ?? activeDocument?.path ?? nextNotes[0]?.path
        if (path && nextNotes.some((note) => note.path === path)) await selectNote(path)
        else if (!nextNotes.length) {
          setActiveDocument(null)
          setContent('')
          autosave.deactivate()
          setBacklinks([])
          setRevisions([])
          setOutgoingLinks([])
        }
      } catch (nextError: unknown) {
        setError(displayError(nextError))
      }
    },
    [activeDocument?.path, autosave, loadNotes, selectNote, view],
  )

  useEffect(() => {
    void refresh()
    void notesApi
      .listTags()
      .then(setTags)
      .catch(() => setTags([]))
  }, [refresh])

  // Run the search. Nothing previously watched `query`, so typing in the search
  // box updated the input and changed nothing else: the note list only ever
  // re-queried when the Library changed underneath it.
  useEffect(() => {
    const trimmed = query.trim()
    const timer = window.setTimeout(
      () => {
        void loadNotes(query, view === 'trash').catch((nextError: unknown) =>
          setError(displayError(nextError)),
        )
      },
      trimmed ? 180 : 0,
    )
    return () => window.clearTimeout(timer)
    // `loadNotes` closes over the current query and view; depending on it here
    // would re-run this effect on every note-list change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, view])

  useEffect(() => {
    if (!isDesktop()) return
    let unlisten: (() => void) | undefined
    void listen('library-changed', () => {
      void loadNotes(query, view === 'trash')
      void notesApi
        .listTags()
        .then(setTags)
        .catch(() => undefined)
    }).then((dispose) => {
      unlisten = dispose
    })
    return () => unlisten?.()
  }, [loadNotes, query, view])

  useEffect(() => {
    if (view === 'tasks') {
      notesApi
        .listTasks(true)
        .then(setTasks)
        .catch((nextError: unknown) => setError(displayError(nextError)))
    }
    if (view === 'graph') {
      notesApi
        .getGraph()
        .then(setGraph)
        .catch((nextError: unknown) => setError(displayError(nextError)))
    }
  }, [view])

  useEffect(() => {
    if (!activeDocument || saveState !== 'dirty') return
    const timer = window.setTimeout(() => {
      void notesApi.saveRecoveryDraft(activeDocument.path, content)
    }, 5000)
    return () => window.clearTimeout(timer)
  }, [activeDocument, content, saveState])

  // Flush an in-progress edit when the window is going away. `beforeunload`
  // fires for both the browser dev shell and a Tauri window close.
  useEffect(() => {
    const handler = (event: BeforeUnloadEvent) => {
      if (!autosave.isDirty) return
      void autosave.flush()
      event.preventDefault()
      event.returnValue = ''
    }
    window.addEventListener('beforeunload', handler)
    return () => window.removeEventListener('beforeunload', handler)
  }, [autosave])

  const createNote = async (startingTitle = 'Untitled') => {
    try {
      const doc = await notesApi.createNote(startingTitle, 'markdown')
      setView('all')
      activateDocument(doc)
      setNotes((current) => [doc, ...current.filter((note) => note.path !== doc.path)])
      setBacklinks([])
      setRevisions([])
      setOutgoingLinks([])
      setMessage('New note')
      window.setTimeout(() => editorRef.current?.focus(), 50)
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const createMissingLink = async (targetPath: string) => {
    const segments = targetPath.split('/').filter(Boolean)
    const fileName = segments.pop() || 'Untitled.md'
    const title = fileName.replace(/\.(md|txt)$/i, '') || 'Untitled'
    try {
      const doc = await notesApi.createNote(title, 'markdown', segments.join('/'))
      setView('all')
      await selectNote(doc.path)
      await refresh(doc.path)
      setMessage('Linked note created')
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const forceSave = async (): Promise<boolean> => {
    if (!activeDocument) return false
    const saved = await autosave.flush()
    if (saved) setRevisions(await notesApi.listRevisions(activeDocument.path))
    return saved
  }

  const renameNote = async () => {
    if (!activeDocument || !titleDraft.trim() || titleDraft.trim() === activeDocument.title) return
    try {
      const doc = await notesApi.renameNote(activeDocument.path, titleDraft.trim())
      activateDocument(doc)
      setNotes((current) => current.map((note) => (note.path === activeDocument.path ? doc : note)))
      setBacklinks(await notesApi.getBacklinks(doc.path))
      setRevisions(await notesApi.listRevisions(doc.path))
      setOutgoingLinks(await notesApi.getOutgoingLinks(doc.path))
    } catch (nextError: unknown) {
      setTitleDraft(activeDocument.title)
      setError(displayError(nextError))
    }
  }

  const createDailyNote = async () => {
    try {
      const doc = await notesApi.createDailyNote(localCalendarDate())
      setView('all')
      activateDocument(doc)
      await refresh(doc.path)
      window.setTimeout(() => editorRef.current?.focus(), 50)
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const moveNote = async () => {
    if (!activeDocument) return
    const folder = window.prompt('Move this note to a Library folder (leave blank for root):', '')
    if (folder === null) return
    try {
      const doc = await notesApi.moveNote(activeDocument.path, folder.trim())
      activateDocument(doc)
      await refresh(doc.path)
      setMessage('Note moved')
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const archiveNote = async () => {
    if (!activeDocument) return
    try {
      const archived = !activeDocument.archived
      const doc = await notesApi.archiveNote(activeDocument.path, archived)
      setMessage(archived ? 'Note archived' : 'Note restored to All notes')
      if (archived && view !== 'archive') {
        setActiveDocument(null)
        setContent('')
        autosave.deactivate()
        setBacklinks([])
        setRevisions([])
        setOutgoingLinks([])
        await refresh()
      } else {
        activateDocument(doc)
        await refresh(doc.path)
      }
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const attachFiles = async () => {
    if (!activeDocument) return
    try {
      const chosen = await open({
        multiple: true,
        filters: [
          {
            name: 'Attachments',
            extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'pdf', 'zip', 'docx', 'xlsx'],
          },
        ],
      })
      const paths = Array.isArray(chosen) ? chosen : chosen ? [chosen] : []
      if (!paths.length) return
      const links: string[] = []
      for (const path of paths) {
        const attachment = await notesApi.importAttachment(path)
        const relative = relativeLibraryPath(activeDocument.path, attachment.path)
        const name = attachment.path.split('/').pop() || 'attachment'
        const image = /\.(png|jpe?g|gif|webp|svg)$/i.test(attachment.path)
        links.push(image ? '![' + name + '](' + relative + ')' : '[' + name + '](' + relative + ')')
      }
      editContent(content + (content.endsWith('\n') ? '' : '\n') + '\n' + links.join('\n') + '\n')
      setMessage(links.length + ' attachment' + (links.length === 1 ? '' : 's') + ' added')
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const moveToTrash = async () => {
    if (!activeDocument) return
    try {
      if (!(await forceSave())) return
      await notesApi.trashNote(activeDocument.path)
      await refresh()
      setMessage('Moved to Trash')
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const toggleTask = async (task: TaskItem) => {
    try {
      const doc = await notesApi.toggleTask(task.id, !task.checked)
      if (activeDocument?.path === doc.path) {
        activateDocument(doc)
      }
      setTasks((current) =>
        current.map((item) => (item.id === task.id ? { ...item, checked: !item.checked } : item)),
      )
      await loadNotes()
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const openImport = async () => {
    try {
      const chosen = await open({
        multiple: true,
        filters: [{ name: 'Notes', extensions: ['md', 'txt'] }],
      })
      const paths = Array.isArray(chosen) ? chosen : chosen ? [chosen] : []
      if (!paths.length) return
      const imported = await notesApi.importFiles(paths)
      await refresh(imported[0]?.path)
      setMessage(`${imported.length} note${imported.length === 1 ? '' : 's'} imported`)
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const exportBackup = async () => exportBackupWithMode(false)

  const exportBackupWithMode = async (includeInternal: boolean) => {
    try {
      const destination = await save({
        defaultPath: includeInternal ? 'Cinqic Notes full backup.zip' : 'Cinqic Notes backup.zip',
        filters: [{ name: 'ZIP backup', extensions: ['zip'] }],
      })
      if (!destination) return
      await notesApi.backupLibrary(destination, includeInternal)
      setMessage(includeInternal ? 'Full backup exported' : 'Library backup exported')
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const recoverDraft = async (draft: RecoveryDraftInfo) => {
    try {
      const [doc, draftContent] = await Promise.all([
        notesApi.getNote(draft.notePath),
        notesApi.readRecoveryDraft(draft.path),
      ])
      setView('all')
      setShowSettings(false)
      activateDocument(doc, draftContent)
      setConflict(null)
      setBacklinks(await notesApi.getBacklinks(doc.path))
      setRevisions(await notesApi.listRevisions(doc.path))
      setOutgoingLinks(await notesApi.getOutgoingLinks(doc.path))
      setMessage('Recovery draft opened — review before saving')
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  /** Switch the workspace to a Library that already exists on disk. */
  const openLibraryAt = useCallback(
    async (path: string, notice: string) => {
      const nextLibrary = await notesApi.openLibrary(path)
      await notesApi.setLastLibrary(path)
      setLibrary(nextLibrary)
      setActiveDocument(null)
      setContent('')
      autosave.deactivate()
      setTitleDraft('')
      setNotes([])
      setBacklinks([])
      setOutgoingLinks([])
      setRevisions([])
      setConflict(null)
      setView('all')
      setQuery('')
      setTagFilter('')
      setFormatFilter('all')
      setTaskFilter('all')
      const [nextNotes, nextTags] = await Promise.all([
        notesApi.listNotesFiltered(false, false),
        notesApi.listTags(),
      ])
      setNotes(nextNotes)
      setTags(nextTags)
      setMessage(notice)
    },
    [autosave],
  )

  const changeLibrary = async () => {
    if (!(await flushPending())) return
    try {
      const chosen = await open({ directory: true, multiple: false })
      const path = Array.isArray(chosen) ? chosen[0] : chosen
      if (!path || path === library.path) return
      await openLibraryAt(path, 'Library changed')
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  /**
   * Restore a backup into a new, empty folder.
   *
   * The active Library is never touched: the backend requires a destination
   * outside it and refuses a non-empty folder, and opening the result is a
   * separate, explicit step for the user.
   */
  const restoreBackup = async (): Promise<string | null> => {
    const chosenArchive = await open({
      multiple: false,
      filters: [{ name: 'ZIP backup', extensions: ['zip'] }],
    })
    const archive = Array.isArray(chosenArchive) ? chosenArchive[0] : chosenArchive
    if (!archive) return null
    const chosenDestination = await open({ directory: true, multiple: false })
    const destination = Array.isArray(chosenDestination) ? chosenDestination[0] : chosenDestination
    if (!destination) return null
    await notesApi.restoreBackup(archive, destination)
    return destination
  }

  const openRestoredLibrary = async (path: string) => {
    if (!(await flushPending())) return
    try {
      await openLibraryAt(path, 'Restored Library opened')
      setShowSettings(false)
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const selectView = (nextView: View) => {
    setView(nextView)
    setQuery('')
    setTagFilter('')
    setFormatFilter('all')
    setTaskFilter('all')
    setError('')
    if (nextView === 'settings') setShowSettings(true)
    if (
      nextView === 'all' ||
      nextView === 'today' ||
      nextView === 'projects' ||
      nextView === 'archive' ||
      nextView === 'trash'
    ) {
      void loadNotes('', nextView === 'trash')
    }
  }

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const modifier = event.ctrlKey || event.metaKey
      if (!modifier) return
      if (event.key.toLowerCase() === 'n') {
        event.preventDefault()
        void createNote()
      } else if (event.key.toLowerCase() === 's') {
        event.preventDefault()
        void forceSave()
      } else if (event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setShowCommandPalette(true)
      } else if (
        activeDocument &&
        editorRef.current === document.activeElement &&
        (event.key.toLowerCase() === 'b' || event.key.toLowerCase() === 'i')
      ) {
        event.preventDefault()
        const editor = editorRef.current
        const start = editor.selectionStart
        const end = editor.selectionEnd
        const marker = event.key.toLowerCase() === 'b' ? '**' : '*'
        const selected = content.slice(start, end)
        editContent(content.slice(0, start) + marker + selected + marker + content.slice(end))
        requestAnimationFrame(() => {
          editor.focus()
          editor.setSelectionRange(start + marker.length, end + marker.length)
        })
      } else if (event.key.toLowerCase() === 'f' && document.activeElement !== editorRef.current) {
        event.preventDefault()
        document.querySelector<HTMLInputElement>('.search-input')?.focus()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  })

  const filteredNotes = useMemo(() => {
    let result = notes
    if (view === 'today') {
      const today = new Date().toDateString()
      result = result.filter((note) => new Date(note.modifiedAt).toDateString() === today)
    }
    if (view === 'projects') result = result.filter((note) => note.project)
    if (view === 'archive') result = result.filter((note) => note.archived)
    if (tagFilter) result = result.filter((note) => note.tags.includes(tagFilter))
    if (formatFilter !== 'all') result = result.filter((note) => note.format === formatFilter)
    if (taskFilter === 'tasks') result = result.filter((note) => note.taskCount > 0)
    if (taskFilter === 'open') result = result.filter((note) => note.openTaskCount > 0)
    return result
  }, [formatFilter, notes, tagFilter, taskFilter, view])

  const saveLabel =
    saveState === 'saved'
      ? 'Saved locally'
      : saveState === 'dirty'
        ? 'Unsaved changes'
        : saveState === 'saving'
          ? 'Saving…'
          : saveState === 'conflict'
            ? 'Conflict'
            : 'Save error'
  const commandActions: Array<[string, () => void | Promise<void>]> = [
    ['New note', () => void createNote()],
    ['Daily note', () => void createDailyNote()],
    ['Quick scratch note', () => void createNote('Scratch')],
    ['Move current note…', () => void moveNote()],
    ['Archive current note', () => void archiveNote()],
    ['Export Library backup', () => void exportBackup()],
    [
      'Search notes',
      () => {
        setShowCommandPalette(false)
        document.querySelector<HTMLInputElement>('.search-input')?.focus()
      },
    ],
    ['Toggle preview', () => setShowPreview(!showPreview)],
    [
      'Rebuild index',
      async () => {
        const next = await notesApi.rebuildIndex()
        setLibrary(next)
        setShowCommandPalette(false)
        setMessage('Index rebuilt from your files')
      },
    ],
    [
      'Open Settings',
      () => {
        setShowSettings(true)
        setView('settings')
        setShowCommandPalette(false)
      },
    ],
  ]

  return (
    <div className="app-frame">
      {showSidebar && (
        <aside className="sidebar" aria-label="Library navigation">
          <div className="sidebar-brand">
            <div className="brand-mark" aria-hidden="true">
              <span>cn</span>
            </div>
            <div>
              <strong>Cinqic Notes</strong>
              <small>Local Library</small>
            </div>
          </div>
          <button className="button primary new-note-button" onClick={() => void createNote()}>
            <span>＋</span> New note
          </button>
          <nav className="primary-nav">
            <NavItem
              icon="⌂"
              label="All notes"
              active={view === 'all'}
              onClick={() => selectView('all')}
              count={notes.length}
            />
            <NavItem
              icon="◷"
              label="Today"
              active={view === 'today'}
              onClick={() => selectView('today')}
            />
            <NavItem
              icon="▱"
              label="Projects"
              active={view === 'projects'}
              onClick={() => selectView('projects')}
            />
            <NavItem
              icon="□"
              label="Archive"
              active={view === 'archive'}
              onClick={() => selectView('archive')}
            />
            <NavItem
              icon="☑"
              label="Tasks"
              active={view === 'tasks'}
              onClick={() => selectView('tasks')}
            />
            <NavItem
              icon="⌘"
              label="Graph"
              active={view === 'graph'}
              onClick={() => selectView('graph')}
            />
          </nav>
          <div className="sidebar-spacer" />
          <button
            className="library-location"
            title={library.path}
            onClick={() => setShowSettings(true)}
          >
            <span className="location-dot" aria-hidden="true" />
            <span>
              <strong>{library.path.split(/[\\/]/).pop() || 'Notes Library'}</strong>
              <small>On this device</small>
            </span>
          </button>
          <nav className="secondary-nav">
            <NavItem
              icon="⌫"
              label="Trash"
              active={view === 'trash'}
              onClick={() => selectView('trash')}
            />
            <NavItem
              icon="⚙"
              label="Settings"
              active={showSettings || view === 'settings'}
              onClick={() => {
                setView('settings')
                setShowSettings(true)
              }}
            />
          </nav>
          <div className="privacy-note">
            <span aria-hidden="true">✓</span>
            <div>
              <strong>Private by default</strong>
              <small>No network or AI required.</small>
            </div>
          </div>
        </aside>
      )}

      <main className="main-pane">
        <header className="topbar">
          <div className="topbar-left">
            <button
              className="icon-button"
              aria-label="Toggle navigation"
              onClick={() => setShowSidebar(!showSidebar)}
            >
              ☰
            </button>
            <span className="topbar-library">
              {library.path.split(/[\\/]/).pop() || 'Notes Library'}
            </span>
          </div>
          <div className="topbar-actions">
            <span className={`save-state ${saveState}`}>
              <i aria-hidden="true" />
              {saveLabel}
            </span>
            <button className="button quiet" onClick={() => setShowCommandPalette(true)}>
              <span>⌘</span> Command
            </button>
            <button
              className="button quiet"
              onClick={() => setShowShare(true)}
              disabled={!activeDocument}
            >
              Share
            </button>
          </div>
        </header>

        {view === 'tasks' ? (
          <TasksView
            tasks={tasks}
            onToggle={toggleTask}
            onOpen={(path) => {
              setView('all')
              void selectNote(path)
            }}
          />
        ) : view === 'graph' ? (
          <GraphView
            graph={graph}
            onOpen={(path) => {
              setView('all')
              void selectNote(path)
            }}
          />
        ) : view === 'trash' ? (
          <TrashView
            notes={notes}
            onEmpty={async () => {
              if (!window.confirm('Permanently remove every note in Trash?')) return
              try {
                const removed = await notesApi.emptyTrash()
                await refresh()
                setMessage(`${removed} note${removed === 1 ? '' : 's'} permanently removed`)
              } catch (nextError: unknown) {
                setError(displayError(nextError))
              }
            }}
            onRestore={async (path) => {
              try {
                const doc = await notesApi.restoreNote(path)
                setView('all')
                activateDocument(doc)
                await refresh(doc.path)
                setMessage('Note restored')
              } catch (nextError: unknown) {
                setError(displayError(nextError))
              }
            }}
          />
        ) : view === 'settings' && showSettings ? (
          <SettingsView
            theme={theme}
            setTheme={setTheme}
            library={library}
            onBackup={exportBackupWithMode}
            onRestoreBackup={restoreBackup}
            onOpenRestored={openRestoredLibrary}
            onChangeLibrary={changeLibrary}
            onRecoverDraft={recoverDraft}
            onRebuild={async () => {
              const next = await notesApi.rebuildIndex()
              setLibrary(next)
              await refresh()
              setMessage('Index rebuilt from your files')
            }}
          />
        ) : (
          <div className={`workspace ${showList ? '' : 'list-collapsed'}`}>
            {showList && (
              <NoteList
                notes={filteredNotes}
                tags={tags}
                activePath={activeDocument?.path}
                query={query}
                tagFilter={tagFilter}
                setTagFilter={setTagFilter}
                formatFilter={formatFilter}
                setFormatFilter={setFormatFilter}
                taskFilter={taskFilter}
                setTaskFilter={setTaskFilter}
                setQuery={(next) => {
                  setQuery(next)
                  void loadNotes(next, view === 'trash')
                }}
                onSelect={(path) => void selectNote(path)}
                onImport={() => void openImport()}
                view={view}
              />
            )}
            <EditorPane
              document={activeDocument}
              onExternalLink={(url) => void openExternalLink(url)}
              content={content}
              setContent={(next) => {
                editContent(next)
              }}
              titleDraft={titleDraft}
              setTitleDraft={setTitleDraft}
              onTitleBlur={() => void renameNote()}
              showPreview={showPreview}
              setShowPreview={setShowPreview}
              onToggleList={() => setShowList(!showList)}
              onSave={() => void forceSave()}
              onTrash={() => void moveToTrash()}
              onMove={() => void moveNote()}
              onArchive={() => void archiveNote()}
              onAttach={() => void attachFiles()}
              backlinks={backlinks}
              outgoingLinks={outgoingLinks}
              onOpenLink={(path) => {
                setView('all')
                void selectNote(path)
              }}
              onCreateLink={(path) => void createMissingLink(path)}
              error={error}
              message={message}
              editorRef={editorRef}
              onResolveConflict={(resolution) => {
                if (activeDocument)
                  void notesApi
                    .resolveConflict(activeDocument.path, resolution)
                    .then((doc) => {
                      activateDocument(doc)
                      setConflict(null)
                      return Promise.all([
                        notesApi.getBacklinks(doc.path),
                        notesApi.listRevisions(doc.path),
                        notesApi.getOutgoingLinks(doc.path),
                      ]).then(([nextBacklinks, nextRevisions, nextOutgoingLinks]) => {
                        setBacklinks(nextBacklinks)
                        setRevisions(nextRevisions)
                        setOutgoingLinks(nextOutgoingLinks)
                      })
                    })
                    .catch((nextError: unknown) => setError(displayError(nextError)))
              }}
              conflict={conflict}
              revisions={revisions}
              onRestoreRevision={async (revisionId) => {
                if (!activeDocument) return
                try {
                  const doc = await notesApi.restoreRevision(activeDocument.path, revisionId)
                  activateDocument(doc)
                  setBacklinks(await notesApi.getBacklinks(doc.path))
                  setRevisions(await notesApi.listRevisions(doc.path))
                  setOutgoingLinks(await notesApi.getOutgoingLinks(doc.path))
                  setMessage('Revision restored')
                } catch (nextError: unknown) {
                  setError(displayError(nextError))
                }
              }}
              onCopyRevision={(revision) => {
                if (!navigator.clipboard?.writeText) {
                  setError('Clipboard is unavailable here.')
                  return
                }
                void navigator.clipboard
                  .writeText(revision.content)
                  .then(() => setMessage('Revision copied'))
                  .catch(() => setError('Could not copy — the clipboard was refused.'))
              }}
            />
          </div>
        )}
      </main>

      {showShare && activeDocument && (
        <ShareDialog
          document={activeDocument}
          content={content}
          onClose={() => setShowShare(false)}
          onExport={async (format) => {
            const destination = await save({
              defaultPath: `${activeDocument.title}.${format}`,
              filters: [{ name: format.toUpperCase(), extensions: [format] }],
            })
            if (destination) {
              await notesApi.exportNote(activeDocument.path, destination, format)
              setMessage('Exported locally')
              setShowShare(false)
            }
          }}
        />
      )}
      {showCommandPalette && (
        <CommandPalette onClose={() => setShowCommandPalette(false)} actions={commandActions} />
      )}
    </div>
  )
}

function NavItem({
  icon,
  label,
  active,
  onClick,
  count,
}: {
  icon: string
  label: string
  active: boolean
  onClick: () => void
  count?: number
}) {
  return (
    <button className={`nav-item ${active ? 'active' : ''}`} onClick={onClick}>
      <span className="nav-icon" aria-hidden="true">
        {icon}
      </span>
      <span>{label}</span>
      {count ? <em>{count}</em> : null}
    </button>
  )
}

function NoteList({
  notes,
  tags,
  activePath,
  query,
  tagFilter,
  setTagFilter,
  formatFilter,
  setFormatFilter,
  taskFilter,
  setTaskFilter,
  setQuery,
  onSelect,
  onImport,
  view,
}: {
  notes: NoteSummary[]
  tags: TagItem[]
  activePath?: string
  query: string
  tagFilter: string
  setTagFilter: (value: string) => void
  formatFilter: 'all' | 'markdown' | 'text'
  setFormatFilter: (value: 'all' | 'markdown' | 'text') => void
  taskFilter: 'all' | 'tasks' | 'open'
  setTaskFilter: (value: 'all' | 'tasks' | 'open') => void
  setQuery: (value: string) => void
  onSelect: (path: string) => void
  onImport: () => void
  view: View
}) {
  return (
    <section className="note-list-panel" aria-label="Notes">
      <div className="list-heading">
        <div>
          <span className="eyebrow">
            {view === 'trash' || view === 'archive' ? 'RECOVERABLE' : 'YOUR LIBRARY'}
          </span>
          <h2>
            {view === 'trash'
              ? 'Trash'
              : view === 'projects'
                ? 'Projects'
                : view === 'archive'
                  ? 'Archive'
                  : 'Notes'}
          </h2>
        </div>
        <button className="icon-button" aria-label="Import notes" onClick={onImport}>
          ↥
        </button>
      </div>
      <label className="search-box">
        <span aria-hidden="true">⌕</span>
        <input
          className="search-input"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search notes"
          aria-label="Search notes"
        />
        <kbd>{searchShortcutLabel()}</kbd>
      </label>
      <div className="note-filters" aria-label="Note filters">
        <label>
          <span>Tag</span>
          <select value={tagFilter} onChange={(event) => setTagFilter(event.target.value)}>
            <option value="">All tags</option>
            {tags.map((tag) => (
              <option key={tag.tag} value={tag.tag}>
                #{tag.tag} ({tag.noteCount})
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>Type</span>
          <select
            value={formatFilter}
            onChange={(event) => setFormatFilter(event.target.value as 'all' | 'markdown' | 'text')}
          >
            <option value="all">All types</option>
            <option value="markdown">Markdown</option>
            <option value="text">Plain text</option>
          </select>
        </label>
        <label>
          <span>Tasks</span>
          <select
            value={taskFilter}
            onChange={(event) => setTaskFilter(event.target.value as 'all' | 'tasks' | 'open')}
          >
            <option value="all">Any tasks</option>
            <option value="tasks">Has tasks</option>
            <option value="open">Has open tasks</option>
          </select>
        </label>
      </div>
      <div className="note-list">
        {notes.map((note) => (
          <button
            key={note.path}
            className={`note-list-item ${activePath === note.path ? 'selected' : ''}`}
            onClick={() => onSelect(note.path)}
          >
            <span className="note-file-icon">{note.format === 'markdown' ? 'M' : 'T'}</span>
            <span className="note-list-copy">
              <strong>{note.title}</strong>
              <small>{note.preview || 'No preview yet'}</small>
              <span>
                {formatRelativeDate(note.modifiedAt)}
                {note.tags.length
                  ? ` · ${note.tags
                      .slice(0, 2)
                      .map((tag) => `#${tag}`)
                      .join(' ')}`
                  : ''}
              </span>
            </span>
            {note.openTaskCount > 0 && <em className="task-count">{note.openTaskCount}</em>}
          </button>
        ))}
        {!notes.length && (
          <div className="empty-list">
            <span>∅</span>
            <p>
              {query
                ? 'No notes match that search.'
                : tagFilter || formatFilter !== 'all' || taskFilter !== 'all'
                  ? 'No notes match these filters.'
                  : view === 'trash'
                    ? 'Trash is empty.'
                    : 'No notes yet.'}
            </p>
            <small>
              {query
                ? 'Try a different word or phrase.'
                : tagFilter || formatFilter !== 'all' || taskFilter !== 'all'
                  ? 'Try clearing one of the filters.'
                  : 'Create a note or import a Markdown folder.'}
            </small>
          </div>
        )}
      </div>
    </section>
  )
}

function TrashView({
  notes,
  onRestore,
  onEmpty,
}: {
  notes: NoteSummary[]
  onRestore: (path: string) => Promise<void>
  onEmpty: () => Promise<void>
}) {
  const trashed = notes.filter((note) => note.trashed)
  return (
    <section className="utility-view">
      <div className="utility-heading">
        <span className="eyebrow">RECOVERABLE</span>
        <h1>Trash</h1>
        <p>Deleted notes stay local until you choose to remove them from the Library.</p>
      </div>
      <div className="trash-list">
        {trashed.length > 0 && (
          <div className="trash-actions">
            <button className="button secondary small" onClick={() => void onEmpty()}>
              Empty Trash
            </button>
          </div>
        )}
        {trashed.map((note) => (
          <div className="trash-row" key={note.id}>
            <span className="note-file-icon">{note.format === 'markdown' ? 'M' : 'T'}</span>
            <span>
              <strong>{note.title}</strong>
              <small>{note.path}</small>
            </span>
            <button className="button secondary small" onClick={() => void onRestore(note.path)}>
              Restore
            </button>
          </div>
        ))}
        {!trashed.length && (
          <div className="empty-utility">
            <span>⌫</span>
            <p>Trash is empty.</p>
            <small>Deleted notes will appear here while they are recoverable.</small>
          </div>
        )}
      </div>
    </section>
  )
}

function EditorPane({
  document,
  onExternalLink,
  content,
  setContent,
  titleDraft,
  setTitleDraft,
  onTitleBlur,
  showPreview,
  setShowPreview,
  onToggleList,
  onSave,
  onTrash,
  onMove,
  onArchive,
  onAttach,
  backlinks,
  outgoingLinks,
  onOpenLink,
  onCreateLink,
  error,
  message,
  editorRef,
  conflict,
  onResolveConflict,
  revisions,
  onRestoreRevision,
  onCopyRevision,
}: {
  document: NoteDocument | null
  onExternalLink: (url: string) => void
  content: string
  setContent: (value: string) => void
  titleDraft: string
  setTitleDraft: (value: string) => void
  onTitleBlur: () => void
  showPreview: boolean
  setShowPreview: (value: boolean) => void
  onToggleList: () => void
  onSave: () => void
  onTrash: () => void
  onMove: () => void
  onArchive: () => void
  onAttach: () => void
  backlinks: BacklinkItem[]
  outgoingLinks: LinkItem[]
  onOpenLink: (path: string) => void
  onCreateLink: (path: string) => void
  error: string
  message: string
  editorRef: React.RefObject<HTMLTextAreaElement | null>
  conflict: ConflictInfo | null
  onResolveConflict: (resolution: 'local' | 'disk') => void
  revisions: RevisionItem[]
  onRestoreRevision: (revisionId: string) => Promise<void>
  onCopyRevision: (revision: RevisionItem) => void
}) {
  /**
   * Activate a rendered external link. The preview never emits a live href, so
   * nothing happens unless the user deliberately activates the element, and the
   * host decides what "opening" means rather than the WebView navigating.
   */
  const handlePreviewActivate = (
    event: ReactMouseEvent<HTMLElement> | ReactKeyboardEvent<HTMLElement>,
  ) => {
    const target = (event.target as HTMLElement | null)?.closest<HTMLElement>(
      '[data-external-href]',
    )
    const url = target?.dataset.externalHref
    if (!url) return
    event.preventDefault()
    onExternalLink(url)
  }

  const [showMeta, setShowMeta] = useState(false)
  const [showHistory, setShowHistory] = useState(false)
  if (!document)
    return (
      <section className="editor-empty">
        <div className="empty-glyph">✎</div>
        <h1>Make space for a thought.</h1>
        <p>
          Choose a note, or start a new one. Your writing is saved to your Library on this device.
        </p>
        <button
          className="button primary"
          onClick={() =>
            window.document.querySelector<HTMLButtonElement>('.new-note-button')?.click()
          }
        >
          Create a note
        </button>
      </section>
    )
  return (
    <section className="editor-pane">
      <div className="editor-toolbar">
        <button className="icon-button" aria-label="Toggle notes list" onClick={onToggleList}>
          ☷
        </button>
        <div className="toolbar-spacer" />
        <button
          className={`toolbar-button ${showPreview ? 'active' : ''}`}
          onClick={() => setShowPreview(!showPreview)}
        >
          {showPreview ? 'Edit' : 'Preview'}
        </button>
        <button className="toolbar-button" onClick={() => setShowHistory(!showHistory)}>
          History{revisions.length ? ` · ${revisions.length}` : ''}
        </button>
        <button className="toolbar-button" onClick={onSave}>
          Save
        </button>
        <button className="toolbar-button" onClick={onAttach}>
          Attach
        </button>
        <button
          className="icon-button"
          aria-label="More note actions"
          onClick={() => setShowMeta(!showMeta)}
        >
          •••
        </button>
        {showMeta && (
          <div className="more-menu">
            <button onClick={onMove}>Move to folder…</button>
            <button onClick={onArchive}>
              {document.archived ? 'Unarchive note' : 'Archive note'}
            </button>
            <button onClick={onTrash}>Move to Trash</button>
            <button onClick={() => setShowMeta(false)}>Close menu</button>
          </div>
        )}
      </div>
      <div className="document-wrap">
        <input
          className="document-title"
          value={titleDraft}
          onChange={(event) => setTitleDraft(event.target.value)}
          onBlur={onTitleBlur}
          aria-label="Note title"
        />
        <div className="document-path">
          {document.path} <span>·</span>{' '}
          {document.format === 'markdown' ? 'Markdown' : 'Plain text'}
        </div>
        {error && (
          <div className="inline-alert" role="alert">
            <span>!</span>
            {error}
          </div>
        )}
        {message && (
          <div className="toast-message" role="status">
            {message}
          </div>
        )}
        {conflict && <ConflictBanner conflict={conflict} onResolve={onResolveConflict} />}
        {showHistory && (
          <RevisionPanel
            revisions={revisions}
            onRestore={onRestoreRevision}
            onCopy={onCopyRevision}
          />
        )}
        {showPreview ? (
          <article
            className="markdown-preview"
            onClick={handlePreviewActivate}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') handlePreviewActivate(event)
            }}
            dangerouslySetInnerHTML={{ __html: renderSafeMarkdown(content, document.format) }}
          />
        ) : (
          <textarea
            ref={editorRef}
            className="document-editor"
            value={content}
            onChange={(event) => setContent(event.target.value)}
            spellCheck={false}
            aria-label="Note content"
            placeholder="Start writing…"
          />
        )}
        {backlinks.length > 0 && (
          <div className="backlinks">
            <div className="section-label">
              BACKLINKS <span>{backlinks.length}</span>
            </div>
            {backlinks.map((link) => (
              <button
                key={`${link.sourcePath}-${link.label}`}
                className="backlink-item"
                onClick={() => onOpenLink(link.sourcePath)}
              >
                <span>↗</span>
                <span>
                  <strong>{link.sourceTitle}</strong>
                  <small>{link.label}</small>
                </span>
              </button>
            ))}
          </div>
        )}
        {outgoingLinks.length > 0 && (
          <div className="backlinks outgoing-links">
            <div className="section-label">
              OUTGOING LINKS <span>{outgoingLinks.length}</span>
            </div>
            {outgoingLinks.map((link) => (
              <div className="outgoing-link-row" key={link.targetPath + '-' + link.label}>
                <button
                  className="backlink-item"
                  disabled={!link.resolved}
                  onClick={() => link.resolved && onOpenLink(link.targetPath)}
                >
                  <span>{link.resolved ? '↘' : '?'}</span>
                  <span>
                    <strong>{link.targetTitle || link.targetPath}</strong>
                    <small>
                      {link.label}
                      {link.resolved ? '' : ' · unresolved'}
                    </small>
                  </span>
                </button>
                {!link.resolved && (
                  <button
                    className="button quiet small link-create-button"
                    onClick={() => onCreateLink(link.targetPath)}
                  >
                    Create note
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  )
}

function RevisionPanel({
  revisions,
  onRestore,
  onCopy,
}: {
  revisions: RevisionItem[]
  onRestore: (revisionId: string) => Promise<void>
  onCopy: (revision: RevisionItem) => void
}) {
  return (
    <div className="revision-panel">
      <div className="section-label">LOCAL VERSION HISTORY</div>
      {revisions.length ? (
        revisions.map((revision) => (
          <div className="revision-row" key={revision.id}>
            <div>
              <strong>
                {formatRelativeDate(revision.createdAt)} · {revision.reason}
              </strong>
              <small>
                {revision.actor === 'local' ? 'You' : revision.actor} ·{' '}
                {revision.content.split(/\r?\n/).length} lines
              </small>
            </div>
            <div className="revision-actions">
              <details className="revision-preview">
                <summary>View</summary>
                <pre>{revision.content}</pre>
              </details>
              <button className="button quiet small" onClick={() => onCopy(revision)}>
                Copy
              </button>
              <button
                className="button secondary small"
                onClick={() => void onRestore(revision.id)}
              >
                Restore
              </button>
            </div>
          </div>
        ))
      ) : (
        <p className="muted-copy">
          No earlier revision yet. Revisions are created before meaningful saves and restores.
        </p>
      )}
    </div>
  )
}

function ConflictBanner({
  conflict,
  onResolve,
}: {
  conflict: ConflictInfo
  onResolve: (resolution: 'local' | 'disk') => void
}) {
  return (
    <div className="conflict-banner">
      <strong>External change detected</strong>
      <p>
        This note changed outside Cinqic Notes. Keep your in-app draft or load the file from disk.
      </p>
      <div>
        <button className="button secondary small" onClick={() => onResolve('local')}>
          Keep my draft
        </button>
        <button className="button quiet small" onClick={() => onResolve('disk')}>
          Load disk version
        </button>
      </div>
      <details>
        <summary>Compare content</summary>
        <pre>{conflict.diskContent}</pre>
      </details>
    </div>
  )
}

function TasksView({
  tasks,
  onToggle,
  onOpen,
}: {
  tasks: TaskItem[]
  onToggle: (task: TaskItem) => void
  onOpen: (path: string) => void
}) {
  const [filter, setFilter] = useState<'all' | 'open' | 'complete'>('all')
  const visibleTasks = tasks.filter(
    (task) => filter === 'all' || (filter === 'open' ? !task.checked : task.checked),
  )
  return (
    <section className="utility-view">
      <div className="utility-heading">
        <span className="eyebrow">ORDINARY MARKDOWN</span>
        <h1>Tasks</h1>
        <p>Checklists stay in the note that owns them.</p>
      </div>
      <div className="task-filters">
        <button
          className={`filter-chip ${filter === 'all' ? 'active' : ''}`}
          onClick={() => setFilter('all')}
        >
          All {tasks.length}
        </button>
        <button
          className={`filter-chip ${filter === 'open' ? 'active' : ''}`}
          onClick={() => setFilter('open')}
        >
          Open {tasks.filter((task) => !task.checked).length}
        </button>
        <button
          className={`filter-chip ${filter === 'complete' ? 'active' : ''}`}
          onClick={() => setFilter('complete')}
        >
          Complete {tasks.filter((task) => task.checked).length}
        </button>
      </div>
      <div className="task-table">
        {visibleTasks.map((task) => (
          <div className={`task-row ${task.checked ? 'complete' : ''}`} key={task.id}>
            <button
              className="task-checkbox"
              aria-label={task.checked ? 'Mark task open' : 'Mark task complete'}
              onClick={() => onToggle(task)}
            >
              {task.checked ? '✓' : ''}
            </button>
            <span>{task.text}</span>
            <button className="task-source" onClick={() => onOpen(task.notePath)}>
              {task.noteTitle} · line {task.line}
            </button>
          </div>
        ))}
        {!visibleTasks.length && (
          <div className="empty-utility">
            <span>☑</span>
            <p>{tasks.length ? 'No tasks in this filter.' : 'No checklist items yet.'}</p>
            <small>
              {tasks.length ? 'Choose another task filter.' : 'Use `- [ ]` in any Markdown note.'}
            </small>
          </div>
        )}
      </div>
    </section>
  )
}

function GraphView({ graph, onOpen }: { graph: GraphData | null; onOpen: (path: string) => void }) {
  return (
    <section className="utility-view">
      <div className="utility-heading">
        <span className="eyebrow">RELATIONSHIPS</span>
        <h1>Graph</h1>
        <p>A readable map of links in your Library.</p>
      </div>
      {graph && (
        <div className="graph-summary">
          <span>
            <strong>{graph.nodes.length}</strong> notes
          </span>
          <span>
            <strong>{graph.edges.length}</strong> links
          </span>
        </div>
      )}
      <div className="graph-list">
        {graph?.nodes.map((node) => (
          <button key={node.path} onClick={() => onOpen(node.path)}>
            <span className="graph-node">{node.title.slice(0, 1).toUpperCase()}</span>
            <span>
              <strong>{node.title}</strong>
              <small>
                {
                  graph.edges.filter(
                    (edge) => edge.source === node.path || edge.target === node.path,
                  ).length
                }{' '}
                connections
              </small>
            </span>
          </button>
        ))}
        {graph && !graph.nodes.length && (
          <div className="empty-utility">
            <span>⌘</span>
            <p>Your graph is waiting for its first link.</p>
            <small>Type `[[Another Note]]` to connect notes.</small>
          </div>
        )}
      </div>
    </section>
  )
}

function SettingsView({
  theme,
  setTheme,
  library,
  onBackup,
  onChangeLibrary,
  onRestoreBackup,
  onOpenRestored,
  onRecoverDraft,
  onRebuild,
}: {
  theme: Theme
  setTheme: (value: Theme) => void
  library: LibraryInfo
  onBackup: (includeInternal: boolean) => Promise<void>
  onChangeLibrary: () => Promise<void>
  onRestoreBackup: () => Promise<string | null>
  onOpenRestored: (path: string) => Promise<void>
  onRecoverDraft: (draft: RecoveryDraftInfo) => Promise<void>
  onRebuild: () => Promise<void>
}) {
  const [working, setWorking] = useState(false)
  const [integrity, setIntegrity] = useState('')
  const [drafts, setDrafts] = useState<RecoveryDraftInfo[]>([])
  const [notice, setNotice] = useState('')
  const [restored, setRestored] = useState('')
  const [restoreError, setRestoreError] = useState('')

  const runRestore = async () => {
    setWorking(true)
    setRestoreError('')
    setRestored('')
    try {
      const destination = await onRestoreBackup()
      if (destination) setRestored(destination)
    } catch (nextError: unknown) {
      setRestoreError(displayError(nextError))
    } finally {
      setWorking(false)
    }
  }

  useEffect(() => {
    void notesApi
      .listRecoveryDrafts()
      .then(setDrafts)
      .catch(() => setDrafts([]))
  }, [])

  const verifyIntegrity = async () => {
    setWorking(true)
    try {
      const result = await notesApi.verifyIntegrity()
      setIntegrity(result.ok ? 'Database integrity verified.' : result.message)
    } catch (nextError: unknown) {
      setIntegrity(displayError(nextError))
    } finally {
      setWorking(false)
    }
  }

  const runBackup = async (includeInternal: boolean) => {
    setWorking(true)
    try {
      await onBackup(includeInternal)
      setNotice(includeInternal ? 'Full backup exported.' : 'Library backup exported.')
    } catch (nextError: unknown) {
      setNotice(displayError(nextError))
    } finally {
      setWorking(false)
    }
  }

  const runRebuild = async () => {
    setWorking(true)
    try {
      await onRebuild()
      setNotice('Index rebuilt from your files.')
    } catch (nextError: unknown) {
      setNotice(displayError(nextError))
    } finally {
      setWorking(false)
    }
  }

  return (
    <section className="utility-view settings-view">
      <div className="utility-heading">
        <span className="eyebrow">QUIETLY YOURS</span>
        <h1>Settings</h1>
        <p>Good defaults first. A few controls for how Cinqic Notes feels and repairs itself.</p>
      </div>
      <div className="settings-grid">
        <section className="settings-card">
          <div>
            <span className="setting-icon">◐</span>
            <div>
              <h2>Appearance</h2>
              <p>Choose how the Library looks on this device.</p>
            </div>
          </div>
          <div className="segmented">
            {(['system', 'light', 'dark'] as Theme[]).map((item) => (
              <button
                key={item}
                className={theme === item ? 'active' : ''}
                onClick={() => setTheme(item)}
              >
                {item[0].toUpperCase() + item.slice(1)}
              </button>
            ))}
          </div>
        </section>
        <section className="settings-card">
          <div>
            <span className="setting-icon">↓</span>
            <div>
              <h2>Backup</h2>
              <p>
                Export ordinary files, with an optional full local snapshot of revisions and
                settings.
              </p>
            </div>
          </div>
          <div className="setting-actions">
            <button
              className="button secondary small"
              disabled={working}
              onClick={() => void onChangeLibrary()}
            >
              Change folder…
            </button>
            <button
              className="button secondary small"
              disabled={working}
              onClick={() => void runBackup(false)}
            >
              Library ZIP
            </button>
            <button
              className="button secondary small"
              disabled={working}
              onClick={() => void runBackup(true)}
            >
              Full backup ZIP
            </button>
          </div>
          {notice && <span className="setting-status muted">{notice}</span>}
        </section>
        <section className="settings-card">
          <div>
            <span className="setting-icon">↑</span>
            <div>
              <h2>Restore a backup</h2>
              <p>
                Restore a backup ZIP into a new, empty folder. Your current Library is never
                overwritten, and the restored copy only opens when you choose to open it.
              </p>
            </div>
          </div>
          <div className="setting-actions">
            <button
              className="button secondary small"
              disabled={working}
              onClick={() => void runRestore()}
            >
              Restore from ZIP…
            </button>
            {restored && (
              <button
                className="button small"
                disabled={working}
                onClick={() => void onOpenRestored(restored)}
              >
                Open restored Library
              </button>
            )}
          </div>
          {restored && (
            <span className="setting-status muted" role="status">
              Restored to <code>{restored}</code>
            </span>
          )}
          {restoreError && (
            <span className="setting-status" role="alert">
              {restoreError}
            </span>
          )}
        </section>
        <section className="settings-card">
          <div>
            <span className="setting-icon">✓</span>
            <div>
              <h2>Repair and recovery</h2>
              <p>
                Rebuild the index, verify the local database, or reopen an autosave draft for
                review.
              </p>
            </div>
          </div>
          <div className="setting-actions">
            <button
              className="button secondary small"
              disabled={working}
              onClick={() => void verifyIntegrity()}
            >
              Verify database
            </button>
            {integrity && <span className="setting-status muted">{integrity}</span>}
          </div>
          {drafts.length > 0 && (
            <div className="recovery-list">
              <span className="section-label">AUTOSAVE DRAFTS</span>
              {drafts.map((draft) => (
                <div className="recovery-row" key={draft.path}>
                  <span>
                    <strong>{draft.notePath}</strong>
                    <small>
                      {draft.size} characters · {formatRelativeDate(draft.createdAt)}
                    </small>
                  </span>
                  <button className="button quiet small" onClick={() => void onRecoverDraft(draft)}>
                    Open draft
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>
        <section className="settings-card">
          <div>
            <span className="setting-icon">⌂</span>
            <div>
              <h2>Notes Library</h2>
              <p className="path-value" title={library.path}>
                {library.path}
              </p>
            </div>
          </div>
          <div className="setting-actions">
            <button
              className="button secondary small"
              disabled={working}
              onClick={() => void runRebuild()}
            >
              {working ? 'Rebuilding…' : 'Rebuild index'}
            </button>
            <span>
              {library.noteCount} notes · indexed {formatRelativeDate(library.lastIndexedAt)}
            </span>
          </div>
        </section>
        <section className="settings-card">
          <div>
            <span className="setting-icon">✓</span>
            <div>
              <h2>Privacy</h2>
              <p>
                Nothing in this app requires a network connection. There is no telemetry, account,
                or hidden upload.
              </p>
            </div>
          </div>
          <span className="setting-status">On this device</span>
        </section>
        <section className="settings-card">
          <div>
            <span className="setting-icon">✦</span>
            <div>
              <h2>Juniper</h2>
              <p>
                Optional integration is not enabled in this milestone. Cinqic Notes remains complete
                without AI.
              </p>
            </div>
          </div>
          <span className="setting-status muted">Disabled</span>
        </section>
      </div>
    </section>
  )
}

function ShareDialog({
  document,
  content,
  onClose,
  onExport,
}: {
  document: NoteDocument
  content: string
  onClose: () => void
  onExport: (format: 'md' | 'txt' | 'html') => Promise<void>
}) {
  const [copied, setCopied] = useState('')
  const copy = async (kind: 'markdown' | 'text') => {
    const value = kind === 'markdown' ? content : toPlainText(content)
    // Reporting success when the clipboard is unavailable — or when writeText
    // was rejected because the document was not focused or permission was
    // refused — tells the user their note is somewhere it is not.
    if (!navigator.clipboard?.writeText) {
      setCopied('Clipboard is unavailable here')
      window.setTimeout(() => setCopied(''), 2600)
      return
    }
    try {
      await navigator.clipboard.writeText(value)
    } catch {
      setCopied('Could not copy — the clipboard was refused')
      window.setTimeout(() => setCopied(''), 2600)
      return
    }
    setCopied(kind === 'markdown' ? 'Markdown copied' : 'Plain text copied')
    window.setTimeout(() => setCopied(''), 1800)
  }
  const share = async () => {
    if (!navigator.share) {
      setCopied('Platform sharing is unavailable here')
      return
    }
    try {
      await navigator.share({ title: document.title, text: content })
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') return
      setCopied('Platform sharing failed')
    }
  }
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) onClose()
      }}
    >
      <section className="dialog" role="dialog" aria-modal="true" aria-labelledby="share-title">
        <div className="dialog-heading">
          <div>
            <span className="eyebrow">YOUR FILE</span>
            <h2 id="share-title">Share or export</h2>
          </div>
          <button className="icon-button" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        <p className="dialog-copy">
          Keep this note portable. Cinqic Notes never needs to upload it.
        </p>
        <div className="share-options">
          {'share' in navigator && (
            <button onClick={() => void share()}>
              <span>↗</span>
              <strong>Share with another app</strong>
              <small>Use the device share sheet when supported</small>
            </button>
          )}
          <button onClick={() => void copy('markdown')}>
            <span>⌘</span>
            <strong>Copy Markdown</strong>
            <small>Preserves the original source</small>
          </button>
          <button onClick={() => void copy('text')}>
            <span>≡</span>
            <strong>Copy plain text</strong>
            <small>Clean text for another app</small>
          </button>
          <button onClick={() => void onExport('md')}>
            <span>↓</span>
            <strong>Export Markdown</strong>
            <small>{document.title}.md</small>
          </button>
          <button onClick={() => void onExport('txt')}>
            <span>↓</span>
            <strong>Export plain text</strong>
            <small>{document.title}.txt</small>
          </button>
          <button onClick={() => void onExport('html')}>
            <span>◇</span>
            <strong>Export safe HTML</strong>
            <small>A standalone, offline file</small>
          </button>
          <button onClick={() => window.print()}>
            <span>▣</span>
            <strong>Print</strong>
            <small>Print the current note through the platform</small>
          </button>
        </div>
        {copied && (
          <p className="copy-confirmation" role="status">
            ✓ {copied}
          </p>
        )}
      </section>
    </div>
  )
}

function CommandPalette({
  onClose,
  actions,
}: {
  onClose: () => void
  actions: Array<[string, () => void | Promise<void>]>
}) {
  const [filter, setFilter] = useState('')
  const [selected, setSelected] = useState(0)
  const listRef = useRef<HTMLDivElement>(null)
  const visible = useMemo(
    () => actions.filter(([label]) => label.toLowerCase().includes(filter.toLowerCase())),
    [actions, filter],
  )

  // Keep the selection inside the filtered list as the user types.
  const active = visible.length ? Math.min(selected, visible.length - 1) : -1
  useEffect(() => setSelected(0), [filter])

  // Follow the selection when it moves out of view via the keyboard.
  useEffect(() => {
    if (active < 0) return
    listRef.current
      ?.querySelector<HTMLElement>(`[data-index="${active}"]`)
      ?.scrollIntoView({ block: 'nearest' })
  }, [active])

  const run = (index: number) => {
    const entry = visible[index]
    if (!entry) return
    void entry[1]()
    onClose()
  }

  const onKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      onClose()
      return
    }
    if (!visible.length) return
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      setSelected((current) => (current + 1) % visible.length)
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      setSelected((current) => (current - 1 + visible.length) % visible.length)
    } else if (event.key === 'Home') {
      event.preventDefault()
      setSelected(0)
    } else if (event.key === 'End') {
      event.preventDefault()
      setSelected(visible.length - 1)
    } else if (event.key === 'Enter') {
      event.preventDefault()
      run(active)
    }
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) onClose()
      }}
    >
      <section
        className="command-palette"
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
      >
        <div className="command-search">
          <span aria-hidden="true">⌘</span>
          <input
            autoFocus
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="What do you want to do?"
            onKeyDown={onKeyDown}
            role="combobox"
            aria-expanded={visible.length > 0}
            aria-controls="command-list"
            aria-activedescendant={active >= 0 ? `command-option-${active}` : undefined}
            aria-label="Search commands"
            autoComplete="off"
          />
        </div>
        <div className="command-list" id="command-list" role="listbox" ref={listRef}>
          {visible.map(([label], index) => (
            <button
              key={label}
              id={`command-option-${index}`}
              data-index={index}
              role="option"
              aria-selected={index === active}
              className={index === active ? 'active' : undefined}
              // Keep focus in the input so the combobox keeps handling keys.
              onMouseDown={(event) => event.preventDefault()}
              onMouseEnter={() => setSelected(index)}
              onClick={() => run(index)}
            >
              {label}
              {index === active && <kbd>↵</kbd>}
            </button>
          ))}
          {!visible.length && <p>No matching command.</p>}
        </div>
        <div className="command-foot">
          <span>↑↓ Navigate</span>
          <span>↵ Run</span>
          <span>Esc Close</span>
        </div>
      </section>
    </div>
  )
}
