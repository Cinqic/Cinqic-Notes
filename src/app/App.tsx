import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { open, save } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { notesApi } from '../lib/api'
import { formatRelativeDate, renderSafeMarkdown } from '../lib/markdown'
import type {
  BacklinkItem,
  ConflictInfo,
  GraphData,
  LibraryInfo,
  NoteDocument,
  NoteSummary,
  RevisionItem,
  SaveState,
  TaskItem,
} from '../types'

type View = 'all' | 'today' | 'projects' | 'tasks' | 'graph' | 'trash' | 'settings'
type Theme = 'system' | 'light' | 'dark'

const isDesktop = () => '__TAURI_INTERNALS__' in window

const displayError = (error: unknown) => {
  if (typeof error === 'string') return error
  if (error && typeof error === 'object' && 'message' in error) return String(error.message)
  return 'Something went wrong. Your original file was not intentionally discarded.'
}

export default function App() {
  const [library, setLibrary] = useState<LibraryInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [bootError, setBootError] = useState('')
  const [theme, setTheme] = useState<Theme>('system')

  useEffect(() => {
    document.documentElement.dataset.theme = theme
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
  const [revisions, setRevisions] = useState<RevisionItem[]>([])
  const [tasks, setTasks] = useState<TaskItem[]>([])
  const [graph, setGraph] = useState<GraphData | null>(null)
  const [conflict, setConflict] = useState<ConflictInfo | null>(null)
  const editorRef = useRef<HTMLTextAreaElement>(null)
  const activePathRef = useRef<string | null>(null)
  activePathRef.current = activeDocument?.path ?? null

  const loadNotes = useCallback(
    async (nextQuery = query, includeTrashed = view === 'trash') => {
      const result = nextQuery.trim()
        ? await notesApi.searchNotes(nextQuery.trim(), includeTrashed)
        : await notesApi.listNotes(includeTrashed)
      setNotes(result)
      return result
    },
    [query, view],
  )

  const selectNote = useCallback(async (path: string) => {
    try {
      const doc = await notesApi.getNote(path)
      setActiveDocument(doc)
      setContent(doc.content)
      setTitleDraft(doc.title)
      setSaveState('saved')
      setError('')
      setConflict(null)
      setBacklinks(await notesApi.getBacklinks(path))
      setRevisions(await notesApi.listRevisions(path))
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }, [])

  const refresh = useCallback(
    async (preferredPath?: string) => {
      try {
        const nextNotes = await loadNotes('', view === 'trash')
        const path = preferredPath ?? activeDocument?.path ?? nextNotes[0]?.path
        if (path && nextNotes.some((note) => note.path === path)) await selectNote(path)
        else if (!nextNotes.length) {
          setActiveDocument(null)
          setContent('')
          setBacklinks([])
          setRevisions([])
        }
      } catch (nextError: unknown) {
        setError(displayError(nextError))
      }
    },
    [activeDocument?.path, loadNotes, selectNote, view],
  )

  useEffect(() => {
    void refresh()
  }, [refresh])

  useEffect(() => {
    if (!isDesktop()) return
    let unlisten: (() => void) | undefined
    void listen('library-changed', () => {
      void loadNotes(query, view === 'trash')
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
    const path = activeDocument.path
    const expectedHash = activeDocument.hash
    const contentToSave = content
    const timer = window.setTimeout(() => {
      setSaveState('saving')
      notesApi
        .updateNote(path, contentToSave, expectedHash)
        .then((doc) => {
          const stillEditing = activePathRef.current === path
          if (stillEditing) {
            setActiveDocument(doc)
            setNotes((current) => current.map((note) => (note.path === path ? doc : note)))
            if (content === contentToSave) {
              setContent(doc.content)
              setSaveState('saved')
              setMessage('Saved locally')
              setTimeout(() => setMessage(''), 1800)
            } else {
              setActiveDocument(doc)
              setSaveState('dirty')
            }
          }
        })
        .catch((nextError: unknown) => {
          setSaveState('conflict')
          setError(displayError(nextError))
          void notesApi
            .getConflict(path)
            .then(setConflict)
            .catch(() => undefined)
        })
    }, 450)
    return () => window.clearTimeout(timer)
  }, [activeDocument, content, saveState])

  const createNote = async () => {
    try {
      const doc = await notesApi.createNote('Untitled', 'markdown')
      setView('all')
      setActiveDocument(doc)
      setContent(doc.content)
      setTitleDraft(doc.title)
      setSaveState('saved')
      setNotes((current) => [doc, ...current.filter((note) => note.path !== doc.path)])
      setBacklinks([])
      setRevisions([])
      setMessage('New note')
      window.setTimeout(() => editorRef.current?.focus(), 50)
    } catch (nextError: unknown) {
      setError(displayError(nextError))
    }
  }

  const forceSave = async (): Promise<boolean> => {
    if (!activeDocument || saveState === 'saving') return false
    try {
      setSaveState('saving')
      const doc = await notesApi.updateNote(activeDocument.path, content, activeDocument.hash)
      setActiveDocument(doc)
      setContent(doc.content)
      setSaveState('saved')
      setMessage('Saved locally')
      window.setTimeout(() => setMessage(''), 1800)
      return true
    } catch (nextError: unknown) {
      setSaveState('conflict')
      setError(displayError(nextError))
      return false
    }
  }

  const renameNote = async () => {
    if (!activeDocument || !titleDraft.trim() || titleDraft.trim() === activeDocument.title) return
    try {
      const doc = await notesApi.renameNote(activeDocument.path, titleDraft.trim())
      setActiveDocument(doc)
      setContent(doc.content)
      setTitleDraft(doc.title)
      setNotes((current) => current.map((note) => (note.path === activeDocument.path ? doc : note)))
      setBacklinks(await notesApi.getBacklinks(doc.path))
      setRevisions(await notesApi.listRevisions(doc.path))
    } catch (nextError: unknown) {
      setTitleDraft(activeDocument.title)
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
        setActiveDocument(doc)
        setContent(doc.content)
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

  const selectView = (nextView: View) => {
    setView(nextView)
    setQuery('')
    setError('')
    if (nextView === 'settings') setShowSettings(true)
    if (
      nextView === 'all' ||
      nextView === 'today' ||
      nextView === 'projects' ||
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
      } else if (event.key.toLowerCase() === 'f' && document.activeElement !== editorRef.current) {
        event.preventDefault()
        document.querySelector<HTMLInputElement>('.search-input')?.focus()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  })

  const filteredNotes = useMemo(() => {
    if (view === 'today') {
      const today = new Date().toDateString()
      return notes.filter((note) => new Date(note.modifiedAt).toDateString() === today)
    }
    if (view === 'projects') return notes.filter((note) => note.project)
    return notes
  }, [notes, view])

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
            onRestore={async (path) => {
              try {
                const doc = await notesApi.restoreNote(path)
                setView('all')
                setActiveDocument(doc)
                setContent(doc.content)
                setTitleDraft(doc.title)
                setSaveState('saved')
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
                activePath={activeDocument?.path}
                query={query}
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
              content={content}
              setContent={(next) => {
                setContent(next)
                setSaveState('dirty')
              }}
              titleDraft={titleDraft}
              setTitleDraft={setTitleDraft}
              onTitleBlur={() => void renameNote()}
              showPreview={showPreview}
              setShowPreview={setShowPreview}
              onToggleList={() => setShowList(!showList)}
              onSave={() => void forceSave()}
              onTrash={() => void moveToTrash()}
              backlinks={backlinks}
              error={error}
              message={message}
              editorRef={editorRef}
              onResolveConflict={(resolution) => {
                if (activeDocument)
                  void notesApi
                    .resolveConflict(activeDocument.path, resolution)
                    .then((doc) => {
                      setActiveDocument(doc)
                      setContent(doc.content)
                      setConflict(null)
                      setSaveState('saved')
                    })
                    .catch((nextError: unknown) => setError(displayError(nextError)))
              }}
              conflict={conflict}
              revisions={revisions}
              onRestoreRevision={async (revisionId) => {
                if (!activeDocument) return
                try {
                  const doc = await notesApi.restoreRevision(activeDocument.path, revisionId)
                  setActiveDocument(doc)
                  setContent(doc.content)
                  setRevisions(await notesApi.listRevisions(doc.path))
                  setSaveState('saved')
                  setMessage('Revision restored')
                } catch (nextError: unknown) {
                  setError(displayError(nextError))
                }
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
  activePath,
  query,
  setQuery,
  onSelect,
  onImport,
  view,
}: {
  notes: NoteSummary[]
  activePath?: string
  query: string
  setQuery: (value: string) => void
  onSelect: (path: string) => void
  onImport: () => void
  view: View
}) {
  return (
    <section className="note-list-panel" aria-label="Notes">
      <div className="list-heading">
        <div>
          <span className="eyebrow">{view === 'trash' ? 'RECOVERABLE' : 'YOUR LIBRARY'}</span>
          <h2>{view === 'trash' ? 'Trash' : view === 'projects' ? 'Projects' : 'Notes'}</h2>
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
        <kbd>⌘ K</kbd>
      </label>
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
                : view === 'trash'
                  ? 'Trash is empty.'
                  : 'No notes yet.'}
            </p>
            <small>
              {query
                ? 'Try a different word or phrase.'
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
}: {
  notes: NoteSummary[]
  onRestore: (path: string) => Promise<void>
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
  backlinks,
  error,
  message,
  editorRef,
  conflict,
  onResolveConflict,
  revisions,
  onRestoreRevision,
}: {
  document: NoteDocument | null
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
  backlinks: BacklinkItem[]
  error: string
  message: string
  editorRef: React.RefObject<HTMLTextAreaElement | null>
  conflict: ConflictInfo | null
  onResolveConflict: (resolution: 'local' | 'disk') => void
  revisions: RevisionItem[]
  onRestoreRevision: (revisionId: string) => Promise<void>
}) {
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
        <button
          className="icon-button"
          aria-label="More note actions"
          onClick={() => setShowMeta(!showMeta)}
        >
          •••
        </button>
        {showMeta && (
          <div className="more-menu">
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
        {showHistory && <RevisionPanel revisions={revisions} onRestore={onRestoreRevision} />}
        {showPreview ? (
          <article
            className="markdown-preview"
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
              <div key={`${link.sourcePath}-${link.label}`} className="backlink-item">
                <span>↗</span>
                <span>
                  <strong>{link.sourceTitle}</strong>
                  <small>{link.label}</small>
                </span>
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
}: {
  revisions: RevisionItem[]
  onRestore: (revisionId: string) => Promise<void>
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
            <button className="button secondary small" onClick={() => void onRestore(revision.id)}>
              Restore
            </button>
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
  return (
    <section className="utility-view">
      <div className="utility-heading">
        <span className="eyebrow">ORDINARY MARKDOWN</span>
        <h1>Tasks</h1>
        <p>Checklists stay in the note that owns them.</p>
      </div>
      <div className="task-filters">
        <span className="filter-chip active">All {tasks.length}</span>
        <span className="filter-chip">Open {tasks.filter((task) => !task.checked).length}</span>
        <span className="filter-chip">Complete {tasks.filter((task) => task.checked).length}</span>
      </div>
      <div className="task-table">
        {tasks.map((task) => (
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
        {!tasks.length && (
          <div className="empty-utility">
            <span>☑</span>
            <p>No checklist items yet.</p>
            <small>Use `- [ ]` in any Markdown note.</small>
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
  onRebuild,
}: {
  theme: Theme
  setTheme: (value: Theme) => void
  library: LibraryInfo
  onRebuild: () => Promise<void>
}) {
  const [working, setWorking] = useState(false)
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
              onClick={() => {
                setWorking(true)
                void onRebuild().finally(() => setWorking(false))
              }}
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
    const value =
      kind === 'markdown'
        ? content
        : content
            .split('')
            .filter((character) => !'*_`#>[]'.includes(character))
            .join('')
    await navigator.clipboard?.writeText(value)
    setCopied(kind === 'markdown' ? 'Markdown copied' : 'Plain text copied')
    window.setTimeout(() => setCopied(''), 1800)
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
  const visible = actions.filter(([label]) => label.toLowerCase().includes(filter.toLowerCase()))
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
          <span>⌘</span>
          <input
            autoFocus
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="What do you want to do?"
            onKeyDown={(event) => {
              if (event.key === 'Escape') onClose()
              if (event.key === 'Enter' && visible[0]) {
                void visible[0][1]()
                onClose()
              }
            }}
          />
        </div>
        <div className="command-list">
          {visible.map(([label, action]) => (
            <button
              key={label}
              onClick={() => {
                void action()
                onClose()
              }}
            >
              {label}
              <kbd>↵</kbd>
            </button>
          ))}
          {!visible.length && <p>No matching command.</p>}
        </div>
        <div className="command-foot">
          <span>↑↓ Navigate</span>
          <span>↵ Run</span>
          <span>esc Close</span>
        </div>
      </section>
    </div>
  )
}
