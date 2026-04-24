// @ts-check
import path from 'path';
import { defineConfig } from 'playwright/test';

const ROOT_DIR = path.resolve(import.meta.dirname, '..', '..');
const EXAMPLES_DIR = path.join(ROOT_DIR, 'examples');

export default defineConfig({
  testDir: import.meta.dirname,
  testMatch: ['examples.e2e.js'],
  timeout: 90_000,
  expect: {
    timeout: 10_000,
  },
  fullyParallel: false,
  retries: 0,
  reporter: [['list']],
  outputDir: path.join(ROOT_DIR, 'test-results', 'playwright'),
  use: {
    baseURL: 'http://127.0.0.1:8080',
    headless: true,
    viewport: { width: 1440, height: 2200 },
    ignoreHTTPSErrors: true,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'off',
  },
  webServer: {
    command: 'bun run serve',
    cwd: EXAMPLES_DIR,
    url: 'http://127.0.0.1:8080',
    timeout: 120_000,
    reuseExistingServer: false,
  },
});
