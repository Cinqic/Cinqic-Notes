/* global console, process */

/**
 * Produce a complete transitive dependency licence inventory for both locked
 * dependency graphs.
 *
 * Offline and reproducible: the JavaScript side reads the installed pnpm store
 * for the locked graph, the Rust side reads `cargo metadata --locked`. Neither
 * consults the network. Run it before a public release and review the summary.
 *
 *   pnpm license:inventory            # write docs/licenses/inventory.md
 *   pnpm license:inventory --check    # fail if the committed file is stale
 */

import { execFile } from 'node:child_process'
import { readFile, mkdir, writeFile } from 'node:fs/promises'
import { resolve, dirname } from 'node:path'
import { promisify } from 'node:util'

const run = promisify(execFile)
const root = resolve(import.meta.dirname, '..')
const output = resolve(root, 'docs/licenses/inventory.md')

/** Single licences that impose no redistribution conditions worth flagging. */
const PERMISSIVE =
  /^(MIT|MIT-0|Apache-2\.0(\s+WITH\s+LLVM-exception)?|BSD-2-Clause|BSD-3-Clause|ISC|Zlib|0BSD|CC0-1\.0|CC-BY-4\.0|Unlicense|Unicode-3\.0|Unicode-DFS-2016|Python-2\.0|BSL-1\.0|WTFPL)$/i
/** Weak copyleft: usable, but attribution and source availability must be honoured. */
const WEAK_COPYLEFT = /^(MPL-2\.0|LGPL[^\s]*|CDDL[^\s]*|EPL[^\s]*)$/i

const worst = (left, right) => {
  const order = { ok: 0, note: 1, review: 2 }
  return order[left] >= order[right] ? left : right
}

/**
 * Classify an SPDX expression.
 *
 * `A OR B` is a choice, so it is only as restrictive as its most permissive
 * option — this is why crates like `r-efi`, licensed
 * `MIT OR Apache-2.0 OR LGPL-2.1-or-later`, are unremarkable. `A AND B`
 * requires both, so it is as restrictive as its worst part.
 */
/** Split on a top-level operator, ignoring anything inside parentheses. */
const splitTop = (license, operator) => {
  const parts = []
  let depth = 0
  let current = ''
  const tokens = license.split(/(\s+)/)
  for (const token of tokens) {
    if (depth === 0 && token.trim().toUpperCase() === operator) {
      parts.push(current.trim())
      current = ''
      continue
    }
    for (const character of token) {
      if (character === '(') depth += 1
      if (character === ')') depth -= 1
    }
    current += token
  }
  parts.push(current.trim())
  return parts.filter(Boolean)
}

const unwrap = (license) => {
  let value = license.trim()
  while (value.startsWith('(') && value.endsWith(')')) {
    // Only strip a pair that actually wraps the whole expression.
    let depth = 0
    let wraps = true
    for (let index = 0; index < value.length; index += 1) {
      if (value[index] === '(') depth += 1
      if (value[index] === ')') depth -= 1
      if (depth === 0 && index < value.length - 1) {
        wraps = false
        break
      }
    }
    if (!wraps) break
    value = value.slice(1, -1).trim()
  }
  return value
}

export const classify = (expression) => {
  const license = unwrap(String(expression).replace(/\//g, ' OR '))
  if (!license || /^UNKNOWN$|^FILE:/i.test(license)) return 'review'

  const orParts = splitTop(license, 'OR')
  if (orParts.length > 1) {
    const verdicts = orParts.map(classify)
    // A choice is only as restrictive as its most permissive option.
    return verdicts.includes('ok') ? 'ok' : verdicts.reduce(worst, 'ok')
  }
  const andParts = splitTop(license, 'AND')
  if (andParts.length > 1) return andParts.map(classify).reduce(worst, 'ok')

  if (PERMISSIVE.test(license)) return 'ok'
  if (WEAK_COPYLEFT.test(license)) return 'note'
  return 'review'
}

const jsInventory = async () => {
  const collect = async (extra) => {
    const { stdout } = await run('pnpm', ['licenses', 'list', '--json', ...extra], {
      cwd: root,
      maxBuffer: 64 * 1024 * 1024,
    })
    const parsed = JSON.parse(stdout)
    const rows = []
    for (const [license, packages] of Object.entries(parsed)) {
      for (const entry of packages) {
        const versions = entry.versions ?? [entry.version]
        for (const version of versions) rows.push({ name: entry.name, version, license })
      }
    }
    return rows
  }
  const all = await collect([])
  const runtime = await collect(['--prod'])
  const runtimeKeys = new Set(runtime.map((row) => `${row.name}@${row.version}`))
  return { all, runtime, runtimeKeys }
}

const rustInventory = async () => {
  const { stdout } = await run(
    'cargo',
    ['metadata', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', '--format-version', '1'],
    { cwd: root, maxBuffer: 256 * 1024 * 1024 },
  )
  const metadata = JSON.parse(stdout)
  const workspace = new Set(metadata.workspace_members)
  return metadata.packages
    .filter((entry) => !workspace.has(entry.id))
    .map((entry) => ({
      name: entry.name,
      version: entry.version,
      license: entry.license ?? (entry.license_file ? `FILE:${entry.license_file}` : 'UNKNOWN'),
    }))
    .sort((left, right) => left.name.localeCompare(right.name))
}

const tally = (rows) => {
  const counts = new Map()
  for (const row of rows) counts.set(row.license, (counts.get(row.license) ?? 0) + 1)
  return [...counts.entries()].sort(
    (left, right) => right[1] - left[1] || left[0].localeCompare(right[0]),
  )
}

const section = (title, rows) => {
  const lines = [
    `### ${title}`,
    '',
    `${rows.length} packages.`,
    '',
    '| License | Packages |',
    '| --- | --- |',
  ]
  for (const [license, count] of tally(rows)) lines.push(`| \`${license}\` | ${count} |`)
  return lines.join('\n')
}

const flagged = (rows, kind) => {
  const matches = rows.filter((row) => classify(row.license) === kind)
  if (!matches.length) return `_None._`
  return matches.map((row) => `- \`${row.name}@${row.version}\` — ${row.license}`).join('\n')
}

const main = async () => {
  const js = await jsInventory()
  const rust = await rustInventory()
  const everything = [...js.all, ...rust]

  const body = `# Dependency license inventory

<!-- Generated by \`pnpm license:inventory\`. Do not edit by hand. -->

Cinqic Notes itself is licensed under Apache-2.0 (see [LICENSE](../../LICENSE) and
[NOTICE](../../NOTICE)). This inventory covers the **dependencies**, whose licenses
remain with their own projects. Apache-2.0 relicenses nothing here.

Generated offline from the locked graphs: \`pnpm licenses list\` for JavaScript and
\`cargo metadata --locked\` for Rust.

## JavaScript

${section('Shipped in the application bundle (production dependencies)', js.runtime)}

${section('Full graph including build and development tooling', js.all)}

Only the production dependencies above are compiled into the distributed
frontend bundle. Development-only entries — including \`caniuse-lite\`
(CC-BY-4.0) and \`argparse\` (Python-2.0) — are build tooling and are not
redistributed.

## Rust

${section('Resolved crate graph', rust)}

The resolved graph covers every target and feature combination Cargo considers;
not every crate is linked into a given platform binary.

## Needs review before a public release

${flagged(everything, 'review')}

## Weak copyleft — attribution and source-availability obligations

${flagged(everything, 'note')}

These are unmodified upstream packages. MPL-2.0 is file-level copyleft and is
satisfied by shipping them unmodified and pointing users at their upstream
sources; it does not affect the license of Cinqic Notes' own code.

## System libraries

Linux packages link the platform WebKitGTK stack (LGPL) dynamically and do not
bundle it. \`rusqlite\` builds SQLite, which is in the public domain.
`

  if (process.argv.includes('--check')) {
    let existing = ''
    try {
      existing = await readFile(output, 'utf8')
    } catch {
      throw new Error('docs/licenses/inventory.md is missing. Run: pnpm license:inventory')
    }
    if (existing.trim() !== body.trim()) {
      throw new Error('docs/licenses/inventory.md is stale. Run: pnpm license:inventory')
    }
    console.log(`License inventory is current (${everything.length} packages).`)
    return
  }

  await mkdir(dirname(output), { recursive: true })
  await writeFile(output, body)
  const review = everything.filter((row) => classify(row.license) === 'review')
  console.log(
    `Wrote docs/licenses/inventory.md — ${js.all.length} JavaScript packages, ${rust.length} crates.`,
  )
  if (review.length) {
    console.log(`${review.length} package(s) need review before a public release.`)
    process.exitCode = 1
  }
}

// Only run when invoked as a script; the classifier is imported by tests.
if (process.argv[1] && import.meta.url === `file://${process.argv[1]}`) {
  await main()
}
