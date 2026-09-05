/* global console */

import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const packageJson = JSON.parse(await readFile(resolve(root, 'package.json'), 'utf8'))
const tauri = JSON.parse(await readFile(resolve(root, 'src-tauri/tauri.conf.json'), 'utf8'))
const cargo = await readFile(resolve(root, 'src-tauri/Cargo.toml'), 'utf8')
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
const versions = [packageJson.version, tauri.version, cargoVersion]
if (!versions.every((version) => version === packageJson.version)) {
  throw new Error(`Version mismatch: ${versions.join(', ')}`)
}
console.log(`Version ${packageJson.version} is consistent across package metadata.`)
