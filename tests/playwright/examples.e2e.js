// @ts-check
import { promises as fs } from 'fs';
import path from 'path';
import { expect, test } from 'playwright/test';

const ROOT_DIR = path.resolve(import.meta.dirname, '..', '..');
const EXAMPLES_DIR = path.join(ROOT_DIR, 'examples');
const ITEMS_PATH = path.join(EXAMPLES_DIR, 'server', 'data', 'items.json');
const SCREENSHOT_PATH = path.join(ROOT_DIR, 'test-results', 'playwright', 'examples-home.png');
const FIXTURE_ITEMS = [
  {
    id: 'seed-widget',
    name: 'Widget',
    source: 'seed',
    createdAt: '2026-04-21T09:00:00.000Z',
  },
  {
    id: 'seed-gadget',
    name: 'Gadget',
    source: 'seed',
    createdAt: '2026-04-21T09:05:00.000Z',
  },
];

/** @type {WeakMap<import('playwright/test').Page, string[]>} */
const browserErrorMap = new WeakMap();

/** @type {string} */
let originalItems = '[]';

/**
 * @param {typeof FIXTURE_ITEMS} items
 */
async function writeItems(items) {
  await fs.writeFile(ITEMS_PATH, JSON.stringify(items, null, 2));
}

async function restoreItems() {
  await fs.writeFile(ITEMS_PATH, originalItems);
}

/**
 * @param {import('playwright/test').Page} page
 * @returns {string[]}
 */
function trackBrowserErrors(page) {
  const existingErrors = browserErrorMap.get(page);
  if (existingErrors) {
    return existingErrors;
  }

  /** @type {string[]} */
  const errors = [];

  page.on('pageerror', (error) => {
    errors.push(`pageerror: ${error.message}`);
  });

  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push(`console: ${message.text()}`);
    }
  });

  browserErrorMap.set(page, errors);
  return errors;
}

/**
 * @param {string[]} errors
 */
function expectNoBrowserErrors(errors) {
  expect(errors, `Unexpected browser errors:\n${errors.join('\n')}`).toEqual([]);
}

/**
 * @param {import('playwright/test').Page} page
 * @returns {Promise<void>}
 */
async function waitForDashboardReady(page) {
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.waitForSelector('.hero');
  await expect(page.getByText('Hello from Yon on Bun!')).toBeVisible();
  await expect(page.getByText('Tac local-first data')).toBeVisible();
  await expect(page.getByText('ok/ready')).toBeVisible();
  await expect(page.locator('#inventory')).toBeVisible();
  await expect(
    page.getByText('No saved items yet. Create one and Yon will persist it for the next request.')
      .or(page.locator('.inventory-list'))
  ).toBeVisible();
}

/**
 * @param {import('playwright/test').Page} page
 * @returns {Promise<void>}
 */
async function expectInteractiveSurface(page) {
  await expect(page.locator('.input-showcase')).toBeVisible();
  const showcasedInputTypes = await page.locator('.input-showcase input').evaluateAll((inputs) =>
    [...new Set(inputs.map((input) => /** @type {HTMLInputElement} */ (input).type))].sort(),
  );
  expect(showcasedInputTypes).toEqual([
    'button',
    'checkbox',
    'color',
    'date',
    'datetime-local',
    'email',
    'file',
    'hidden',
    'image',
    'month',
    'number',
    'password',
    'radio',
    'range',
    'reset',
    'search',
    'submit',
    'tel',
    'text',
    'time',
    'url',
    'week',
  ]);
  await expect(page.locator('.input-showcase textarea')).toHaveCount(1);
  await expect(page.locator('.input-showcase select')).toHaveCount(1);
  await expect(page.locator('#browser-studio canvas')).toBeVisible();
  await expect(page.locator('#browser-studio progress')).toHaveAttribute('value', '62');
  await expect(page.locator('#browser-studio meter')).toHaveAttribute('value', '62');
  await expect(page.locator('#browser-studio').getByRole('button', { name: 'Surge' })).toBeVisible();
  await expect(page.locator('#inventory').getByRole('button', { name: 'Save' })).toBeVisible();
}

/**
 * @param {import('playwright/test').Page} page
 * @param {string} label
 */
function componentPanel(page, label) {
  return page.locator('.component-grid .panel').filter({ hasText: label });
}

test.beforeAll(async () => {
  originalItems = await fs.readFile(ITEMS_PATH, 'utf8').catch(() => '[]');
});

test.beforeEach(async ({ page }) => {
  await writeItems(FIXTURE_ITEMS);
  trackBrowserErrors(page);
});

test.afterAll(async () => {
  await restoreItems();
});

test('examples homepage looks polished and loads live dashboard data', async ({ page }) => {
  await waitForDashboardReady(page);
  await expectInteractiveSurface(page);
  await expect(page.locator('.shell')).toBeVisible();
  await expect(page.locator('.sidebar-brand')).toContainText('Tac + Yon');
  await expect(page.locator('.hero')).toContainText('Build what the browser can imagine, backed by Yon and FYLO');
  await expect(page.locator('.journey-map')).toContainText('Compose');
  await expect(page.locator('.stats')).toContainText('Session visits');
  await page.screenshot({ path: SCREENSHOT_PATH, fullPage: true });
  expectNoBrowserErrors(browserErrorMap.get(page) ?? []);
});

test('fylo browser uses responsive M2 controls without browser console errors', async ({ page }) => {
  const browserErrors = trackBrowserErrors(page);

  await page.goto('/_fylo', { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('heading', { name: /Inspect and query collections/ })).toBeVisible();
  await expect(page.locator('#fylo-query-source')).toBeVisible();
  await expect(page.locator('md-outlined-text-field, md-text-button, md-filled-button, md-assist-chip')).toHaveCount(0);
  await expect
    .poll(async () => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1))
    .toBe(true);

  await page.setViewportSize({ width: 390, height: 900 });
  await expect(page.locator('.fylo-shell')).toBeVisible();
  await expect
    .poll(async () => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1))
    .toBe(true);

  expectNoBrowserErrors(browserErrors);
});

test('Tac navigation preserves query parameters and fragments', async ({ page }) => {
  const browserErrors = trackBrowserErrors(page);

  await page.goto('/?tour=direct#compose', { waitUntil: 'domcontentloaded' });
  await page.waitForSelector('.hero');
  await expect(page).toHaveURL(/\?tour=direct#compose$/);
  await expect(page.getByText('Hello from Yon on Bun!')).toBeVisible();

  await page.evaluate(() => {
    const link = document.createElement('a');
    link.id = 'navigation-query-fixture';
    link.href = '/?tour=click#react';
    link.textContent = 'Query navigation fixture';
    document.body.append(link);
  });
  await page.evaluate(() => /** @type {HTMLAnchorElement} */ (document.querySelector('#navigation-query-fixture')).click());
  await expect(page).toHaveURL(/\?tour=click#react$/);

  await page.evaluate(() => history.back());
  await expect(page).toHaveURL(/\?tour=direct#compose$/);
  await page.evaluate(() => history.forward());
  await expect(page).toHaveURL(/\?tour=click#react$/);
  expectNoBrowserErrors(browserErrors);
});

test('every example button and input responds correctly', async ({ page }) => {
  const browserErrors = trackBrowserErrors(page);
  await waitForDashboardReady(page);
  await expectInteractiveSurface(page);

  const sidebar = page.locator('.sidebar');
  await expect(sidebar).toBeVisible();
  await page.getByRole('button', { name: 'Menu' }).click();
  await expect(sidebar).toBeHidden();
  await page.getByRole('button', { name: 'Menu' }).click();
  await expect(sidebar).toBeVisible();

  const themeSurface = page.locator('.topbar').first();
  const lightSurface = await themeSurface.evaluate((element) => getComputedStyle(element).backgroundColor);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('button', { name: 'Theme', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect.poll(async () => themeSurface.evaluate((element) => getComputedStyle(element).backgroundColor)).not.toBe(lightSurface);
  await page.getByRole('button', { name: 'Toggle theme', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect.poll(async () => themeSurface.evaluate((element) => getComputedStyle(element).backgroundColor)).toBe(lightSurface);

  await page.getByRole('link', { name: 'Open API docs' }).click();
  await expect(page).toHaveURL(/\/api-docs$/);
  await expect(page.getByRole('heading', { name: 'OpenAPI rendered with a Tachyon-native interactive console.' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open JSON spec' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Operations' })).toBeVisible();
  await expect(page.getByLabel('Search operations')).toBeVisible();
  await page.getByLabel('Search operations').fill('/languages/javascript');
  const docsOperation = page.locator('#operation-get-languages-javascript');
  await expect(docsOperation).toBeVisible();
  await docsOperation.locator('.operation-toggle').click();
  await docsOperation.getByRole('button', { name: 'Try it out' }).click();
  await docsOperation.getByRole('button', { name: 'Execute' }).click();
  await expect(docsOperation.locator('.live-response')).toContainText('Hello from Yon on Bun!');
  await expect(docsOperation.locator('.live-response')).toContainText('200');
  await page.goBack();
  await waitForDashboardReady(page);

  const requestIdCell = page.locator('.definition-grid dd').filter({ hasText: /.+/ }).nth(1);
  const firstRequestId = await requestIdCell.textContent();
  await page.getByRole('button', { name: 'Refresh' }).first().click();
  await expect.poll(async () => await requestIdCell.textContent()).not.toBe(firstRequestId);

  const itemInput = page.locator('#inventory input[placeholder="Create a backend item"]');
  await itemInput.fill('Playwright Widget');
  await expect(itemInput).toHaveValue('Playwright Widget');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Saved "Playwright Widget" through Yon.')).toBeVisible();
  await expect(page.locator('.inventory-list')).toContainText('Playwright Widget');
  await expect(itemInput).toHaveValue('');

  await page.getByRole('button', { name: 'Refresh' }).nth(1).click();
  await expect(page.locator('.inventory-list')).toContainText('Playwright Widget');

  const draft = page.locator('#frontend textarea');
  const studio = page.locator('#browser-studio');
  await studio.getByRole('button', { name: 'Surge' }).click();
  await expect(studio.locator('progress')).toHaveAttribute('value', '88');
  await expect(studio.locator('output')).toContainText('Surge palette selected');
  await studio.getByLabel('Studio intensity').fill('45');
  await expect(studio.locator('meter')).toHaveAttribute('value', '45');
  await expect(studio.locator('output')).toContainText('45% energy');
  await studio.getByRole('button', { name: 'Redraw' }).click();
  await expect(studio.locator('time')).toContainText('Frame');
  const exampleName = page.getByLabel('Input text', { exact: true });
  await exampleName.fill('Reactive Tac input');
  await expect(page.locator('.input-showcase .event-receipt')).toContainText('Reactive Tac input');
  await expect(exampleName).toHaveValue('Reactive Tac input');
  const subscribed = page.getByLabel('Input checkbox');
  await subscribed.check();
  await expect(subscribed).toBeChecked();
  const stack = page.getByLabel('Input select');
  await stack.selectOption('fullstack');
  await expect(stack).toHaveValue('fullstack');
  await expect(page.locator('.input-showcase .event-receipt')).toContainText('fullstack');

  await draft.fill('Playwright draft note');
  await expect(draft).toHaveValue('Playwright draft note');
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForSelector('.hero');
  await expect(draft).toHaveValue('Playwright draft note');
  await page.getByRole('button', { name: 'Clear draft' }).click();
  await expect(draft).toHaveValue('');

  const persistentPanel = componentPanel(page, 'Persistent clicks');
  const persistentValue = persistentPanel.locator('.value');
  const persistentStart = Number((await persistentValue.textContent())?.trim() ?? '0');
  await persistentPanel.getByRole('button', { name: 'Click', exact: true }).click();
  await expect(persistentValue).toHaveText(String(persistentStart + 1));
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForSelector('.hero');
  await expect(componentPanel(page, 'Persistent clicks').locator('.value')).toHaveText(String(persistentStart + 1));
  await persistentPanel.getByRole('button', { name: 'Reset', exact: true }).click();
  await expect(persistentValue).toHaveText('0');

  const sessionPanel = componentPanel(page, 'Session clicks');
  const sessionValue = sessionPanel.locator('.value');
  const sessionStart = Number((await sessionValue.textContent())?.trim() ?? '0');
  await sessionPanel.getByRole('button', { name: 'Click', exact: true }).click();
  await expect(sessionValue).toHaveText(String(sessionStart + 1));
  await sessionPanel.getByRole('button', { name: 'Reset', exact: true }).click();
  await expect(sessionValue).toHaveText('0');

  await page.getByRole('button', { name: 'Clear', exact: true }).click();
  await expect(page.getByText('Inventory reset.')).toBeVisible();
  await expect(page.getByText('No saved items yet. Create one and Yon will persist it for the next request.')).toBeVisible();

  await page.getByRole('button', { name: 'Reload live data' }).click();
  await expect(page.getByText('Hello from Yon on Bun!')).toBeVisible();
  await expect(page.getByText('Tac local-first data')).toBeVisible();
  expectNoBrowserErrors(browserErrors);
});
