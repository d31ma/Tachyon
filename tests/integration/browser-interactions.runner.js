// @ts-check
import { chromium } from 'playwright';

/**
 * @typedef {{ test: string, status: 'PASS' | 'FAIL', ms: number, detail?: string }} BrowserRunResult
 */

const BASE = 'http://localhost:8080';
const TIMEOUT = 8000;
const STARTUP_DELAY_MS = 1000;

/** @type {import('playwright').Browser | undefined} */
let browser;
/** @type {import('playwright').Page | undefined} */
let page;
/** @type {BrowserRunResult[]} */
const results = [];

/**
 * @param {string} name
 * @param {() => Promise<void>} fn
 */
async function run(name, fn) {
    const t0 = performance.now();
    try {
        await fn();
        results.push({ test: name, status: 'PASS', ms: Math.round(performance.now() - t0) });
        console.log(`  [PASS] ${name}`);
    }
    catch (error) {
        const detail = error instanceof Error ? error.message : String(error);
        results.push({ test: name, status: 'FAIL', ms: Math.round(performance.now() - t0), detail });
        console.log(`  [FAIL] ${name} - ${detail}`);
    }
}

async function startServer() {
    new Worker('./tests/integration/server-worker.js').postMessage({
        script: './src/cli/serve.js',
        cwd: './examples',
    });
    await Bun.sleep(STARTUP_DELAY_MS);
}

async function main() {
    console.log('\n[Tachyon Browser Smoke Tests]\n');
    await startServer();
    browser = await chromium.launch({ headless: true });
    page = await (await browser.newContext()).newPage();

    await run('Page loads and renders the main UI', async () => {
        if (!page)
            throw new Error('Browser page failed to initialize');
        const resp = await page.goto(BASE, { waitUntil: 'domcontentloaded' });
        if (!resp || !resp.ok())
            throw new Error(`Page load failed: ${resp?.status()}`);
        await page.waitForSelector('.hero', { timeout: TIMEOUT });
        await page.waitForSelector('.sidebar-brand', { timeout: TIMEOUT });
    });

    await run('Component content renders in the browser', async () => {
        if (!page)
            throw new Error('Browser page failed to initialize');
        await page.waitForSelector('[data-tac-scope=\"clicker\"] .panel, [data-tac-scope=\"ui-clicker\"] .panel', { timeout: TIMEOUT });
        const clickers = await page.$$('[data-tac-scope=\"clicker\"] .panel, [data-tac-scope=\"ui-clicker\"] .panel');
        if (clickers.length < 1)
            throw new Error('Expected at least one rendered clicker');
    });

    await run('SPA navigation still works', async () => {
        if (!page)
            throw new Error('Browser page failed to initialize');
        const apiLink = await page.$('a[href="/api"]');
        if (!apiLink)
            throw new Error('API link not found');
        await apiLink.click();
        await page.waitForFunction(() => window.location.pathname === '/api', undefined, { timeout: TIMEOUT });
        await page.goto(BASE, { waitUntil: 'domcontentloaded' });
        await page.waitForSelector('.hero', { timeout: TIMEOUT });
    });

    await browser.close();
    const failed = results.filter((result) => result.status === 'FAIL');
    console.log(`\n${results.length - failed.length} passed, ${failed.length} failed\n`);
    if (failed.length > 0) {
        for (const result of failed) {
            console.log(`  [FAIL] ${result.test}: ${result.detail}`);
        }
    }
    process.exit(failed.length > 0 ? 1 : 0);
}

main().catch(async (error) => {
    console.error('Fatal error:', error);
    if (browser)
        await browser.close().catch(() => { });
    process.exit(1);
});
