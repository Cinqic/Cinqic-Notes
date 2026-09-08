import { describe, expect, it, vi } from 'vitest'
import { AutosaveController, type SaveRequest, type SaveResult } from './autosave'

const hashOf = (content: string) => `h:${content}`

/** Let pending timers and microtasks run. */
const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

/**
 * A save backend whose completions are released one at a time, so a test can
 * hold a write "in flight" while further edits arrive.
 */
const deferredBackend = () => {
  const calls: Array<{
    request: SaveRequest
    settled: boolean
    resolve: () => void
    reject: (error: unknown) => void
  }> = []
  const save = (request: SaveRequest) =>
    new Promise<SaveResult>((resolve, reject) => {
      const call = {
        request,
        settled: false,
        resolve: () => {
          call.settled = true
          resolve({
            path: request.path,
            content: request.content,
            hash: hashOf(request.content),
          })
        },
        reject: (error: unknown) => {
          call.settled = true
          reject(error)
        },
      }
      calls.push(call)
    })
  const pending = () => calls.filter((call) => !call.settled)
  /** Resolve every outstanding write, including ones issued as a consequence. */
  const settleAll = async () => {
    for (let guard = 0; guard < 20; guard += 1) {
      const outstanding = pending()
      if (!outstanding.length) break
      outstanding.forEach((call) => call.resolve())
      await tick()
    }
  }
  return { calls, save, pending, settleAll }
}

const immediateBackend = () => {
  const requests: SaveRequest[] = []
  const save = async (request: SaveRequest): Promise<SaveResult> => {
    requests.push({ ...request })
    return { path: request.path, content: request.content, hash: hashOf(request.content) }
  }
  return { requests, save }
}

describe('AutosaveController', () => {
  it('does not let a late save completion overwrite a newer edit', async () => {
    const backend = deferredBackend()
    const controller = new AutosaveController({ save: backend.save, debounceMs: 0 })
    controller.activate('Note.md', 'A', hashOf('A'))

    controller.edit('B')
    await tick()
    expect(backend.calls).toHaveLength(1)
    expect(backend.calls[0].request.content).toBe('B')

    // The user keeps typing while the save for "B" is still in flight.
    controller.edit('BC')
    expect(controller.isDirty).toBe(true)

    backend.calls[0].resolve()
    await tick()

    // The stale completion must not have replaced "BC"; a follow-up write
    // carries the newest text instead.
    expect(backend.calls).toHaveLength(2)
    expect(backend.calls[1].request.content).toBe('BC')

    backend.calls[1].resolve()
    await controller.flush()

    expect(controller.isDirty).toBe(false)
    expect(controller.snapshot.savedContent).toBe('BC')
    expect(controller.snapshot.state).toBe('saved')
  })

  it('sends the hash established by the previous write on the follow-up save', async () => {
    const backend = deferredBackend()
    const controller = new AutosaveController({ save: backend.save, debounceMs: 0 })
    controller.activate('Note.md', 'A', hashOf('A'))

    controller.edit('B')
    await tick()
    controller.edit('BC')
    await backend.settleAll()
    await controller.flush()

    expect(backend.calls).toHaveLength(2)
    expect(backend.calls[0].request.expectedHash).toBe(hashOf('A'))
    expect(backend.calls[1].request.expectedHash).toBe(hashOf('B'))
  })

  it('never runs two saves for the same note concurrently', async () => {
    const backend = deferredBackend()
    const controller = new AutosaveController({ save: backend.save, debounceMs: 0 })
    controller.activate('Note.md', '', null)

    controller.edit('one')
    const first = controller.flush()
    await tick()
    expect(backend.calls).toHaveLength(1)

    controller.edit('two')
    controller.edit('three')
    // Still exactly one outstanding write while the first is unresolved.
    expect(backend.pending()).toHaveLength(1)

    backend.calls[0].resolve()
    await tick()

    expect(backend.calls).toHaveLength(2)
    // The two intermediate edits are coalesced into the newest text.
    expect(backend.calls[1].request.content).toBe('three')

    backend.calls[1].resolve()
    await first
    expect(controller.isDirty).toBe(false)
  })

  it('flush persists an edit that is still inside the debounce window', async () => {
    const backend = immediateBackend()
    const controller = new AutosaveController({ save: backend.save, debounceMs: 100_000 })
    controller.activate('Note.md', 'A', hashOf('A'))

    controller.edit('A edited')
    expect(backend.requests).toHaveLength(0)

    const clean = await controller.flush()

    expect(clean).toBe(true)
    expect(backend.requests).toEqual([
      { path: 'Note.md', content: 'A edited', expectedHash: hashOf('A') },
    ])
  })

  it('keeps the buffer dirty when a save fails so the text is not lost', async () => {
    const save = vi.fn().mockRejectedValue(new Error('disk full'))
    const errors: string[] = []
    const controller = new AutosaveController({
      save,
      debounceMs: 0,
      onError: (_error, state) => errors.push(state),
    })
    controller.activate('Note.md', 'A', hashOf('A'))
    controller.edit('A edited')

    const clean = await controller.flush()

    expect(clean).toBe(false)
    expect(controller.isDirty).toBe(true)
    expect(controller.snapshot.state).toBe('error')
    expect(errors).toEqual(['error'])
  })

  it('reports a conflict distinctly and retains the local edit', async () => {
    const save = vi.fn().mockRejectedValue(new Error('Conflict: file changed on disk'))
    const controller = new AutosaveController({ save, debounceMs: 0 })
    controller.activate('Note.md', 'A', hashOf('A'))
    controller.edit('A edited')

    await controller.flush()

    expect(controller.snapshot.state).toBe('conflict')
    expect(controller.isDirty).toBe(true)
  })

  it('does not touch a newly activated note when an earlier save resolves late', async () => {
    const backend = deferredBackend()
    const controller = new AutosaveController({ save: backend.save, debounceMs: 0 })
    controller.activate('First.md', 'first', hashOf('first'))

    controller.edit('first edited')
    const pending = controller.flush()
    await tick()
    expect(backend.calls).toHaveLength(1)

    // Navigate before the write completes.
    controller.activate('Second.md', 'second', hashOf('second'))
    backend.calls[0].resolve()
    await pending

    expect(backend.calls).toHaveLength(1)
    expect(controller.snapshot.path).toBe('Second.md')
    expect(controller.snapshot.savedContent).toBe('second')
    expect(controller.isDirty).toBe(false)
  })

  it('treats an edit back to the saved content as clean', () => {
    const backend = immediateBackend()
    const controller = new AutosaveController({ save: backend.save, debounceMs: 10 })
    controller.activate('Note.md', 'A', hashOf('A'))

    controller.edit('AB')
    expect(controller.snapshot.state).toBe('dirty')
    controller.edit('A')

    expect(controller.snapshot.state).toBe('saved')
    expect(controller.isDirty).toBe(false)
  })

  it('adopt() resets the buffer after out-of-band resolution', async () => {
    const backend = immediateBackend()
    const controller = new AutosaveController({ save: backend.save, debounceMs: 0 })
    controller.activate('Note.md', 'A', hashOf('A'))
    controller.edit('local edit')

    controller.adopt('Note.md', 'resolved', hashOf('resolved'))

    expect(controller.isDirty).toBe(false)
    expect(controller.snapshot.state).toBe('saved')
    await controller.flush()
    expect(backend.requests).toHaveLength(0)
  })

  it('reports dirty state for a note switch guard', async () => {
    const backend = immediateBackend()
    const controller = new AutosaveController({ save: backend.save, debounceMs: 100_000 })
    controller.activate('Note.md', 'A', hashOf('A'))

    expect(controller.isDirty).toBe(false)
    controller.edit('A edited')
    expect(controller.isDirty).toBe(true)

    await controller.flush()
    expect(controller.isDirty).toBe(false)
  })
})
