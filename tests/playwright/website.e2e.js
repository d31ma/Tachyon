// @ts-check
import { expect, test } from 'playwright/test';

/** @type {WeakMap<import('playwright/test').Page, string[]>} */
const browserErrors = new WeakMap();

/** @param {import('playwright/test').Page} page */
function trackBrowserErrors(page) {
  /** @type {string[]} */
  const errors = [];
  page.on('pageerror', (error) => errors.push(`pageerror: ${error.message}`));
  page.on('console', (message) => {
    if (message.type() === 'error')
      errors.push(`console: ${message.text()}`);
  });
  browserErrors.set(page, errors);
}

/** @param {import('playwright/test').Page} page */
async function openWebsite(page) {
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('heading', { name: 'One codebase. Every surface.' })).toBeVisible();
  await expect(page.locator('w-app > .w-app')).toBeVisible();
  await page.waitForFunction(() => typeof window.__tc_rerender === 'function');
}

/** @param {import('playwright/test').Page} page */
async function expectDuvayShells(page) {
  await expect(page.locator('w-app > .w-app')).toHaveCount(1);
  await expect(page.locator('w-app-bar > header.w-app-bar')).toHaveCount(1);
  await expect(page.locator('w-navigation-drawer > aside.w-navigation-drawer')).toHaveCount(1);
  await expect(page.locator('w-card > .w-card')).toHaveCount(22);
  await expect(page.locator('w-btn > .w-btn')).toHaveCount(10);
}

test.beforeEach(async ({ page }) => {
  trackBrowserErrors(page);
});

test('renders the polished DUVAY landing experience', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openWebsite(page);
  await expectDuvayShells(page);

  await expect(page.getByRole('heading', { name: 'A full stack with one mental model.' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'One endpoint, your language.' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'See it work.' })).toBeVisible();
  await expect(page.locator('[data-tac-scope="panel-polyglot"]')).toBeVisible();
  await expect(page.locator('[data-tac-scope="panel-desktop"]')).toBeVisible();
  await expect(page.locator('[data-tac-scope="panel-native"]')).toBeVisible();
  await expect(page.getByText('Tachyon 26.23', { exact: false })).toHaveCount(0);

  const edgeAlignment = await page.evaluate(() => {
    const hero = document.querySelector('.tc-hero')?.getBoundingClientRect();
    const main = document.querySelector('w-main')?.getBoundingClientRect();
    return { heroLeft: hero?.left, heroRight: hero?.right, mainLeft: main?.left, mainRight: main?.right };
  });
  expect(edgeAlignment.heroLeft).toBe(edgeAlignment.mainLeft);
  expect(edgeAlignment.heroRight).toBe(edgeAlignment.mainRight);

  await page.locator('.tc-drawer-list a[href="#demo"]').click();
  await expect(page).toHaveURL(/#demo$/);
  await expect
    .poll(() => page.evaluate(() => Math.abs(document.querySelector('#demo')?.getBoundingClientRect().top ?? 9999)))
    .toBeLessThan(100);
  expect(browserErrors.get(page)).toEqual([]);
});

test('stateful interactions preserve DUVAY Light DOM component shells', async ({ page }) => {
  await openWebsite(page);

  for (const language of ['Python', 'Rust', 'Go']) {
    await page.getByRole('tab', { name: language }).click();
    await expect(page.getByRole('tabpanel')).toContainText(`Hello from ${language}`);
    await expectDuvayShells(page);
  }

  for (const action of ['Calm', 'Surge', 'Redraw']) {
    await page.getByRole('button', { name: action, exact: true }).click();
    await expectDuvayShells(page);
  }

  await expect(page.getByText('Native canvas rendering controlled by companion state.')).toBeVisible();
  await expectDuvayShells(page);

  await page.getByRole('button', { name: 'Copy install command' }).click();
  await expect(page.getByRole('button', { name: 'Copy install command' })).toContainText(/Copied|Selected/);
  await expectDuvayShells(page);

  await page.getByRole('button', { name: 'Run the desktop Tac worker' }).click();
  await expect(page.locator('.tc-result pre')).toContainText('"value":21');
  await expect(page.getByText('Worker unavailable')).toHaveCount(0);
  await expectDuvayShells(page);
  expect(browserErrors.get(page)).toEqual([]);
});

test('mobile layout remains touch-friendly and free of horizontal overflow', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openWebsite(page);

  await expect(page.locator('.tc-mobile-navigation')).toBeVisible();
  await expect(page.locator('w-app-bar-nav-icon')).toBeHidden();
  await expect(page.getByRole('link', { name: 'Explore the live lab' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Read the source' })).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1))
    .toBe(true);

  const mobileGeometry = await page.evaluate(() => {
    const hero = document.querySelector('.tc-hero')?.getBoundingClientRect();
    const navigation = document.querySelector('.tc-mobile-navigation .w-bottom-navigation')?.getBoundingClientRect();
    const items = [...document.querySelectorAll('.tc-mobile-navigation .w-bottom-nav-item')]
      .map((item) => item.getBoundingClientRect());
    return {
      heroLeft: hero?.left,
      heroRight: hero?.right,
      viewportWidth: window.innerWidth,
      gaps: items.slice(1).map((item, index) => item.left - items[index].right),
      widths: items.map((item) => item.width),
      leadingInset: navigation && items[0] ? items[0].left - navigation.left : null,
      trailingInset: navigation && items.at(-1) ? navigation.right - items.at(-1).right : null,
    };
  });
  expect(mobileGeometry.heroLeft).toBe(0);
  expect(mobileGeometry.heroRight).toBe(mobileGeometry.viewportWidth);
  expect(Math.min(...mobileGeometry.gaps)).toBeGreaterThanOrEqual(4);
  expect(Math.max(...mobileGeometry.widths) - Math.min(...mobileGeometry.widths)).toBeLessThanOrEqual(1);
  expect(Math.abs((mobileGeometry.leadingInset ?? 0) - (mobileGeometry.trailingInset ?? 0))).toBeLessThanOrEqual(1);
  await expect(page.locator('#site-navigation')).not.toHaveAttribute('open', '');

  const mobileNavigation = page.locator('.tc-mobile-navigation');
  await mobileNavigation.getByRole('link', { name: 'Live lab' }).click();
  await expect(page).toHaveURL(/#demo$/);
  await expect
    .poll(() => page.evaluate(() => Math.abs(document.querySelector('#demo')?.getBoundingClientRect().top ?? 9999)))
    .toBeLessThan(100);

  await mobileNavigation.getByRole('link', { name: 'Overview' }).click();
  await expect(page).toHaveURL(/#hero$/);
  await expect
    .poll(() => page.evaluate(() => Math.abs(document.querySelector('#hero')?.getBoundingClientRect().top ?? 9999)))
    .toBeLessThan(100);

  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  await expect
    .poll(() => page.evaluate(() => {
      const footer = document.querySelector('.tc-footer')?.getBoundingClientRect();
      return footer ? footer.bottom <= window.innerHeight : false;
    }))
    .toBe(true);

  expect(browserErrors.get(page)).toEqual([]);
});

test('tablet sidebar remains anchored while the document scrolls', async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 800 });
  await openWebsite(page);

  const navigation = page.locator('#site-navigation');
  const navigationToggle = page.locator('w-app-bar-nav-icon button');
  await navigationToggle.click();
  await expect(navigation).toHaveAttribute('open', '');

  await page.evaluate(() => window.scrollTo(0, 1200));
  await expect
    .poll(() => page.evaluate(() => {
      const drawer = document.querySelector('#site-navigation .w-navigation-drawer');
      if (!drawer)
        return null;
      const rect = drawer.getBoundingClientRect();
      return {
        position: getComputedStyle(drawer).position,
        top: Math.round(rect.top),
        bottom: Math.round(rect.bottom),
      };
    }))
    .toEqual({ position: 'fixed', top: 72, bottom: 800 });

  expect(browserErrors.get(page)).toEqual([]);
});
