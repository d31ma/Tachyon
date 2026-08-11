#!/usr/bin/env node
// Offline-cache gate.
//
// A service worker that installs but never serves a request offline is the
// exact defect this guards against, so nothing here is asserted from the
// generated files. The page is loaded in a real browser, the network is cut,
// and the page is reloaded.
//
// A worker needs a secure context and must not be on a loopback host, since
// the generated registration deliberately skips loopback so a stale cache
// cannot fight live reload. The fixture is therefore served over HTTPS with a
// throwaway certificate, under a name mapped back to the loopback interface.

import { chromium } from 'playwright';
import { createServer } from 'node:https';
import { spawnSync } from 'node:child_process';
import { createReadStream, existsSync, mkdirSync, readFileSync, statSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const TY = process.env.TAC_BIN ?? path.join(REPO, 'target/release/ty');
const PROJECT = path.join(tmpdir(), 'ty-service-worker-gate');
const HOST = 'tachyon.test';
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json' };

const write = (relative, contents) => {
  const file = path.join(PROJECT, relative);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, contents);
};

rmSync(PROJECT, { recursive: true, force: true });
write('client/pages/tac.html', '<main aria-label="Offline"><h1 id="title">Cached page</h1></main>\n');
write('client/pages/tac.css', '#title { color: rgb(0, 128, 0) }\n');

const built = spawnSync(TY, ['build', PROJECT], { encoding: 'utf8' });
if (built.status !== 0) {
  console.error(built.stderr || built.stdout);
  process.exit(1);
}

// A throwaway certificate, valid only for this run.
const key = path.join(tmpdir(), 'ty-service-worker-key.pem');
const certificate = path.join(tmpdir(), 'ty-service-worker-cert.pem');
const issued = spawnSync('openssl', [
  'req', '-x509', '-newkey', 'rsa:2048', '-nodes',
  '-keyout', key, '-out', certificate, '-days', '1',
  '-subj', `/CN=${HOST}`, '-addext', `subjectAltName=DNS:${HOST}`,
], { encoding: 'utf8' });
if (issued.status !== 0) {
  console.error(`cannot issue a test certificate: ${issued.stderr}`);
  process.exit(1);
}

const root = path.join(PROJECT, 'dist');
let requests = 0;
const server = createServer({ key: readFileSync(key), cert: readFileSync(certificate) }, (request, response) => {
  requests += 1;
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
const { port } = server.address();
const origin = `https://${HOST}:${port}`;
const profile = path.join(tmpdir(), 'ty-service-worker-profile');
rmSync(profile, { recursive: true, force: true });
const context = await chromium.launchPersistentContext(profile, {
  ignoreHTTPSErrors: true,
  args: [
    `--host-resolver-rules=MAP ${HOST} 127.0.0.1`,
    // A worker script is fetched by the network service, which does not honour
    // the page-level certificate override.
    '--ignore-certificate-errors',
  ],
});
// A hung step must fail the gate, not stall it.
context.setDefaultTimeout(15_000);
context.setDefaultNavigationTimeout(15_000);
const page = await context.newPage();
page.on('console', (message) => {
  if (message.type() === 'error') console.log(`    note console: ${message.text()}`);
});

try {
  await page.goto(`${origin}/`, { waitUntil: 'networkidle' });

  // The worker must reach the controlling state, or nothing below proves
  // anything: an uncontrolled page would simply be served by the network.
  const controller = await page.evaluate(async () => {
    const registration = await Promise.race([
      navigator.serviceWorker.ready,
      new Promise((_resolve, reject) => setTimeout(() => reject(new Error('worker never became ready')), 8000)),
    ]);
    await new Promise((resolve) => {
      if (navigator.serviceWorker.controller) return resolve();
      navigator.serviceWorker.addEventListener('controllerchange', resolve, { once: true });
      setTimeout(resolve, 5000);
    });
    return {
      scope: new URL(registration.scope).pathname,
      script: new URL(registration.active?.scriptURL ?? '').pathname,
      controlled: Boolean(navigator.serviceWorker.controller),
    };
  });
  expect(controller.script, '/tachyon-sw.js', 'worker served from the root');
  expect(controller.scope, '/', 'scope covers every page');
  expect(controller.controlled, true, 'page is controlled');

  // A reload while controlled populates the cache with what the page uses.
  await page.reload({ waitUntil: 'networkidle' });
  const cachedPaths = await page.evaluate(async () => {
    const names = await caches.keys();
    const cache = await caches.open(names.find((name) => name.startsWith('tachyon-static-')));
    return (await cache.keys()).map((request) => new URL(request.url).pathname).sort();
  });
  expect(cachedPaths.includes('/'), true, `document cached (${cachedPaths.join(' ')})`);
  expect(cachedPaths.some((entry) => entry.endsWith('.css')), true, 'stylesheet cached');

  // The real assertion: with the network gone, the page still loads and its
  // stylesheet still applies.
  const before = requests;
  await context.setOffline(true);
  await page.reload({ waitUntil: 'load' });
  expect(await page.textContent('#title'), 'Cached page', 'document served offline');
  expect(
    await page.evaluate(() => getComputedStyle(document.getElementById('title')).color),
    'rgb(0, 128, 0)',
    'stylesheet served offline',
  );
  await context.setOffline(false);
  console.log(`    note server saw ${requests - before} request(s) while offline`);

  // On a loopback host the worker must not register, or a stale cache would
  // fight the development server's live reload. Registrations are per-origin,
  // so the same profile proves this without a second browser.
  const loopbackPage = await context.newPage();
  await loopbackPage.goto(`https://127.0.0.1:${port}/`, { waitUntil: 'networkidle' });
  await loopbackPage.waitForTimeout(600);
  expect(
    await loopbackPage.evaluate(async () => (await navigator.serviceWorker.getRegistrations()).length),
    0,
    'no worker registered on a loopback host',
  );
  await loopbackPage.close();
}
finally {
  await context.close();
  server.close();
}

if (!process.exitCode) console.log('\nofflinecache gate passed');
