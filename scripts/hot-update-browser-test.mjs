#!/usr/bin/env node

import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import {
  cpSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { createInterface } from 'node:readline'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright'

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const binary = process.env.TAC_BIN
  ?? path.join(repository, process.platform === 'win32' ? 'target/debug/ty.exe' : 'target/debug/ty')
const fixture = path.join(repository, 'crates/tachyon-cli/tests/fixtures/hot-update-browser')
const project = mkdtempSync(path.join(tmpdir(), 'tachyon-hot-update-'))
cpSync(fixture, project, { recursive: true })

const component = path.join(project, 'client/components/hot-counter')
const templatePath = path.join(component, 'tac.html')
const modulePath = path.join(component, 'tac.js')
const stylePath = path.join(component, 'tac.css')
const originalTemplate = readFileSync(templatePath, 'utf8')

const server = spawn(binary, ['dev', project, '--port', '0'], {
  cwd: repository,
  stdio: ['ignore', 'pipe', 'pipe'],
})
let browser

try {
  const lines = createInterface({ input: server.stdout })
  const address = await Promise.race([
    (async () => {
      for await (const line of lines) {
        const match = line.match(/http:\/\/127\.0\.0\.1:\d+\//)
        if (match) return match[0]
      }
      throw new Error('Development server exited before reporting readiness.')
    })(),
    new Promise((_, reject) => {
      const timer = setTimeout(
        () => reject(new Error('Development server readiness timed out.')),
        20_000,
      )
      timer.unref()
    }),
  ])

  browser = await chromium.launch({ headless: true })
  const page = await browser.newPage()
  const errors = []
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })
  page.on('pageerror', (error) => errors.push(String(error)))

  await page.goto(address, { waitUntil: 'load' })
  await page.locator('tachyon-component[data-tachyon-active="true"]').waitFor()
  await page.getByRole('button', { name: 'Increase', exact: true }).click()
  await page.waitForFunction(() => document.querySelector('#count')?.textContent === '2')
  await page.getByText('More', { exact: true }).click()
  await page.locator('#draft').fill('developer draft')

  writeFileSync(stylePath, 'button { color: rgb(29, 78, 216); }\n')
  await page.waitForFunction(() =>
    document.documentElement.dataset.tachyonHot === 'css'
      && getComputedStyle(document.querySelector('#increment')).color === 'rgb(29, 78, 216)',
  )
  assert.equal(await page.textContent('#count'), '2', 'CSS HMR must retain component state')

  const nextModule = readFileSync(modulePath, 'utf8').replace(
    "version = 'one'",
    "version = 'two'",
  )
  writeFileSync(modulePath, nextModule)
  await page.waitForFunction(() => {
    const root = document.querySelector('tachyon-component')
    return document.documentElement.dataset.tachyonHot === 'island'
      && root?.dataset.moduleVersion === 'two'
      && document.querySelector('#module-version')?.textContent === 'two'
  })
  assert.equal(await page.textContent('#count'), '2', 'component HMR must restore declared hot state')
  assert.equal(await page.locator('#details').evaluate((node) => node.open), true)
  assert.equal(await page.locator('#draft').inputValue(), 'developer draft')
  assert.equal(await page.evaluate(() => document.activeElement?.id), 'draft')
  assert.equal(await page.evaluate(() => sessionStorage.getItem('tachyon-hot-disposed')), 'true')
  assert.equal(await page.evaluate(() => sessionStorage.getItem('tachyon-hot-aborted')), 'true')

  writeFileSync(
    templatePath,
    '<section aria-label="Hot counter"><logic :else>broken</logic></section>\n',
  )
  await page.locator('#tachyon-hot-diagnostics').waitFor()
  assert.equal(await page.textContent('#count'), '2', 'failed builds must retain last-good DOM')
  assert.match(await page.textContent('#tachyon-hot-diagnostics'), /TY1302/)

  writeFileSync(templatePath, originalTemplate)
  await page.waitForFunction(() =>
    !document.querySelector('#tachyon-hot-diagnostics')
      && document.querySelector('#count')?.textContent === '1',
  )
  assert.deepEqual(errors, [], `browser errors: ${errors.join(' | ')}`)
  console.log('PASS: semantic hot-update browser gate')
} finally {
  if (browser) await browser.close()
  if (server.exitCode === null) {
    const exited = once(server, 'exit')
    server.kill('SIGTERM')
    await exited
  }
  rmSync(project, { recursive: true, force: true })
}
