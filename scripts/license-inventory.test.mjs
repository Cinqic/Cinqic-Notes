import { describe, expect, it } from 'vitest'
import { classify } from './license-inventory.mjs'

describe('SPDX classification', () => {
  it('accepts plain permissive licenses', () => {
    for (const license of ['MIT', 'Apache-2.0', 'ISC', 'BSD-3-Clause', 'Unicode-3.0', 'CC0-1.0']) {
      expect(classify(license), license).toBe('ok')
    }
  })

  it('treats a disjunction as its most permissive option', () => {
    expect(classify('MIT OR Apache-2.0')).toBe('ok')
    expect(classify('MIT/Apache-2.0')).toBe('ok')
    // r-efi: an LGPL option is irrelevant when MIT is also offered.
    expect(classify('MIT OR Apache-2.0 OR LGPL-2.1-or-later')).toBe('ok')
    expect(classify('Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT')).toBe('ok')
  })

  it('treats a conjunction as its most restrictive part', () => {
    // unicode-ident
    expect(classify('(MIT OR Apache-2.0) AND Unicode-3.0')).toBe('ok')
    expect(classify('MIT AND GPL-3.0')).toBe('review')
  })

  it('flags copyleft that is genuinely required', () => {
    expect(classify('MPL-2.0')).toBe('note')
    expect(classify('LGPL-2.1-or-later')).toBe('note')
    expect(classify('GPL-3.0-only')).toBe('review')
    expect(classify('AGPL-3.0')).toBe('review')
    expect(classify('SSPL-1.0')).toBe('review')
  })

  it('flags anything it cannot identify', () => {
    expect(classify('UNKNOWN')).toBe('review')
    expect(classify('')).toBe('review')
    expect(classify('FILE:LICENSE.txt')).toBe('review')
    expect(classify('Weird-Custom-1.0')).toBe('review')
  })
})
