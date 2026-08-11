#!/usr/bin/env node
// Tac client event gate.
//
// Builds a project that binds on:<event> handlers, serves it, and drives the
// page in a real browser. A marker that renders but never fires is the exact
// defect this replaces, so nothing here is asserted from the generated HTML
// alone.

import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { spawnSync } from 'node:child_process';
import { createReadStream, existsSync, mkdirSync, statSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const TY = process.env.TAC_BIN ?? path.join(REPO, 'target/release/ty');
const PROJECT = path.join(tmpdir(), 'ty-events-gate');
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json' };

const write = (relative, contents) => {
  const file = path.join(PROJECT, relative);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, contents);
};

rmSync(PROJECT, { recursive: true, force: true });
write('client/pages/tac.html', `<main aria-label="Events">
  <output id="count">0</output>
  <button on:click="increment()">Add one</button>
  <button on:click="addBy(5)">Add five</button>
  <input id="name" on:input="rename()">
  <p id="greeting">-</p>
  <input id="typed" on:input="record($event.target.value)">
  <p id="echo">-</p>
  <input id="labelled" on:input="labelled('email', $event.target.value)">
  <p id="pair">-</p>
  <button id="raw" on:click="describe($event)">Describe</button>
  <p id="described">-</p>
</main>
`);
write('client/pages/tac.js', `export function increment() {
  const out = document.getElementById('count')
  out.textContent = String(Number(out.textContent) + 1)
}
export function addBy(_event, amount) {
  const out = document.getElementById('count')
  out.textContent = String(Number(out.textContent) + amount)
}
export function rename(event) {
  document.getElementById('greeting').textContent = 'Hello ' + event.target.value
}
export function record(_event, value) {
  document.getElementById('echo').textContent = String(value)
}
export function labelled(_event, field, value) {
  document.getElementById('pair').textContent = field + '=' + String(value)
}
export function describe(_event, passed) {
  document.getElementById('described').textContent = passed?.type ?? 'missing'
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
  else fail(`${label}: expected ${wanted}, got ${actual}`);
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
  expect(await page.textContent('#count'), '0', 'client-rendered value');

  await page.click('text=Add one');
  await page.waitForTimeout(150);
  expect(await page.textContent('#count'), '1', 'handler with no arguments');

  await page.click('text=Add five');
  await page.waitForTimeout(150);
  expect(await page.textContent('#count'), '6', 'handler with a literal argument');

  await page.fill('#name', 'Ada');
  await page.waitForTimeout(200);
  expect(await page.textContent('#greeting'), 'Hello Ada', 'handler receiving the event');

  // $event and its property paths are the convention every template framework
  // converged on, so a binding must be able to read the value being typed.
  await page.fill('#typed', 'Grace');
  await page.waitForTimeout(200);
  expect(await page.textContent('#echo'), 'Grace', '$event.target.value argument');

  await page.fill('#labelled', 'ada@example.test');
  await page.waitForTimeout(200);
  expect(await page.textContent('#pair'), 'email=ada@example.test', 'literal beside $event path');

  await page.click('#raw');
  await page.waitForTimeout(200);
  expect(await page.textContent('#described'), 'click', 'bare $event argument');

  if (errors.length) fail(`console errors: ${errors.join(' | ')}`);
  else console.log('    ok   no console errors');
} finally {
  await browser.close();
  server.close();
}

console.log(process.exitCode ? 'FAIL: Tac client event gate' : 'PASS: Tac client event gate');
