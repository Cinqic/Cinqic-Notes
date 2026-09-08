const escapeHtml = (value: string) =>
  value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;')

const inline = (value: string) =>
  value
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/__([^_]+)__/g, '<strong>$1</strong>')
    .replace(/~~([^~]+)~~/g, '<del>$1</del>')
    .replace(/\*([^*]+)\*/g, '<em>$1</em>')
    .replace(/_([^_]+)_/g, '<em>$1</em>')
    // External links are shown, and are addressable by the app, but are never
    // a live href. A bare <a href="https://…"> inside the desktop WebView
    // navigates the application window to a remote page on click, which would
    // leave the local-first boundary entirely. The host decides what opening a
    // link means; see `externalHref` in the preview click handler.
    .replace(
      /\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g,
      '<span class="external-link" role="link" tabindex="0" data-external-href="$2" title="$2">$1</span>',
    )
    .replace(/\[([^\]]+)\]\((?!https?:\/\/)[^)]+\)/g, '<span class="wiki-link">$1</span>')
    .replace(/\[\[([^\]]+)\]\]/g, '<span class="wiki-link">$1</span>')

const tableCells = (value: string) => {
  const trimmed = value.trim().replace(/^\|/, '').replace(/\|$/, '')
  return trimmed.split('|').map((cell) => cell.trim())
}

const isTableDivider = (value: string) =>
  tableCells(value).length > 0 && tableCells(value).every((cell) => /^:?-{3,}:?$/.test(cell))

/**
 * A deliberately small, escaped preview renderer. Raw HTML and remote images
 * are never executed or fetched; source Markdown remains the editable truth.
 */
export const renderSafeMarkdown = (source: string, format: 'markdown' | 'text') => {
  if (format === 'text') return `<p>${escapeHtml(source).replaceAll('\n', '<br />')}</p>`

  const lines = source.split(/\r?\n/)
  const html: string[] = []
  let inCode = false
  let codeLines: string[] = []
  let listTag: 'ul' | 'ol' | null = null

  const closeList = () => {
    if (listTag) {
      html.push(`</${listTag}>`)
      listTag = null
    }
  }

  for (let index = 0; index < lines.length; index += 1) {
    const rawLine = lines[index]
    const escaped = escapeHtml(rawLine)
    if (escaped.trim().startsWith('```')) {
      if (inCode) {
        html.push(`<pre><code>${codeLines.join('\n')}</code></pre>`)
        codeLines = []
      }
      closeList()
      inCode = !inCode
      continue
    }
    if (inCode) {
      codeLines.push(escaped)
      continue
    }
    if (!escaped.trim()) {
      closeList()
      continue
    }
    if (lines[index + 1] && rawLine.includes('|') && isTableDivider(lines[index + 1])) {
      closeList()
      const headers = tableCells(escaped)
      html.push('<table><thead><tr>')
      headers.forEach((cell) => html.push(`<th>${inline(cell)}</th>`))
      html.push('</tr></thead><tbody>')
      index += 2
      while (index < lines.length && lines[index].includes('|')) {
        html.push('<tr>')
        tableCells(escapeHtml(lines[index])).forEach((cell) =>
          html.push(`<td>${inline(cell)}</td>`),
        )
        html.push('</tr>')
        index += 1
      }
      html.push('</tbody></table>')
      index -= 1
      continue
    }
    const heading = escaped.match(/^(#{1,6})\s+(.+)$/)
    if (heading) {
      closeList()
      const level = heading[1].length
      html.push(`<h${level}>${inline(heading[2])}</h${level}>`)
      continue
    }
    if (/^[-*+]\s+\[[ xX]\]\s+/.test(escaped)) {
      if (listTag !== 'ul') {
        closeList()
        html.push('<ul class="task-list">')
        listTag = 'ul'
      }
      const task = escaped.replace(/^[-*+]\s+\[([ xX])\]\s+/, '$1')
      html.push(
        `<li><span class="task-box">${task[0].toLowerCase() === 'x' ? '✓' : ''}</span>${inline(task.slice(1).trim())}</li>`,
      )
      continue
    }
    if (/^[-*+]\s+/.test(escaped)) {
      if (listTag !== 'ul') {
        closeList()
        html.push('<ul>')
        listTag = 'ul'
      }
      html.push(`<li>${inline(escaped.replace(/^[-*+]\s+/, ''))}</li>`)
      continue
    }
    if (/^\d+[.)]\s+/.test(escaped)) {
      if (listTag !== 'ol') {
        closeList()
        html.push('<ol>')
        listTag = 'ol'
      }
      html.push(`<li>${inline(escaped.replace(/^\d+[.)]\s+/, ''))}</li>`)
      continue
    }
    if (/^>\s?/.test(escaped)) {
      closeList()
      html.push(`<blockquote>${inline(escaped.replace(/^>\s?/, ''))}</blockquote>`)
      continue
    }
    if (/^---+$/.test(escaped.trim())) {
      closeList()
      html.push('<hr />')
      continue
    }
    if (/^!\[[^\]]*\]\(([^)]+)\)/.test(escaped)) {
      closeList()
      const alt = escaped.match(/^!\[([^\]]*)\]/)?.[1] ?? 'image'
      html.push(`<p class="blocked-image">Image attachment: ${alt}</p>`)
      continue
    }
    closeList()
    html.push(`<p>${inline(escaped)}</p>`)
  }
  closeList()
  if (inCode) html.push(`<pre><code>${codeLines.join('\n')}</code></pre>`)
  return html.join('') || '<p class="empty-preview">Nothing to preview yet.</p>'
}

/**
 * Convert the supported Markdown subset to readable plain text.
 *
 * The previous implementation removed every `*_`#>[]` character one at a time,
 * which corrupted ordinary prose — `snake_case`, `2 * 3`, and `C#` all lost
 * characters. This unwraps the constructs the renderer actually understands and
 * otherwise leaves the user's text exactly as written.
 */
export const toPlainText = (source: string) => {
  const lines = source.split(/\r?\n/)
  const output: string[] = []
  let inCode = false

  for (const line of lines) {
    if (line.trim().startsWith('```')) {
      inCode = !inCode
      continue
    }
    if (inCode) {
      output.push(line)
      continue
    }
    let text = line
    text = text.replace(/^(\s*)(#{1,6})\s+/, '$1')
    text = text.replace(/^(\s*)>\s?/, '$1')
    text = text.replace(/^(\s*)[-*+]\s+\[([ xX])\]\s+/, (_match, indent, mark) =>
      mark.toLowerCase() === 'x' ? `${indent}[done] ` : `${indent}[ ] `,
    )
    text = text.replace(/^(\s*)[-*+]\s+/, '$1• ')
    if (/^\s*-{3,}\s*$/.test(text)) {
      output.push('')
      continue
    }
    text = inlinePlainText(text)
    output.push(text)
  }
  return output.join('\n')
}

const inlinePlainText = (value: string) =>
  value
    // Images and links keep their visible text, not their target.
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/\[\[([^\]|]+)\|([^\]]+)\]\]/g, '$2')
    .replace(/\[\[([^\]]+)\]\]/g, '$1')
    // Emphasis markers are only removed where they actually wrap something,
    // so `snake_case` and `2 * 3` survive untouched.
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/__([^_]+)__/g, '$1')
    .replace(/~~([^~]+)~~/g, '$1')
    .replace(/(^|[\s(])\*([^*\s][^*]*?)\*(?=[\s).,;:!?]|$)/g, '$1$2')
    .replace(/(^|[\s(])_([^_\s][^_]*?)_(?=[\s).,;:!?]|$)/g, '$1$2')

/** Today's date as a local `YYYY-MM-DD` calendar date. */
export const localCalendarDate = (now: Date = new Date()) => {
  const year = String(now.getFullYear()).padStart(4, '0')
  const month = String(now.getMonth() + 1).padStart(2, '0')
  const day = String(now.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

export const formatRelativeDate = (iso: string) => {
  const time = Date.parse(iso)
  if (!Number.isFinite(time)) return ''
  const seconds = Math.max(0, (Date.now() - time) / 1000)
  if (seconds < 60) return 'just now'
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`
  if (seconds < 604800) return `${Math.floor(seconds / 86400)}d`
  return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(time)
}
