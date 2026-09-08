import { useEffect, useRef } from 'react'

/**
 * Elements that can hold keyboard focus. Anything explicitly removed from the
 * tab order, disabled, or hidden is excluded.
 */
const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

export const focusableWithin = (container: HTMLElement): HTMLElement[] =>
  [...container.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (element) => !element.hasAttribute('hidden') && element.getAttribute('aria-hidden') !== 'true',
  )

/**
 * Decide where Tab should move focus inside a modal.
 *
 * Returns the element that should receive focus, or `null` to let the browser
 * handle it normally. Keeping this a pure function means the wrap-around rules
 * are testable without rendering anything.
 */
export const nextDialogFocus = (
  focusable: HTMLElement[],
  active: Element | null,
  shiftKey: boolean,
): HTMLElement | null => {
  if (!focusable.length) return null
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  const index = focusable.indexOf(active as HTMLElement)

  // Focus escaped the dialog, or has not entered it yet.
  if (index === -1) return shiftKey ? last : first
  if (shiftKey && active === first) return last
  if (!shiftKey && active === last) return first
  return null
}

/**
 * Modal dialog behaviour: focus the dialog on open, keep Tab inside it, close
 * on Escape, and return focus to whatever was focused before it opened.
 */
export const useModalDialog = <T extends HTMLElement>(onClose: () => void) => {
  const ref = useRef<T>(null)
  const closeRef = useRef(onClose)
  closeRef.current = onClose

  useEffect(() => {
    const container = ref.current
    if (!container) return
    const previouslyFocused = document.activeElement as HTMLElement | null

    // Only move focus if the dialog has not already claimed it (an autoFocus
    // input inside it, for example).
    if (!container.contains(document.activeElement)) {
      const [initial] = focusableWithin(container)
      ;(initial ?? container).focus()
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        closeRef.current()
        return
      }
      if (event.key !== 'Tab') return
      const target = nextDialogFocus(
        focusableWithin(container),
        document.activeElement,
        event.shiftKey,
      )
      if (!target) return
      event.preventDefault()
      target.focus()
    }

    document.addEventListener('keydown', onKeyDown, true)
    return () => {
      document.removeEventListener('keydown', onKeyDown, true)
      // Restore focus so keyboard users are not dropped at the top of the page.
      if (previouslyFocused?.isConnected) previouslyFocused.focus()
    }
  }, [])

  return ref
}
