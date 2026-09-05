import { describe, expect, it } from 'vitest'
import { formatRelativeDate, renderSafeMarkdown } from './markdown'

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
