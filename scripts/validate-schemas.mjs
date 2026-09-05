/* global console */

import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const files = [
  'schemas/notes/notes-core.v1.schema.json',
  'schemas/tools/juniper-notes-tools.v1.schema.json',
  'schemas/tools/juniper-notes-tools.v1.json',
]
for (const file of files) {
  const parsed = JSON.parse(await readFile(resolve(root, file), 'utf8'))
  if (!parsed || typeof parsed !== 'object') throw new Error(`${file} is not an object`)
}
const tools = JSON.parse(await readFile(resolve(root, files[2]), 'utf8'))
if (tools.schemaVersion !== 'notes-tools.v1' || tools.tools.length !== 14)
  throw new Error('Notes tool contract is incomplete')
console.log(`Validated ${files.length} JSON schema/contract files.`)
