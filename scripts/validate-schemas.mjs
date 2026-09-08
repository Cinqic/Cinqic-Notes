/* global console, process */

/**
 * Validate the machine-facing Notes Core contracts.
 *
 * This is a development-time check only. Nothing here runs inside the
 * application, and it makes no network requests — Ajv compiles the schemas
 * locally and the JSON Schema 2020-12 meta-schema ships with Ajv itself.
 *
 * It checks three things the previous version did not:
 *
 *  1. each schema is itself a valid JSON Schema (compiled, not merely parsed);
 *  2. the concrete tool contract validates against its schema;
 *  3. the schemas actually reject the malformed data they claim to reject,
 *     so a passing run means something.
 */

import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import Ajv2020 from 'ajv/dist/2020.js'

const root = resolve(import.meta.dirname, '..')

const NOTE_SCHEMA = 'schemas/notes/notes-core.v1.schema.json'
const TOOLS_SCHEMA = 'schemas/tools/juniper-notes-tools.v1.schema.json'
const TOOLS_CONTRACT = 'schemas/tools/juniper-notes-tools.v1.json'

const readJson = async (relative) => JSON.parse(await readFile(resolve(root, relative), 'utf8'))

const failures = []
const record = (message) => failures.push(message)

/** A note that satisfies every constraint, used as the base for negative cases. */
const validNote = () => ({
  id: 'a1',
  path: 'Notes/Welcome.md',
  title: 'Welcome',
  format: 'markdown',
  contentHash: 'a'.repeat(64),
})

const noteCases = {
  valid: [
    ['a plain Markdown note', validNote()],
    ['a plain text note', { ...validNote(), path: 'note.txt', format: 'text' }],
    ['a nested path', { ...validNote(), path: 'Projects/2026/Plan.md' }],
    ['a Unicode path', { ...validNote(), path: 'Notes/Ünïcode ✓.md' }],
    ['extra properties, which are allowed', { ...validNote(), project: true }],
  ],
  invalid: [
    ['an absolute POSIX path', { ...validNote(), path: '/etc/passwd.md' }],
    ['a parent traversal', { ...validNote(), path: '../outside.md' }],
    ['a nested traversal', { ...validNote(), path: 'Notes/../../outside.md' }],
    ['a Windows backslash path', { ...validNote(), path: 'Notes\\Welcome.md' }],
    ['an unsupported extension', { ...validNote(), path: 'Notes/Welcome.exe' }],
    ['no extension', { ...validNote(), path: 'Notes/Welcome' }],
    ['an unknown format', { ...validNote(), format: 'richtext' }],
    ['a short content hash', { ...validNote(), contentHash: 'abc' }],
    ['an uppercase content hash', { ...validNote(), contentHash: 'A'.repeat(64) }],
    ['an empty id', { ...validNote(), id: '' }],
    ['a missing title', (({ title: _title, ...rest }) => rest)(validNote())],
    ['a missing path', (({ path: _path, ...rest }) => rest)(validNote())],
  ],
}

const toolCases = {
  valid: [['the shipped contract', null]],
  invalid: [
    ['an unknown schema version', { schemaVersion: 'notes-tools.v2', tools: ['notes.list'] }],
    ['an unknown tool name', { schemaVersion: 'notes-tools.v1', tools: ['notes.delete'] }],
    ['a namespace escape', { schemaVersion: 'notes-tools.v1', tools: ['other.list'] }],
    ['duplicate tools', { schemaVersion: 'notes-tools.v1', tools: ['notes.list', 'notes.list'] }],
    ['an empty tool list', { schemaVersion: 'notes-tools.v1', tools: [] }],
    ['a non-string tool', { schemaVersion: 'notes-tools.v1', tools: [42] }],
    ['an unexpected property', { schemaVersion: 'notes-tools.v1', tools: ['notes.list'], x: 1 }],
    ['a missing tool list', { schemaVersion: 'notes-tools.v1' }],
  ],
}

const main = async () => {
  const ajv = new Ajv2020({ allErrors: true, strict: true })

  const [noteSchema, toolsSchema, toolsContract] = await Promise.all([
    readJson(NOTE_SCHEMA),
    readJson(TOOLS_SCHEMA),
    readJson(TOOLS_CONTRACT),
  ])

  // 1. The schemas must compile as valid JSON Schema 2020-12.
  let validateNote
  let validateTools
  try {
    validateNote = ajv.compile(noteSchema)
  } catch (error) {
    record(`${NOTE_SCHEMA} is not a valid JSON Schema: ${error.message}`)
  }
  try {
    validateTools = ajv.compile(toolsSchema)
  } catch (error) {
    record(`${TOOLS_SCHEMA} is not a valid JSON Schema: ${error.message}`)
  }
  if (!validateNote || !validateTools) {
    console.error(failures.join('\n'))
    process.exit(1)
  }

  // 2. The shipped contract must satisfy its own schema.
  toolCases.valid[0][1] = toolsContract
  if (!validateTools(toolsContract)) {
    record(
      `${TOOLS_CONTRACT} does not satisfy ${TOOLS_SCHEMA}: ${ajv.errorsText(validateTools.errors)}`,
    )
  }

  // 3. Positive and negative cases, so the schemas are shown to discriminate.
  const check = (validate, label, cases) => {
    for (const [description, data] of cases.valid) {
      if (!validate(data)) {
        record(`${label}: expected to accept ${description} — ${ajv.errorsText(validate.errors)}`)
      }
    }
    for (const [description, data] of cases.invalid) {
      if (validate(data)) record(`${label}: expected to REJECT ${description}, but it was accepted`)
    }
  }
  check(validateNote, 'notes-core.v1', noteCases)
  check(validateTools, 'juniper-notes-tools.v1', toolCases)

  // 4. The contract stays in step with the tool names the schema permits.
  const permitted = toolsSchema.properties.tools.items.pattern
  const declared = toolsContract.tools
  const namesInPattern = permitted.match(/\(([^)]+)\)/)?.[1].split('|') ?? []
  const missing = namesInPattern.filter((name) => !declared.includes(`notes.${name}`))
  if (missing.length) {
    record(
      `${TOOLS_CONTRACT} omits tools the schema allows: ${missing.map((name) => `notes.${name}`).join(', ')}`,
    )
  }

  if (failures.length) {
    console.error(`Schema validation failed:\n${failures.map((line) => `  - ${line}`).join('\n')}`)
    process.exit(1)
  }

  const total =
    noteCases.valid.length +
    noteCases.invalid.length +
    toolCases.valid.length +
    toolCases.invalid.length
  console.log(
    `Validated 2 schemas and 1 contract against ${total} positive and negative cases ` +
      `(${declared.length} tools declared).`,
  )
}

await main()
