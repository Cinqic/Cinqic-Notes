import type { Theme } from '../types'

const THEME_KEY = 'cinqic-notes.theme'
const THEMES: Theme[] = ['system', 'light', 'dark']

export const isTheme = (value: unknown): value is Theme =>
  typeof value === 'string' && (THEMES as string[]).includes(value)

/**
 * Read the stored appearance preference.
 *
 * Falls back to `system` for anything unrecognised, and for environments where
 * storage is unavailable or throws (private windows, blocked site data). Only
 * the preference is stored — never note contents.
 */
export const readTheme = (storage: Storage | undefined = safeStorage()): Theme => {
  try {
    const stored = storage?.getItem(THEME_KEY)
    return isTheme(stored) ? stored : 'system'
  } catch {
    return 'system'
  }
}

export const writeTheme = (theme: Theme, storage: Storage | undefined = safeStorage()) => {
  try {
    storage?.setItem(THEME_KEY, theme)
  } catch {
    // A preference that cannot be remembered is not worth failing over.
  }
}

const safeStorage = (): Storage | undefined => {
  try {
    return typeof localStorage === 'undefined' ? undefined : localStorage
  } catch {
    return undefined
  }
}
