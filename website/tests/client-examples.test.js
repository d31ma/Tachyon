// @ts-nocheck -- example text is validated by the selected real compiler.
import { expect, test } from 'bun:test'
import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const repository = fileURLToPath(new URL('../../', import.meta.url))
const dataRoot = new URL('../client/shared/data/', import.meta.url)

test('published browser companion examples compile and use the documented runtime contract', async () => {
  const docs = JSON.parse(await readFile(new URL('docs.json', dataRoot), 'utf8'))
  const catalogue = JSON.parse(await readFile(new URL('features.json', dataRoot), 'utf8'))
  const examples = []
  for (const topic of ['companions', 'storage', 'devices']) {
    for (const section of docs.topics[topic].sections) {
      if (section.code?.trim().startsWith('export default class'))
        examples.push({ name: `${topic}: ${section.heading}`, code: section.code })
    }
  }
  const recipe = docs.topics.cookbook.sections.find((section) => section.heading === 'Signal-driven UI').code
  for (const source of recipe.split(/(?=\/\/ (?:Form|List) companion:)/).filter(Boolean))
    examples.push({ name: 'signal recipe', code: source })
  for (const feature of catalogue.features.filter(({ id }) => ['companions', 'persisted', 'fetch-cache'].includes(id))) {
    for (const file of feature.files.filter(({ name }) => name.endsWith('/tac.js')))
      examples.push({ name: feature.id, code: file.code })
  }
  expect(examples.length).toBeGreaterThan(10)
  const project = await mkdtemp(join(tmpdir(), 'tachyon-published-examples-'))
  try {
    for (const [index, example] of examples.entries()) {
      const page = join(project, 'client/pages', `example-${index}`)
      await mkdir(page, { recursive: true })
      await writeFile(join(page, 'tac.html'), '<main>Example</main>')
      await writeFile(join(page, 'tac.js'), example.code)
    }
    const executable = process.platform === 'win32' ? 'ty.exe' : 'ty'
    const release = join(repository, 'target/release', executable)
    const debug = join(repository, 'target/debug', executable)
    const binary = process.env.TAC_BIN ?? (existsSync(release) ? release : existsSync(debug) ? debug : null)
    const command = binary ? [binary, 'build', project]
      : ['cargo', 'run', '--locked', '--manifest-path', join(repository, 'Cargo.toml'), '--', 'build', project]
    const built = spawnSync(command[0], command.slice(1), {
      encoding: 'utf8', timeout: 120000,
      env: { ...process.env, RUSTUP_TOOLCHAIN: '1.97.1' },
    })
    expect(built.status, built.stderr || built.stdout).toBe(0)
    for (const [index, example] of examples.entries()) {
      const module = await import(pathToFileURL(join(project, 'dist', `example-${index}`, 'client.js')).href)
      expect(typeof module.default, example.name).toBe('function')
      if (example.name === 'storage: Cached reads') {
        const owner = new module.default()
        const calls = []
        owner.tac = { fetch: async (...args) => {
          calls.push(args)
          return new Response(JSON.stringify([{ name: 'public product' }]))
        } }
        await owner.load()
        expect(calls[0]).toEqual(['/api/products', { credentials: 'omit' }, { cache: 'cache-first' }])
        expect(owner.products).toEqual([{ name: 'public product' }])
      }
    }
  } finally { await rm(project, { recursive: true, force: true }) }
}, 120000)
