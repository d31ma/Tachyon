import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { once } from 'node:events'
import { createInterface } from 'node:readline'
import { chromium } from 'playwright'

const binary = process.platform === 'win32' ? 'target/debug/ty.exe' : 'target/debug/ty'
const fixture = 'crates/tachyon-cli/tests/fixtures/phase3-browser'
const build = spawnSync(binary, ['build', fixture, '--no-incremental'], {
  encoding: 'utf8',
  stdio: 'pipe',
})
assert.equal(build.status, 0, build.stderr)

const server = spawn(binary, ['dev', fixture, '--port', '0'], {
  stdio: ['ignore', 'pipe', 'pipe'],
})
let browser
try {
  const lines = createInterface({ input: server.stdout })
  const readinessTimeout = new Promise((_, reject) => {
    const timer = setTimeout(
      () => reject(new Error('Development server readiness timed out.')),
      15_000,
    )
    timer.unref()
  })
  const address = await Promise.race([
    (async () => {
      for await (const line of lines) {
        const match = line.match(/http:\/\/127\.0\.0\.1:\d+\//)
        if (match) return match[0]
      }
      throw new Error('Development server exited before reporting readiness.')
    })(),
    readinessTimeout,
  ])

  browser = await chromium.launch({ headless: true })
  const page = await browser.newPage()
  await page.goto(address, { waitUntil: 'load' })
  await page.waitForFunction(() => {
    const loaded = document.querySelector(
      'tachyon-island[data-tachyon-component="demo-load"]',
    )
    const failure = document.querySelector(
      'tachyon-island[data-tachyon-component="demo-failure"]',
    )
    return loaded?.dataset.activated === 'Loaded'
      && failure?.dataset.tachyonIslandError === 'activation_failed'
  })

  const state = await page.locator('tachyon-island').evaluateAll((roots) =>
    roots.map((root) => ({
      component: root.dataset.tachyonComponent,
      activated: root.dataset.activated || null,
      error: root.dataset.tachyonIslandError || null,
      module: root.dataset.tachyonModule || null,
      text: root.textContent.trim(),
    })),
  )
  assert.deepEqual(state, [
    {
      component: 'demo-load',
      activated: 'Loaded',
      error: null,
      module: '/.tachyon/components/demo-load.js',
      text: 'Loaded',
    },
    {
      component: 'demo-interactive',
      activated: null,
      error: null,
      module: '/.tachyon/components/demo-interactive.js',
      text: 'Interactive',
    },
    {
      component: 'demo-failure',
      activated: null,
      error: 'activation_failed',
      module: '/.tachyon/components/demo-failure.js',
      text: 'Preserved',
    },
    {
      component: 'demo-never',
      activated: null,
      error: null,
      module: null,
      text: 'Never',
    },
  ])

  await page.getByRole('button', { name: 'Interactive', exact: true }).click()
  await page.waitForFunction(() => {
    const root = document.querySelector(
      'tachyon-island[data-tachyon-component="demo-interactive"]',
    )
    return root?.dataset.activated === 'Interactive' && root?.dataset.replayed === '1'
  })
} finally {
  if (browser) await browser.close()
  if (server.exitCode === null) {
    const exited = once(server, 'exit')
    server.kill('SIGTERM')
    await exited
  }
}
