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
    .replace(/\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g, '<a href="$2" rel="noreferrer">$1</a>')
    .replace(/\[\[([^\]]+)\]\]/g, '<span class="wiki-link">$1</span>')

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
  let inList = false

  const closeList = () => {
    if (inList) {
      html.push('</ul>')
      inList = false
    }
  }

  for (const rawLine of lines) {
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
    const heading = escaped.match(/^(#{1,6})\s+(.+)$/)
    if (heading) {
      closeList()
      const level = heading[1].length
      html.push(`<h${level}>${inline(heading[2])}</h${level}>`)
      continue
    }
    if (/^[-*+]\s+\[[ xX]\]\s+/.test(escaped)) {
      if (!inList) {
        html.push('<ul class="task-list">')
        inList = true
      }
      const task = escaped.replace(/^[-*+]\s+\[([ xX])\]\s+/, '$1')
      html.push(
        `<li><span class="task-box">${task[0].toLowerCase() === 'x' ? '✓' : ''}</span>${inline(task.slice(1).trim())}</li>`,
      )
      continue
    }
    if (/^[-*+]\s+/.test(escaped)) {
      if (!inList) {
        html.push('<ul>')
        inList = true
      }
      html.push(`<li>${inline(escaped.replace(/^[-*+]\s+/, ''))}</li>`)
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
