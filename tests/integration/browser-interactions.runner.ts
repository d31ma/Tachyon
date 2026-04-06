import { chromium, type Browser, type Page } from 'playwright'

const BASE = 'http://localhost:8080'
const TIMEOUT = 8000
const STARTUP_DELAY_MS = 1000

let browser: Browser
let page: Page

const results: { test: string; status: 'PASS' | 'FAIL'; ms: number; detail?: string }[] = []

async function run(name: string, fn: () => Promise<void>) {
    const t0 = performance.now()
    try {
        await fn()
        results.push({ test: name, status: 'PASS', ms: Math.round(performance.now() - t0) })
        console.log(`  ✓ ${name}`)
    } catch (e: any) {
        results.push({ test: name, status: 'FAIL', ms: Math.round(performance.now() - t0), detail: e.message })
        console.log(`  ✗ ${name} — ${e.message}`)
    }
}

async function startServer() {
    new Worker('./tests/integration/server-worker.ts').postMessage({
        script: './src/cli/serve.ts',
        cwd: './examples'
    })
    await Bun.sleep(STARTUP_DELAY_MS)
}

async function main() {
    console.log('\n🔍 Tachyon Browser Smoke Tests\n')

    await startServer()

    browser = await chromium.launch({ headless: true })
    page = await (await browser.newContext()).newPage()

    await run('Page loads and renders the main UI', async () => {
        const resp = await page.goto(BASE, { waitUntil: 'domcontentloaded' })
        if (!resp || !resp.ok()) throw new Error(`Page load failed: ${resp?.status()}`)
        await page.waitForSelector('.hero', { timeout: TIMEOUT })
        await page.waitForSelector('.sidebar-brand', { timeout: TIMEOUT })
    })

    await run('Component content renders in the browser', async () => {
        await page.waitForSelector('.clicker', { timeout: TIMEOUT })
        const clickers = await page.$$('.clicker')
        if (clickers.length < 1) throw new Error('Expected at least one rendered clicker')
    })

    await run('SPA navigation still works', async () => {
        const apiLink = await page.$('a[href="/api"]')
        if (!apiLink) throw new Error('API link not found')
        await apiLink.click()
        await page.waitForFunction(() => window.location.pathname === '/api', undefined, { timeout: TIMEOUT })

        await page.goto(BASE, { waitUntil: 'domcontentloaded' })
        await page.waitForSelector('.hero', { timeout: TIMEOUT })
    })

    await browser.close()

    const failed = results.filter(r => r.status === 'FAIL')
    console.log(`\n${results.length - failed.length} passed, ${failed.length} failed\n`)

    if (failed.length > 0) {
        for (const test of failed) {
            console.log(`  ✗ ${test.test}: ${test.detail}`)
        }
    }

    process.exit(failed.length > 0 ? 1 : 0)
}

main().catch(async e => {
    console.error('Fatal error:', e)
    if (browser) await browser.close().catch(() => {})
    process.exit(1)
})
