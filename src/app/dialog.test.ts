import { describe, expect, it } from 'vitest'
import { focusableWithin, nextDialogFocus } from './dialog'

const build = (html: string) => {
  const host = document.createElement('div')
  host.innerHTML = html
  document.body.append(host)
  return host
}

describe('focusableWithin', () => {
  it('collects focusable controls in document order', () => {
    const host = build(`
      <button id="a">a</button>
      <input id="b" />
      <a id="c" href="#x">c</a>
      <div id="d" tabindex="0"></div>
    `)
    expect(focusableWithin(host).map((element) => element.id)).toEqual(['a', 'b', 'c', 'd'])
  })

  it('skips disabled, hidden, and untabbable elements', () => {
    const host = build(`
      <button id="a">a</button>
      <button id="skip1" disabled>no</button>
      <input id="skip2" disabled />
      <div id="skip3" tabindex="-1"></div>
      <button id="skip4" hidden>no</button>
      <button id="skip5" aria-hidden="true">no</button>
      <button id="z">z</button>
    `)
    expect(focusableWithin(host).map((element) => element.id)).toEqual(['a', 'z'])
  })
})

describe('nextDialogFocus', () => {
  const items = ['first', 'middle', 'last'].map((id) => {
    const element = document.createElement('button')
    element.id = id
    return element
  })
  const [first, middle, last] = items

  it('wraps forward from the last element to the first', () => {
    expect(nextDialogFocus(items, last, false)).toBe(first)
  })

  it('wraps backward from the first element to the last', () => {
    expect(nextDialogFocus(items, first, true)).toBe(last)
  })

  it('lets the browser handle movement in the middle of the list', () => {
    expect(nextDialogFocus(items, middle, false)).toBeNull()
    expect(nextDialogFocus(items, middle, true)).toBeNull()
  })

  it('pulls focus back in when it is outside the dialog', () => {
    const stray = document.createElement('button')
    expect(nextDialogFocus(items, stray, false)).toBe(first)
    expect(nextDialogFocus(items, stray, true)).toBe(last)
    expect(nextDialogFocus(items, null, false)).toBe(first)
  })

  it('does nothing when the dialog holds nothing focusable', () => {
    expect(nextDialogFocus([], first, false)).toBeNull()
  })

  it('handles a dialog with a single focusable element', () => {
    expect(nextDialogFocus([first], first, false)).toBe(first)
    expect(nextDialogFocus([first], first, true)).toBe(first)
  })
})
