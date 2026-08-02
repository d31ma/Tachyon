#!/usr/bin/env node
// Island-local client evaluation gate (ADR 0010).
//
// An expression an island defers must actually resolve against the companion
// instance in a real browser. A marker that ships but never fills is the exact
// defect this guards, so nothing here is asserted from the generated HTML.

import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { spawnSync } from 'node:child_process';
import { createReadStream, existsSync, mkdirSync, statSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const TY = process.env.TY_BIN ?? path.join(REPO, 'target/release/ty');
const PROJECT = path.join(tmpdir(), 'ty-island-expression-gate');
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json' };

const write = (relative, contents) => {
  const file = path.join(PROJECT, relative);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, contents);
};

rmSync(PROJECT, { recursive: true, force: true });
write('client/pages/tac.html', '<main aria-label="Island"><panel-status hydrate="load" /></main>\n');
write('client/components/panel/status/tac.html', `<section>
  <p id="method">{loadingState()}</p>
  <p id="field">{label}</p>
  <p id="maths">{count + 1}</p>
  <p id="ternary">{count > 3 ? 'many' : 'few'}</p>
  <p id="nested">{report.title}</p>
  <p id="chained">{report.rows.length}</p>
  <p id="awaited">{await note()}</p>
  <p id="live">{count}</p>
  <button id="bump" on:click="count += 1">bump</button>
  <button id="reset" on:click="reset()">reset</button>
  <input id="typed" on:input="label = $event.target.value">
  <p id="typedOut">{label}</p>
</section>
`);
write('client/components/panel/status/tac.js', `export default class {
  label = 'ready'
  count = 6
  report = { title: 'Report', rows: [1, 2, 3] }
  loadingState() { return 'live' }
  async note() { return 'awaited' }
  reset() { this.count = 0 }
  hydrate() {}
}
`);

const built = spawnSync(TY, ['build', PROJECT], { encoding: 'utf8' });
if (built.status !== 0) {
  console.error(built.stderr || built.stdout);
  process.exit(1);
}

const root = path.join(PROJECT, 'dist');
const server = createServer((request, response) => {
  let file = path.join(root, decodeURIComponent(request.url.split('?')[0]));
  if (existsSync(file) && statSync(file).isDirectory()) file = path.join(file, 'index.html');
  if (!existsSync(file)) { response.writeHead(404).end(); return; }
  response.writeHead(200, { 'content-type': MIME[path.extname(file)] ?? 'application/octet-stream' });
  createReadStream(file).pipe(response);
});

const fail = (message) => { console.error(`FAIL: ${message}`); process.exitCode = 1; };
const expect = (actual, wanted, label) => {
  if (actual === wanted) console.log(`    ok   ${label}: ${actual}`);
  else fail(`${label}: expected ${JSON.stringify(wanted)}, got ${JSON.stringify(actual)}`);
};

await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on('console', (message) => { if (message.type() === 'error') errors.push(message.text()); });
page.on('pageerror', (error) => errors.push(String(error)));

try {
  await page.goto(`${origin}/`, { waitUntil: 'networkidle' });
  await page.waitForFunction(() => !document.querySelector('tachyon-expr'), null, { timeout: 5000 });

  expect(await page.textContent('#method'), 'live', 'method call on the companion');
  expect(await page.textContent('#field'), 'ready', 'companion field');
  expect(await page.textContent('#maths'), '7', 'arithmetic over a companion field');
  expect(await page.textContent('#ternary'), 'many', 'conditional over a companion field');
  expect(await page.textContent('#nested'), 'Report', 'nested property');
  expect(await page.textContent('#chained'), '3', 'property of a nested array');
  expect(await page.textContent('#awaited'), 'awaited', 'awaited companion method');

  // An assigning binding writes to the instance, and the island re-resolves
  // its own expressions, so the rendered value follows.
  expect(await page.textContent('#live'), '6', 'initial value');
  await page.click('#bump');
  await page.waitForFunction(() => document.querySelector('#live').textContent === '7');
  expect(await page.textContent('#live'), '7', 'compound assignment updates the view');
  await page.click('#bump');
  await page.waitForTimeout(100);
  expect(await page.textContent('#live'), '8', 'assignment accumulates');

  // A handler on the companion updates the view too.
  await page.click('#reset');
  await page.waitForTimeout(100);
  expect(await page.textContent('#live'), '0', 'companion method updates the view');
  expect(await page.textContent('#maths'), '1', 'every expression refreshes, not just one');

  // A plain assignment from $event.
  await page.fill('#typed', 'typed value');
  await page.waitForTimeout(150);
  expect(await page.textContent('#typedOut'), 'typed value', 'assignment from $event');

  // The island must be marked active, not silently failed.
  expect(
    await page.getAttribute('tachyon-island', 'data-tachyon-active'),
    'true',
    'island activated',
  );
  expect(await page.getAttribute('tachyon-island', 'data-tachyon-island-error'), null, 'no island error');
  expect(errors.length, 0, `no console errors${errors.length ? `: ${errors[0]}` : ''}`);
}
finally {
  await browser.close();
  server.close();
}

if (!process.exitCode) console.log('\nisland expression gate passed');
