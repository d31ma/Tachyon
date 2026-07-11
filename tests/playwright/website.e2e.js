// @ts-check
import { expect, test } from 'playwright/test';

/** @type {WeakMap<import('playwright/test').Page, string[]>} */
const browserErrors = new WeakMap();

/**
 * Tracks page errors. Resource 404s are excluded: the website is frontend-only
 * by design, so FYLO gateway probes 404 and fall back to the OPFS mirror.
 * @param {import('playwright/test').Page} page
 */
function trackBrowserErrors(page) {
  /** @type {string[]} */
  const errors = [];
  page.on('pageerror', (error) => errors.push(`pageerror: ${error.message}`));
  page.on('console', (message) => {
    if (message.type() === 'error' && !message.text().includes('Failed to load resource'))
      errors.push(`console: ${message.text()}`);
  });
  browserErrors.set(page, errors);
}

/** @param {import('playwright/test').Page} page @param {string} path */
async function openWebsite(page, path = '/') {
  await page.goto(path, { waitUntil: 'domcontentloaded' });
  await expect(page.locator('w-app-bar > header.w-app-bar')).toBeVisible();
  await page.waitForFunction(() => typeof window.__tc_rerender === 'function');
}

/** @param {import('playwright/test').Page} page @param {string} language */
function languageCard(page, language) {
  return page.locator(`[data-tac-scope="language-${language}"]`);
}

test.beforeEach(async ({ page }) => {
  trackBrowserErrors(page);
});

test('renders the landing page with persisted hero state', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openWebsite(page);

  await expect(page.getByRole('heading', { name: 'Ship the whole stack faster than light.' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Everything on this site runs on the features it describes' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Explore the live atlas' })).toBeVisible();
  await expect(page.locator('.hero-actions').getByRole('link', { name: 'Read the docs' })).toBeVisible();
  // `$visits` (sessionStorage) and `$$totalVisits` (localStorage) hero fields.
  await expect(page.locator('.hero-note')).toContainText(/greeted this browser/);

  expect(browserErrors.get(page)).toEqual([]);
});

test('the atlas shell redirects to the overview and navigates by sidebar', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openWebsite(page, '/atlas');

  // Docs-style shell: /atlas lands on the overview section.
  await expect(page).toHaveURL(/\/atlas\/overview$/);
  await expect(page.locator('.atlas-sidebar')).toBeVisible();
  await expect(page.locator('.atlas-hero h1')).toContainText('Everything Tachyon does');
  await expect(page.locator('[data-tac-scope="stats-grid"]')).toBeVisible();

  // Sidebar navigation swaps the section into the shell's slot.
  await page.locator('.atlas-sidebar-list a[href="/atlas/store"]').click();
  await expect(page).toHaveURL(/\/atlas\/store$/);
  await expect(page.locator('[data-tac-scope="panel-fylo"]')).toBeVisible();
  await expect(page.locator('.atlas-sidebar-list a[aria-current="page"]')).toHaveAttribute('href', '/atlas/store');

  expect(browserErrors.get(page)).toEqual([]);
});

test('polyglot companions exercise the portable surface on the atlas', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 2200 });
  await openWebsite(page, '/atlas/connect');

  await expect(page.locator('[data-tac-scope="panel-polyglot"]')).toBeVisible();
  await expect(page.locator('[data-tac-scope="panel-portablebridge"]')).toBeVisible();
  await expect(page.locator('.poly-language-grid > *')).toHaveCount(5);

  // Every companion language exposes a Wave button on the shared signal bus.
  for (const language of ['TypeScript', 'JavaScript', 'Dart', 'Kotlin', 'Swift', 'C#'])
    await expect(page.getByRole('button', { name: `Wave from ${language}` })).toBeVisible();

  // Wait for every card's async @onMount probe to settle before interacting.
  await expect(languageCard(page, 'javascript')).toContainText(/FYLO (local|ready)/);
  await expect(languageCard(page, 'swift')).toContainText('web.fetch is portable');

  // Real Dart semantics beside the documented JavaScript dialect semantics.
  await expect(languageCard(page, 'dart')).toContainText('7 ~/ 2 = 3 (real Dart integer division)');
  await expect(languageCard(page, 'csharp')).toContainText('7 / 2 = 3.5');

  // Kotlin's sync web prelude resolved navigator/location values on mount.
  await expect(languageCard(page, 'kotlin')).toContainText('ready / per-tab');
  await expect(languageCard(page, 'kotlin')).toContainText(/at http/);

  // One @publish from Kotlin updates every other language's @subscribe.
  await page.getByRole('button', { name: 'Wave from Kotlin' }).click();
  for (const language of ['javascript', 'dart', 'swift', 'csharp'])
    await expect(languageCard(page, language)).toContainText('from Kotlin');
  await expect(languageCard(page, 'javascript')).toContainText(/[1-9]\d* waves seen/);

  // Swift drives the local-first fetch() prelude.
  await page.getByRole('button', { name: 'Fetch this page' }).click();
  await expect(languageCard(page, 'swift')).toContainText('HTTP 200 via local-first fetch()');

  // Dart and C# run FYLO CRUD against the OPFS-backed browser engine.
  await languageCard(page, 'dart').getByRole('button', { name: 'Seed FYLO' }).click();
  await expect(languageCard(page, 'dart')).toContainText(/\d+ documents in OPFS/);
  await languageCard(page, 'csharp').getByRole('button', { name: 'Seed FYLO' }).click();
  await expect(languageCard(page, 'csharp')).toContainText(/\d+ documents in OPFS/);

  // The Rust device bridge fails closed in a browser bundle, by name.
  const bridge = page.locator('[data-tac-scope="panel-portablebridge"]');
  await bridge.getByRole('button', { name: 'Copy bridge report' }).click();
  await expect(bridge).toContainText('clipboard requires a clipboard.writeText bundle capability');
  await bridge.getByRole('button', { name: 'Inspect native files' }).click();
  await expect(bridge).toContainText('native filesystem is unavailable in this browser build');

  expect(browserErrors.get(page)).toEqual([]);
});

test('mobile landing stays free of horizontal overflow', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openWebsite(page);

  await expect(page.getByRole('heading', { name: 'Ship the whole stack faster than light.' })).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1))
    .toBe(true);

  expect(browserErrors.get(page)).toEqual([]);
});
