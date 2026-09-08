/* global console */

import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'

const SPDX = 'Apache-2.0'
const root = resolve(import.meta.dirname, '..')
const read = (relative) => readFile(resolve(root, relative), 'utf8')
const problems = []

const packageJson = JSON.parse(await read('package.json'))
if (packageJson.license !== SPDX) {
  problems.push(
    `package.json declares "license": ${JSON.stringify(packageJson.license)}; expected "${SPDX}".`,
  )
}

const cargo = await read('src-tauri/Cargo.toml')
const cargoLicense = cargo.match(/^license\s*=\s*"([^"]*)"/m)?.[1]
if (cargoLicense !== SPDX) {
  problems.push(
    `src-tauri/Cargo.toml declares license = ${JSON.stringify(cargoLicense ?? null)}; expected "${SPDX}".`,
  )
}

// The canonical legal text must stay recognizably Apache License 2.0. These markers are
// stable parts of the official text, so a truncated or substituted LICENSE fails here.
const license = await read('LICENSE')
const markers = [
  'Apache License',
  'Version 2.0, January 2004',
  'http://www.apache.org/licenses/',
  'TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION',
]
const missing = markers.filter((marker) => !license.includes(marker))
if (missing.length > 0) {
  problems.push(`LICENSE is not recognizably Apache License 2.0; missing: ${missing.join(', ')}.`)
}

// Documentation that states the project's own license. Third-party dependency licenses are
// deliberately not checked here, so only first-party claims about Cinqic Notes are matched.
const firstPartyDocs = ['README.md', 'NOTICE', 'THIRD_PARTY_NOTICES.md']
const staleClaim = /Cinqic Notes[^.]{0,120}?\b(MIT|BSD|GPL|MPL|ISC|Unlicense|proprietary)\b/i
for (const relative of firstPartyDocs) {
  const text = await read(relative)
  const match = text.match(staleClaim)
  if (match) {
    problems.push(
      `${relative} still claims Cinqic Notes is licensed as: "${match[0].trim()}"; expected Apache License 2.0.`,
    )
  }
}

if (problems.length > 0) {
  console.error('First-party license declarations are inconsistent:')
  for (const problem of problems) {
    console.error(`  - ${problem}`)
  }
  throw new Error(
    `${problems.length} first-party license ${problems.length === 1 ? 'declaration is' : 'declarations are'} inconsistent with ${SPDX}.`,
  )
}

console.log(`License ${SPDX} is consistent across first-party project metadata.`)
