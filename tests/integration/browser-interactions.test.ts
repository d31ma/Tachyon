import { chromium, type Page, type Browser } from 'playwright'

const BASE = 'http://localhost:8080'
const TIMEOUT = 8000

let browser: Browser
let page: Page

const results: { test: string; status: 'PASS' | 'FAIL'; ms: number; detail?: string }[] = []

async function run(name: string, fn: () => Promise<void>) {
    const t0 = performance.now()
    try {
        await fn()
        const ms = Math.round(performance.now() - t0)
        results.push({ test: name, status: 'PASS', ms })
        console.log(`  ✓ ${name} (${ms}ms)`)
    } catch (e: any) {
        const ms = Math.round(performance.now() - t0)
        results.push({ test: name, status: 'FAIL', ms, detail: e.message })
        console.log(`  ✗ ${name} (${ms}ms) — ${e.message}`)
    }
}

async function main() {
    console.log('\n🔍 Tachyon Frontend Browser Tests\n')

    browser = await chromium.launch({ headless: true })
    const context = await browser.newContext()
    page = await context.newPage()

    // ── Page Load ──────────────────────────────────────────────────────────
    // Capture all console messages
    const consoleLogs: string[] = []
    page.on('console', msg => {
        const text = msg.text()
        if (text.startsWith('[tachyon]')) consoleLogs.push(text)
    })
    page.on('pageerror', e => consoleLogs.push(`PAGE_ERROR: ${e.message}`))

    await run('Page loads and renders content', async () => {
        const resp = await page.goto(BASE, { waitUntil: 'domcontentloaded' })
        if (!resp || !resp.ok()) throw new Error(`Page load failed: ${resp?.status()}`)
        // Wait for render to complete (body gets populated by SPA renderer)
        await page.waitForFunction(() => document.body.innerHTML.length > 100, undefined, { timeout: TIMEOUT })
    })

    // ── Layout renders ─────────────────────────────────────────────────────
    await run('Layout renders with sidebar and topbar', async () => {
        // The layout has sidebar-brand and topbar
        await page.waitForSelector('.sidebar-brand', { timeout: TIMEOUT })
        await page.waitForSelector('.topbar', { timeout: TIMEOUT })
    })

    // ── Slot content renders inside layout ─────────────────────────────────
    await run('Page content renders inside layout slot', async () => {
        await page.waitForSelector('#ty-layout-slot', { timeout: TIMEOUT })
        const slotContent = await page.$eval('#ty-layout-slot', el => el.innerHTML.length)
        if (slotContent < 50) throw new Error(`Slot content too small: ${slotContent} chars`)
    })

    // ── Hero section renders ───────────────────────────────────────────────
    await run('Hero section renders with greeting', async () => {
        await page.waitForSelector('.hero', { timeout: TIMEOUT })
        const heroText = await page.$eval('.hero', el => el.textContent ?? '')
        if (!heroText.includes('Welcome to Tachyon')) throw new Error(`Hero text missing: ${heroText.slice(0, 100)}`)
    })

    // ── Stats section renders ──────────────────────────────────────────────
    await run('Stats section renders with numbers', async () => {
        await page.waitForSelector('.stats', { timeout: TIMEOUT })
        const statCards = await page.$$('.stat-card')
        if (statCards.length !== 3) throw new Error(`Expected 3 stat cards, got ${statCards.length}`)
    })

    // ── Clicker component renders ──────────────────────────────────────────
    await run('Clicker components render', async () => {
        await page.waitForSelector('.clicker', { timeout: TIMEOUT })
        const clickers = await page.$$('.clicker')
        if (clickers.length < 2) throw new Error(`Expected 2+ clickers, got ${clickers.length}`)
    })

    // ── Click interaction: "About" toggle ──────────────────────────────────
    await run('About toggle shows alert on first click', async () => {
        // Find the About button in the hero
        const aboutBtn = await page.$('.hero-actions sl-button')
        if (!aboutBtn) throw new Error('About button not found')
        await aboutBtn.click()
        // Wait for the alert to appear
        await page.waitForSelector('sl-alert[variant="primary"]', { timeout: TIMEOUT })
    })

    await run('About toggle hides alert on second click', async () => {
        const aboutBtn = await page.$('.hero-actions sl-button')
        if (!aboutBtn) throw new Error('About button not found')
        await aboutBtn.click()
        // Wait for the alert to disappear
        await page.waitForFunction(
            () => !document.querySelector('sl-alert[variant="primary"]'),
            undefined,
            { timeout: TIMEOUT }
        )
    })

    // ── Click interaction: Clicker increment ───────────────────────────────
    await run('Clicker increment button updates count', async () => {
        // Get initial value
        const firstClicker = (await page.$$('.clicker'))[0]
        if (!firstClicker) throw new Error('No clicker found')
        const initialValue = await firstClicker.$eval('.clicker-value', el => el.textContent?.trim())
        
        // Click the increment button (second button in clicker-actions)
        const incBtn = await firstClicker.$('.clicker-actions sl-button[variant="primary"]')
        if (!incBtn) throw new Error('Increment button not found')
        
        // Collect console errors for debugging
        const errors: string[] = []
        page.on('pageerror', e => errors.push(e.message))
        
        await incBtn.click()
        await page.waitForTimeout(1000)
        
        // Check if there were errors
        if (errors.length > 0) throw new Error(`Console errors: ${errors.join('; ')}`)
        
        // Check if value changed
        const newValue = await firstClicker.$eval('.clicker-value', el => el.textContent?.trim())
        const expected = String(Number(initialValue) + 1)
        if (newValue !== expected) {
            // Debug: check if @click attribute exists
            const hasClick = await page.evaluate(() => {
                const btns = document.querySelectorAll('.clicker-actions sl-button[variant="primary"]')
                return Array.from(btns).map(b => ({
                    id: b.id,
                    click: b.getAttribute('@click'),
                    attrs: Array.from(b.attributes).map(a => `${a.name}=${a.value}`)
                }))
            })
            throw new Error(`Expected ${expected}, got ${newValue}. Button info: ${JSON.stringify(hasClick)}. Logs: ${consoleLogs.join(' | ')}`)
        }
    })

    // ── Click interaction: Clicker reset ───────────────────────────────────
    await run('Clicker reset button resets count to 0', async () => {
        const firstClicker = (await page.$$('.clicker'))[0]
        if (!firstClicker) throw new Error('No clicker found')
        
        const resetBtn = await firstClicker.$('.clicker-actions sl-button[variant="danger"]')
        if (!resetBtn) throw new Error('Reset button not found')
        await resetBtn.click()
        await page.waitForTimeout(1000)
        
        const value = await firstClicker.$eval('.clicker-value', el => el.textContent?.trim())
        if (value !== '0') throw new Error(`Expected 0 after reset, got ${value}`)
    })

    // ── Task filter buttons ────────────────────────────────────────────────
    await run('Filter buttons switch task view', async () => {
        // Click "Done" filter
        const filterBtns = await page.$$('.task-header sl-button-group sl-button')
        if (filterBtns.length < 3) throw new Error(`Expected 3 filter buttons, got ${filterBtns.length}`)
        
        // Count initial tasks
        const initialTasks = await page.$$('.task-item')
        
        // Click "Done" button (3rd)
        await filterBtns[2].click()
        await page.waitForTimeout(500)
        
        const doneTasks = await page.$$('.task-item')
        
        // Click "All" button (1st) to restore
        await filterBtns[0].click()
        await page.waitForTimeout(500)
        
        const allTasks = await page.$$('.task-item')
        
        // Done should have fewer items than All
        if (doneTasks.length >= initialTasks.length && initialTasks.length > 0) {
            // This is acceptable only if all tasks are done
            const pendingBtn = filterBtns[1]
            await pendingBtn.click()
            await page.waitForTimeout(500)
            const pendingTasks = await page.$$('.task-item')
            if (pendingTasks.length === 0) {
                // All tasks are done, that's fine
            } else {
                throw new Error(`Done filter didn't reduce items: done=${doneTasks.length} all=${initialTasks.length}`)
            }
            await filterBtns[0].click()
            await page.waitForTimeout(500)
        }
    })

    // ── Two-way value binding (input) ──────────────────────────────────────
    await run('Input two-way binding works with sl-input', async () => {
        const input = await page.$('.add-row sl-input')
        if (!input) throw new Error('Task input not found')
        
        // Type into the input
        await input.click()
        await page.keyboard.type('Test task from Playwright')
        
        // Trigger the value binding via sl-input event
        await input.dispatchEvent('sl-input')
        await page.waitForTimeout(300)
    })

    // ── Add task ───────────────────────────────────────────────────────────
    await run('Add task button creates a new task item', async () => {
        const initialCount = (await page.$$('.task-item')).length
        
        const addBtn = await page.$('.add-row sl-button[variant="primary"]')
        if (!addBtn) throw new Error('Add button not found')
        await addBtn.click()
        
        await page.waitForTimeout(800)
        
        const newCount = (await page.$$('.task-item')).length
        if (newCount <= initialCount) {
            throw new Error(`Task count didn't increase: ${initialCount} → ${newCount}`)
        }
    })

    // ── SPA navigation ─────────────────────────────────────────────────────
    await run('SPA navigation to /api works', async () => {
        const apiLink = await page.$('a[href="/api"]')
        if (!apiLink) throw new Error('API link not found')
        await apiLink.click()
        
        await page.waitForFunction(
            () => window.location.pathname === '/api',
            undefined,
            { timeout: TIMEOUT }
        )
        
        // Wait for content to change
        await page.waitForTimeout(500)
        const pathname = await page.evaluate(() => window.location.pathname)
        if (pathname !== '/api') throw new Error(`Expected /api, got ${pathname}`)
    })

    // ── SPA navigate back ──────────────────────────────────────────────────
    await run('SPA navigation back to / works', async () => {
        const homeLink = await page.$('a[href="/"]')
        if (!homeLink) throw new Error('Home link not found')
        await homeLink.click()
        
        await page.waitForFunction(
            () => window.location.pathname === '/',
            undefined,
            { timeout: TIMEOUT }
        )
        await page.waitForSelector('.hero', { timeout: TIMEOUT })
    })

    // ── Layout persists across navigation ──────────────────────────────────
    await run('Layout sidebar persists across SPA navigation', async () => {
        const sidebar = await page.$('.sidebar')
        if (!sidebar) throw new Error('Sidebar missing after navigation')
        const topbar = await page.$('.topbar')
        if (!topbar) throw new Error('Topbar missing after navigation')
    })

    // ── Toggle sidebar ─────────────────────────────────────────────────────
    await run('Sidebar toggle hides sidebar', async () => {
        const toggleBtn = await page.$('.topbar-left sl-icon-button')
        if (!toggleBtn) throw new Error('Sidebar toggle button not found')
        
        await toggleBtn.click()
        await page.waitForTimeout(1500)
        
        const sidebar = await page.$('.sidebar')
        // After toggle, sidebar should be gone (wrapped in logic :if)
        if (sidebar) {
            const visible = await sidebar.isVisible()
            if (visible) {
                const debug = await page.evaluate(() => {
                    const s = document.querySelector('.sidebar')
                    return { exists: !!s, bodyLen: document.body.innerHTML.length }
                })
                throw new Error(`Sidebar still visible. Debug: ${JSON.stringify(debug)}. Logs: ${consoleLogs.slice(-5).join(' | ')}`)
            }
        }
    })

    await run('Sidebar toggle shows sidebar again', async () => {
        const toggleBtn = await page.$('.topbar-left sl-icon-button')
        if (!toggleBtn) throw new Error('Sidebar toggle button not found')
        
        await toggleBtn.click()
        await page.waitForTimeout(1500)
        
        const sidebar = await page.$('.sidebar')
        if (!sidebar) throw new Error('Sidebar not restored after second toggle')
    })

    // ── Rapid interactions (stress test) ───────────────────────────────────
    await run('Rapid clicker increments are all captured', async () => {
        // Navigate back to home
        await page.goto(BASE, { waitUntil: 'domcontentloaded' })
        await page.waitForSelector('.clicker', { timeout: TIMEOUT })

        const firstClicker = (await page.$$('.clicker'))[0]
        if (!firstClicker) throw new Error('No clicker found')
        
        // Reset first
        const resetBtn = await firstClicker.$('.clicker-actions sl-button[variant="danger"]')
        if (resetBtn) {
            await resetBtn.click()
            await page.waitForTimeout(300)
        }
        
        const incBtn = await firstClicker.$('.clicker-actions sl-button[variant="primary"]')
        if (!incBtn) throw new Error('Increment button not found')
        
        // Click 5 times rapidly
        for (let i = 0; i < 5; i++) {
            await incBtn.click()
            await page.waitForTimeout(100)
        }
        
        // Wait for all updates to settle
        await page.waitForTimeout(1000)
        
        const finalValue = await firstClicker.$eval('.clicker-value', el => el.textContent?.trim())
        if (finalValue !== '5') {
            throw new Error(`Expected 5 after 5 rapid clicks, got ${finalValue}`)
        }
    })

    // ── Summary ────────────────────────────────────────────────────────────
    await browser.close()

    console.log('\n─── Results ───')
    const passed = results.filter(r => r.status === 'PASS').length
    const failed = results.filter(r => r.status === 'FAIL').length
    console.log(`${passed} passed, ${failed} failed out of ${results.length} tests\n`)
    
    if (failed > 0) {
        console.log('Failed tests:')
        for (const r of results.filter(r => r.status === 'FAIL')) {
            console.log(`  ✗ ${r.test}: ${r.detail}`)
        }
    }

    process.exit(failed > 0 ? 1 : 0)
}

main().catch(e => {
    console.error('Fatal error:', e)
    process.exit(1)
})
