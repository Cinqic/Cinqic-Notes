import { describe, expect, it } from 'vitest'
import { formatRelativeDate, localCalendarDate, renderSafeMarkdown, toPlainText } from './markdown'

describe('safe Markdown preview', () => {
  it('renders common constructs while escaping raw HTML', () => {
    const html = renderSafeMarkdown(
      '# Hello\n\n**bold** and [[Project]]\n\n<script>alert(1)</script>',
      'markdown',
    )
    expect(html).toContain('<h1>Hello</h1>')
    expect(html).toContain('<strong>bold</strong>')
    expect(html).toContain('class="wiki-link"')
    expect(html).not.toContain('<script>')
    expect(html).toContain('&lt;script&gt;')
  })

  it('does not fetch or render remote images', () => {
    const html = renderSafeMarkdown('![private](https://example.com/private.png)', 'markdown')
    expect(html).toContain('Image attachment: private')
    expect(html).not.toContain('https://example.com')
  })

  it('keeps plain text readable', () => {
    expect(renderSafeMarkdown('one\ntwo', 'text')).toBe('<p>one<br />two</p>')
  })

  it('renders ordered lists, tables, and local links without making them executable', () => {
    const html = renderSafeMarkdown(
      '1. First\n2. Second\n\n| Name | State |\n| --- | --- |\n| Cinqic | local |\n\n[Note](Notes/Note.md)',
      'markdown',
    )
    expect(html).toContain('<ol><li>First</li><li>Second</li></ol>')
    expect(html).toContain('<table><thead><tr><th>Name</th><th>State</th></tr>')
    expect(html).toContain('<span class="wiki-link">Note</span>')
    expect(html).not.toContain('href=')
  })
})

describe('relative dates', () => {
  it('returns an empty label for malformed timestamps', () => {
    expect(formatRelativeDate('not-a-date')).toBe('')
  })
})

describe('external links', () => {
  it('never produces a live href that could navigate the WebView', () => {
    const html = renderSafeMarkdown('See [Cinqic](https://cinqic.com/page).', 'markdown')
    expect(html).not.toContain('<a ')
    // No live href attribute; the data- attribute is inert until the host acts.
    expect(html).not.toMatch(/\shref=/)
    expect(html).toContain('data-external-href="https://cinqic.com/page"')
    expect(html).toContain('>Cinqic<')
  })

  it('does not treat a javascript: URL as a link at all', () => {
    const html = renderSafeMarkdown('[x](javascript:alert(1))', 'markdown')
    expect(html).not.toMatch(/\shref=/)
    expect(html).not.toContain('data-external-href')
    expect(html).toContain('class="wiki-link"')
  })

  it('escapes quotes inside a link target so the attribute cannot be broken out of', () => {
    const html = renderSafeMarkdown('[x](https://e.com/"onmouseover="alert(1))', 'markdown')
    expect(html).not.toContain('onmouseover="alert')
    expect(html).toContain('&quot;')
  })
})

describe('plain text conversion', () => {
  it('preserves prose that merely contains Markdown punctuation', () => {
    const source = [
      'The rate is 2 * 3 and the flag is snake_case.',
      'Issue #42 tracks the C# port.',
      'A path like a/b_c/d_e stays intact.',
    ].join('\n')
    expect(toPlainText(source)).toBe(source)
  })

  it('unwraps the constructs the renderer understands', () => {
    expect(toPlainText('# Title')).toBe('Title')
    expect(toPlainText('**bold** and *italic*')).toBe('bold and italic')
    expect(toPlainText('`code`')).toBe('code')
    expect(toPlainText('~~gone~~')).toBe('gone')
    expect(toPlainText('> quoted')).toBe('quoted')
    expect(toPlainText('- item')).toBe('• item')
  })

  it('keeps link text and drops only the target', () => {
    expect(toPlainText('See [the page](https://example.com).')).toBe('See the page.')
    expect(toPlainText('See [[Project]].')).toBe('See Project.')
    expect(toPlainText('See [[Project|the plan]].')).toBe('See the plan.')
    expect(toPlainText('![diagram](a.png)')).toBe('diagram')
  })

  it('marks checklist state readably', () => {
    expect(toPlainText('- [x] done\n- [ ] todo')).toBe('[done] done\n[ ] todo')
  })

  it('leaves fenced code contents alone', () => {
    expect(toPlainText('```\nconst a = b * c\n```')).toBe('const a = b * c')
  })

  it('does not lose characters from an empty document', () => {
    expect(toPlainText('')).toBe('')
  })
})

describe('local calendar date', () => {
  it('uses the local calendar day, not the UTC one', () => {
    // 23:30 on 8 September in a UTC+2 zone is still 8 September locally, while
    // toISOString() would report the 8th only by luck and the 9th elsewhere.
    const late = new Date(2026, 8, 8, 23, 30, 0)
    expect(localCalendarDate(late)).toBe('2026-09-08')
  })

  it('formats single-digit months and days with padding', () => {
    expect(localCalendarDate(new Date(2026, 0, 5, 12, 0, 0))).toBe('2026-01-05')
  })

  it('agrees with the local date at the start and end of a day', () => {
    expect(localCalendarDate(new Date(2026, 11, 31, 0, 0, 0))).toBe('2026-12-31')
    expect(localCalendarDate(new Date(2026, 11, 31, 23, 59, 59))).toBe('2026-12-31')
  })
})
