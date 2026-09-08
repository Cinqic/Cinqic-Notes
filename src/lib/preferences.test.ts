import { describe, expect, it } from 'vitest'
import { isTheme, readTheme, writeTheme } from './preferences'

const memoryStorage = (initial: Record<string, string> = {}): Storage => {
  const map = new Map(Object.entries(initial))
  return {
    get length() {
      return map.size
    },
    clear: () => map.clear(),
    getItem: (key: string) => map.get(key) ?? null,
    key: (index: number) => [...map.keys()][index] ?? null,
    removeItem: (key: string) => void map.delete(key),
    setItem: (key: string, value: string) => void map.set(key, value),
  } as Storage
}

const throwingStorage = (): Storage =>
  ({
    getItem: () => {
      throw new Error('blocked')
    },
    setItem: () => {
      throw new Error('blocked')
    },
  }) as unknown as Storage

describe('appearance preference', () => {
  it('round-trips each supported theme', () => {
    for (const theme of ['system', 'light', 'dark'] as const) {
      const storage = memoryStorage()
      writeTheme(theme, storage)
      expect(readTheme(storage)).toBe(theme)
    }
  })

  it('falls back to system when nothing is stored', () => {
    expect(readTheme(memoryStorage())).toBe('system')
  })

  it('falls back to system for an unrecognised stored value', () => {
    expect(readTheme(memoryStorage({ 'cinqic-notes.theme': 'solarized' }))).toBe('system')
    expect(readTheme(memoryStorage({ 'cinqic-notes.theme': '' }))).toBe('system')
  })

  it('survives storage that throws or is absent', () => {
    expect(readTheme(throwingStorage())).toBe('system')
    expect(() => writeTheme('dark', throwingStorage())).not.toThrow()
    expect(readTheme(undefined)).toBe('system')
    expect(() => writeTheme('dark', undefined)).not.toThrow()
  })

  it('recognises only the supported themes', () => {
    expect(isTheme('dark')).toBe(true)
    expect(isTheme('Dark')).toBe(false)
    expect(isTheme(null)).toBe(false)
    expect(isTheme(42)).toBe(false)
  })
})
