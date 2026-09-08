import type { SaveState } from '../types'

export interface SaveRequest {
  path: string
  content: string
  expectedHash: string | null
}

export interface SaveResult {
  path: string
  content: string
  hash: string
}

export type SaveFn = (request: SaveRequest) => Promise<SaveResult>

export interface AutosaveSnapshot {
  state: SaveState
  path: string | null
  /** Content the controller last confirmed as written to disk. */
  savedContent: string
}

export interface AutosaveOptions {
  save: SaveFn
  debounceMs?: number
  onChange?: (snapshot: AutosaveSnapshot) => void
  onSaved?: (result: SaveResult) => void
  onError?: (error: unknown, state: Extract<SaveState, 'conflict' | 'error'>) => void
}

const isConflict = (error: unknown) =>
  typeof error === 'string'
    ? error.toLowerCase().includes('conflict')
    : error instanceof Error && error.message.toLowerCase().includes('conflict')

/**
 * Serialized, generation-aware autosave for a single active note buffer.
 *
 * The invariants this exists to guarantee:
 *
 * 1. A save that completes late never replaces newer in-memory content. The
 *    controller only ever records what it *sent*; it never writes a response
 *    body back over the live buffer.
 * 2. At most one save per note is in flight. Newer edits are coalesced into a
 *    follow-up pass of the same loop, so `expectedHash` stays ordered.
 * 3. Pending debounced work is flushed — never silently cancelled — when the
 *    caller navigates away, switches Library, or closes the window.
 */
export class AutosaveController {
  private readonly save: SaveFn
  private readonly debounceMs: number
  private readonly onChange?: (snapshot: AutosaveSnapshot) => void
  private readonly onSaved?: (result: SaveResult) => void
  private readonly onError?: (
    error: unknown,
    state: Extract<SaveState, 'conflict' | 'error'>,
  ) => void

  private timer: ReturnType<typeof setTimeout> | null = null
  private path: string | null = null
  /** Newest content the user has typed. */
  private content = ''
  /** Content most recently confirmed written for `path`. */
  private savedContent = ''
  /** Hash the backend last confirmed for `path`. */
  private savedHash: string | null = null
  private loop: Promise<void> | null = null
  private state: SaveState = 'saved'

  constructor(options: AutosaveOptions) {
    this.save = options.save
    this.debounceMs = options.debounceMs ?? 450
    this.onChange = options.onChange
    this.onSaved = options.onSaved
    this.onError = options.onError
  }

  get snapshot(): AutosaveSnapshot {
    return { state: this.state, path: this.path, savedContent: this.savedContent }
  }

  private setState(next: SaveState) {
    if (this.state === next) return
    this.state = next
    this.onChange?.(this.snapshot)
  }

  private clearTimer() {
    if (this.timer === null) return
    clearTimeout(this.timer)
    this.timer = null
  }

  /**
   * Point the controller at a note whose content is already known to match
   * disk. Any pending work for a previous note must be flushed first by the
   * caller; `activate` deliberately does not discard unsaved content silently.
   */
  activate(path: string, content: string, hash: string | null) {
    this.clearTimer()
    this.path = path
    this.content = content
    this.savedContent = content
    this.savedHash = hash
    this.setState('saved')
    this.onChange?.(this.snapshot)
  }

  /** Drop the buffer entirely (no active note). */
  deactivate() {
    this.clearTimer()
    this.path = null
    this.content = ''
    this.savedContent = ''
    this.savedHash = null
    this.setState('saved')
  }

  /** Record a user edit and schedule a debounced save. */
  edit(content: string) {
    if (!this.path) return
    this.content = content
    if (content === this.savedContent && this.state !== 'conflict') {
      this.clearTimer()
      this.setState('saved')
      return
    }
    this.setState('dirty')
    this.clearTimer()
    this.timer = setTimeout(() => {
      this.timer = null
      void this.run()
    }, this.debounceMs)
  }

  get isDirty() {
    return this.path !== null && this.content !== this.savedContent
  }

  /**
   * Force any pending edit to be written now and wait for it. Returns true when
   * the buffer is clean afterwards. Callers must await this before switching
   * notes, changing Library, or closing the window.
   */
  async flush(): Promise<boolean> {
    this.clearTimer()
    await this.run()
    return !this.isDirty && this.state !== 'conflict' && this.state !== 'error'
  }

  /**
   * Load content into the buffer as an unsaved edit *without* scheduling a
   * save. Used when the user is shown recovered work to review: it must not
   * overwrite the note on disk until they decide to keep it.
   */
  hold(content: string) {
    if (!this.path) return
    this.clearTimer()
    this.content = content
    this.setState(content === this.savedContent ? 'saved' : 'dirty')
    this.onChange?.(this.snapshot)
  }

  /** Adopt content resolved out-of-band (conflict resolution, revision restore). */
  adopt(path: string, content: string, hash: string) {
    if (this.path !== path) return
    this.content = content
    this.savedContent = content
    this.savedHash = hash
    this.setState('saved')
    this.onChange?.(this.snapshot)
  }

  private async run(): Promise<void> {
    if (this.loop) {
      await this.loop
      return
    }
    this.loop = this.drain().finally(() => {
      this.loop = null
    })
    await this.loop
  }

  private async drain(): Promise<void> {
    while (this.path !== null && this.content !== this.savedContent) {
      const path = this.path
      const attempt = this.content
      const expectedHash = this.savedHash
      this.setState('saving')
      let result: SaveResult
      try {
        result = await this.save({ path, content: attempt, expectedHash })
      } catch (error) {
        // The buffer stays dirty on purpose: the user's text is still the only
        // copy of their newest edit, and a retry or conflict resolution needs it.
        const next = isConflict(error) ? 'conflict' : 'error'
        this.setState(next)
        this.onError?.(error, next)
        return
      }
      if (this.path !== path) {
        // Navigated away mid-flight. The write itself succeeded; nothing about
        // the new buffer may be touched.
        return
      }
      // Record only what we sent. Never assign `result.content` over the live
      // buffer — that is precisely how a late completion eats a newer edit.
      this.savedContent = attempt
      this.savedHash = result.hash
      this.onSaved?.(result)
      if (this.content === attempt) {
        this.setState('saved')
        return
      }
      // A newer edit landed while this save was in flight. Loop again so the
      // follow-up write uses the hash this write just established.
      this.setState('dirty')
    }
  }
}
