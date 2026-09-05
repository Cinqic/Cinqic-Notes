/* global console, process */

import { mkdir, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'

const destination = resolve(process.argv[2] ?? 'benchmarks/generated')
const sizes = [1000, 10000]

for (const size of sizes) {
  const directory = resolve(destination, `notes-${size}`)
  await mkdir(directory, { recursive: true })
  for (let index = 1; index <= size; index += 1) {
    const content = `# Benchmark note ${index}\n\nA local fixture for search and indexing. #benchmark\n\n- [ ] Task ${index}\n`
    await writeFile(resolve(directory, `Note-${String(index).padStart(5, '0')}.md`), content)
  }
  console.log(`Generated ${size.toLocaleString()} Markdown notes in ${directory}`)
}
