import { afterEach, expect, test } from 'bun:test'
import { pathToFileURL } from 'node:url'
import path from 'node:path'
import { createTachyonApp } from './helpers.mjs'

const apps = []

afterEach(async () => {
  await Promise.all(apps.splice(0).map((app) => app.cleanup()))
})

test('tac.bundle prerenders component pages and supports global Tac output', async () => {
  const app = await createTachyonApp()
  apps.push(app)

  await app.writeFile('browser/components/status-card.html', '<script>let label = ""</script><strong>Status: {label}</strong>')
  await app.writeFile('browser/pages/index.html', [
    '<script>',
    "  let label = 'green'",
    '</script>',
    '<status-card :label="label" />',
    '<package-contract-widget data-source="blackbox"></package-contract-widget>',
  ].join('\n'))

  app.runBin('tac.bundle', [], { env: { TAC_FORMAT: 'global' } })

  const html = await app.readFile('dist/index.html')
  const pageModule = await app.readFile('dist/pages/index.js')
  const componentModule = await app.readFile('dist/components/status-card.js')

  expect(html).toContain('Status: green')
  expect(html).toContain('<package-contract-widget')
  expect(pageModule).toContain('register("/pages/index.js"')
  expect(componentModule).toContain('register("/components/status-card.js"')
  expect(pageModule).not.toContain('export default')
})

test('async Tac event handlers are awaited before rerender output is produced', async () => {
  const app = await createTachyonApp()
  apps.push(app)

  await app.writeFile('browser/pages/index.html', [
    '<script>',
    "let status = 'idle';",
    'async function requestMfa() {',
    "  status = 'loading';",
    '  await Promise.resolve();',
    "  status = 'phone-input';",
    '}',
    '</script>',
    '<button @click="requestMfa()">Continue</button>',
    '<p>{status}</p>',
  ].join('\n'))

  app.runBin('tac.bundle')

  const modulePath = path.join(app.root, 'dist', 'pages', 'index.js')
  const pageModule = await import(`${pathToFileURL(modulePath).href}?contract=${Date.now()}`)
  const render = await pageModule.default()
  const initial = await render()
  const buttonId = initial.match(/<button[^>]* id="([^"]+)"/)?.[1]

  expect(buttonId).toBeDefined()
  expect(initial).toContain('>idle</p>')

  const updated = await render(buttonId)

  expect(updated).toContain('>phone-input</p>')
  expect(updated).not.toContain('>loading</p>')
})
