import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { dirname, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright'

assert.equal(process.platform, 'darwin', 'Phase 4 macOS evidence requires macOS.')

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const binary = join(repository, 'target/debug/ty')
const fixture = join(
  repository,
  'crates/tachyon-cli/tests/fixtures/phase4-macos',
)
const evidenceDirectory = join(repository, 'target/phase4-evidence')
const temporary = mkdtempSync(join(tmpdir(), 'tachyon-phase4-'))
const project = join(temporary, 'project')
const helper = join(temporary, 'phase4-macos-accessibility')
const visualHelper = join(temporary, 'phase4-visual-compare')
const nativeScreenshot = join(evidenceDirectory, 'native-macos.png')
const webScreenshot = join(evidenceDirectory, 'mobile-web.png')
const accessibilityPath = join(evidenceDirectory, 'accessibility.json')
const visualPath = join(evidenceDirectory, 'visual-comparison.json')
const reportPath = join(evidenceDirectory, 'report.json')
const bundleIdentifier = 'dev.tachyon.phase-four-evidence'
const processName = 'TachyonPhaseFour'
const lifecyclePath = join(
  process.env.HOME,
  'Library/Logs/Tachyon/dev.tachyon.phase-four-evidence.jsonl',
)

let browser
let server
let applicationStarted = false

function run(program, commandArguments, options = {}) {
  const result = spawnSync(program, commandArguments, {
    cwd: repository,
    encoding: 'utf8',
    stdio: 'pipe',
    ...options,
  })
  assert.equal(
    result.status,
    0,
    `${program} ${commandArguments.join(' ')} failed:\n${result.stderr || result.stdout}`,
  )
  return result.stdout
}

function stopApplication() {
  spawnSync('/usr/bin/osascript', [
    '-e',
    `tell application id "${bundleIdentifier}" to quit`,
  ])
}

const pause = (milliseconds) =>
  new Promise((resolvePause) => setTimeout(resolvePause, milliseconds))

async function retry(label, callback, attempts = 40) {
  let lastError
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      return callback()
    } catch (error) {
      lastError = error
      await pause(150)
    }
  }
  throw new Error(`${label} did not become ready: ${lastError}`)
}

function flattenNative(node, result = []) {
  if (node.accessibility?.role) {
    result.push({
      role: node.accessibility.role,
      name: node.accessibility.label || '',
    })
  }
  for (const child of node.children || []) flattenNative(child, result)
  return result
}

function hasSemantic(entries, role, name) {
  return entries.some((entry) => entry.role === role && entry.name === name)
}

function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex')
}

try {
  stopApplication()
  cpSync(fixture, project, {
    recursive: true,
    filter: (source) => relative(fixture, source).split(sep)[0] !== 'dist',
  })
  mkdirSync(evidenceDirectory, { recursive: true })
  rmSync(lifecyclePath, { force: true })

  run('/usr/bin/xcrun', [
    'swiftc',
    '-warnings-as-errors',
    '-O',
    '-framework',
    'AppKit',
    '-framework',
    'ApplicationServices',
    join(repository, 'scripts/phase4-macos-accessibility.swift'),
    '-o',
    helper,
  ])
  run('/usr/bin/xcrun', [
    'swiftc',
    '-warnings-as-errors',
    '-O',
    '-framework',
    'CoreGraphics',
    '-framework',
    'ImageIO',
    join(repository, 'scripts/phase4-visual-compare.swift'),
    '-o',
    visualHelper,
  ])

  const buildOutput = run(binary, ['build', project, '--target', 'macos'])
  const output = join(project, 'dist/macos')
  const app = join(output, 'TachyonPhaseFour.app')
  assert.ok(existsSync(join(app, 'Contents/MacOS/TachyonPhaseFour')))

  const nativeDocument = JSON.parse(
    readFileSync(join(output, 'native-ui/root.json'), 'utf8'),
  )
  const nativeSemantics = flattenNative(nativeDocument.root)
  for (const [role, name] of [
    ['main', 'Phase Four evidence'],
    ['heading', 'Native rendering'],
    ['button', 'Increase count'],
    ['status', 'Current count'],
    ['textbox', 'Customer name'],
    ['group', 'Implementation details'],
    ['group', 'Sales chart'],
  ]) {
    assert.ok(hasSemantic(nativeSemantics, role, name), `${role} "${name}"`)
  }

  run('/usr/bin/open', ['-n', app])
  applicationStarted = true
  await retry('native Accessibility window', () =>
    JSON.parse(run(helper, [processName])),
  )
  const accessibility = JSON.parse(
    run(helper, [processName, '--interact']),
  )
  writeFileSync(
    accessibilityPath,
    `${JSON.stringify(accessibility, null, 2)}\n`,
  )
  assert.deepEqual(accessibility.interactions, [
    'increment',
    'input',
    'disclosure',
  ])
  const elements = accessibility.elements
  assert.ok(
    elements.some(
      (element) =>
        element.role === 'AXButton' &&
        element.identifier === 'n_000006' &&
        element.label === 'Increase count',
    ),
  )
  assert.ok(
    elements.some(
      (element) =>
        element.identifier === 'n_000007' && element.value === '1',
    ),
  )
  assert.ok(
    elements.some(
      (element) =>
        element.role === 'AXTextField' &&
        element.identifier === 'n_000008' &&
        element.label === 'Customer name' &&
        element.value === 'Ada',
    ),
  )
  assert.ok(
    elements.some(
      (element) =>
        element.role === 'AXDisclosureTriangle' &&
        element.label === 'Implementation details' &&
        element.value === 'true',
    ),
  )
  assert.ok(
    elements.some(
      (element) => element.value === 'Disclosure content is native.',
    ),
  )
  assert.ok(
    elements.some(
      (element) =>
        element.role === 'AXGroup' && element.label === 'Sales chart',
    ),
  )

  const bounds = run('/usr/bin/osascript', [
    '-e',
    `tell application "System Events" to tell process "${processName}"`,
    '-e',
    'set frontmost to true',
    '-e',
    'set p to position of window 1',
    '-e',
    'set s to size of window 1',
    '-e',
    'return (item 1 of p as text) & "," & (item 2 of p as text) & "," & (item 1 of s as text) & "," & (item 2 of s as text)',
    '-e',
    'end tell',
  ]).trim()
  const boundsMatch = bounds.match(
    /^(-?\d+),(-?\d+),(\d+),(\d+)$/,
  )
  assert.ok(boundsMatch, `unexpected native window bounds: ${bounds}`)
  const nativeWidth = Number(boundsMatch[3])
  const nativeHeight = Number(boundsMatch[4])
  assert.equal(nativeWidth, 420)
  assert.ok(
    nativeHeight >= 600 && nativeHeight <= 780,
    `native window height ${nativeHeight} is outside the usable parity range`,
  )
  run('/usr/sbin/screencapture', [
    '-x',
    `-R${bounds}`,
    nativeScreenshot,
  ])

  const webRoot = join(output, 'web')
  server = createServer((request, response) => {
    const pathname = decodeURIComponent(
      new URL(request.url || '/', 'http://127.0.0.1').pathname,
    )
    const requested = resolve(webRoot, pathname === '/' ? 'index.html' : `.${pathname}`)
    if (requested !== webRoot && !requested.startsWith(`${webRoot}${sep}`)) {
      response.writeHead(403).end()
      return
    }
    try {
      const bytes = readFileSync(requested)
      const type = requested.endsWith('.js')
        ? 'text/javascript; charset=utf-8'
        : 'text/html; charset=utf-8'
      response.writeHead(200, { 'content-type': type }).end(bytes)
    } catch {
      response.writeHead(404).end()
    }
  })
  await new Promise((resolveListen, rejectListen) => {
    server.once('error', rejectListen)
    server.listen(0, '127.0.0.1', resolveListen)
  })
  const address = server.address()
  assert.equal(typeof address, 'object')
  const url = `http://127.0.0.1:${address.port}/`

  browser = await chromium.launch({ headless: true })
  const page = await browser.newPage({
    viewport: { width: nativeWidth, height: nativeHeight },
  })
  await page.goto(url, { waitUntil: 'load' })
  await page.waitForFunction(
    () => document.documentElement.dataset.tachyonController === 'active',
  )
  await page.getByRole('button', { name: 'Increase count' }).click()
  await page.getByRole('textbox', { name: 'Customer name' }).fill('Ada')
  await page.locator('summary').click()
  await assert.doesNotReject(() =>
    page.getByRole('status', { name: 'Current count' }).waitFor(),
  )
  assert.equal(
    await page.getByRole('status', { name: 'Current count' }).textContent(),
    '1',
  )
  assert.equal(
    await page.getByRole('textbox', { name: 'Customer name' }).inputValue(),
    'Ada',
  )
  assert.equal(await page.locator('details').getAttribute('open'), '')
  assert.equal(
    await page.getByText('Disclosure content is native.').isVisible(),
    true,
  )

  const webSemantics = await page.evaluate(() => [
    {
      role: 'main',
      name: document.querySelector('main')?.getAttribute('aria-label') || '',
    },
    {
      role: 'heading',
      name: document.querySelector('h1')?.textContent?.trim() || '',
    },
    {
      role: 'button',
      name: document.querySelector('button')?.getAttribute('aria-label') || '',
    },
    {
      role: 'status',
      name: document.querySelector('output')?.getAttribute('aria-label') || '',
    },
    {
      role: 'textbox',
      name: document.querySelector('input')?.getAttribute('aria-label') || '',
    },
    {
      role: 'group',
      name: document.querySelector('details')?.getAttribute('aria-label') || '',
    },
    {
      role: 'group',
      name:
        document.querySelector('fancy-chart')?.getAttribute('aria-label') || '',
    },
  ])
  assert.deepEqual(webSemantics, [
    { role: 'main', name: 'Phase Four evidence' },
    { role: 'heading', name: 'Native rendering' },
    { role: 'button', name: 'Increase count' },
    { role: 'status', name: 'Current count' },
    { role: 'textbox', name: 'Customer name' },
    { role: 'group', name: 'Implementation details' },
    { role: 'group', name: 'Sales chart' },
  ])
  await page.screenshot({ path: webScreenshot })

  const visualOutput = run(visualHelper, [nativeScreenshot, webScreenshot])
  writeFileSync(visualPath, visualOutput)
  const visual = JSON.parse(visualOutput)
  assert.equal(visual.passed, true)

  stopApplication()
  applicationStarted = false
  await retry('native process termination', () => {
    const result = spawnSync('/usr/bin/pgrep', ['-x', processName])
    assert.notEqual(result.status, 0)
    return true
  })
  const lifecycle = readFileSync(lifecyclePath, 'utf8')
    .trim()
    .split('\n')
    .map((line) => JSON.parse(line))
  const lifecycleEvents = lifecycle.map((entry) => entry.event)
  for (const event of [
    'controller.created',
    'controller.mounted',
    'controller.active',
    'state.increment',
    'state.input',
    'state.disclosure',
    'controller.destroyed',
  ]) {
    assert.ok(lifecycleEvents.includes(event), `missing lifecycle event ${event}`)
  }
  assert.ok(
    lifecycleEvents.indexOf('controller.created') <
      lifecycleEvents.indexOf('controller.mounted'),
  )
  assert.ok(
    lifecycleEvents.indexOf('controller.mounted') <
      lifecycleEvents.indexOf('controller.active'),
  )
  assert.ok(
    lifecycleEvents.indexOf('controller.active') <
      lifecycleEvents.indexOf('controller.destroyed'),
  )

  const report = {
    contractVersion: 1,
    target: 'macos',
    build: buildOutput.trim(),
    nativeSemantics,
    webSemantics,
    lifecycleEvents,
    interactions: accessibility.interactions,
    visual,
    artifacts: {
      accessibility: {
        path: 'accessibility.json',
        sha256: sha256(accessibilityPath),
      },
      nativeScreenshot: {
        path: 'native-macos.png',
        sha256: sha256(nativeScreenshot),
      },
      webScreenshot: {
        path: 'mobile-web.png',
        sha256: sha256(webScreenshot),
      },
      visualComparison: {
        path: 'visual-comparison.json',
        sha256: sha256(visualPath),
      },
    },
  }
  writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`)
  process.stdout.write(
    `Phase 4 macOS acceptance passed; evidence: ${reportPath}\n`,
  )
} finally {
  if (browser) await browser.close()
  if (server) {
    await new Promise((resolveClose) => server.close(resolveClose))
  }
  if (applicationStarted) stopApplication()
  rmSync(temporary, { force: true, recursive: true })
}
