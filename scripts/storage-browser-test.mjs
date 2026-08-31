#!/usr/bin/env node
// Browser-storage gate.
//
// Three caches, each proved against the real platform rather than against the
// generated source, because every one of them is a claim about what a browser
// does across a reload, a tab, or a cut network:
//
//   Web Storage    a `$` field is restored in the same tab and nowhere else,
//                  while a `$$` field is shared by every tab.
//   IndexedDB      tac.fetch honours its cache policy, and answers from what
//                  was stored when the network is gone.
//   CacheStorage   a component behind an untaken branch is warmed after the
//                  page is idle, so it still renders once the network is gone.
//
// The first two phases run on loopback, where the offline cache deliberately
// leaves itself unregistered, so the service worker cannot stand in for the
// layer under test. The third needs the worker, so it is served over HTTPS
// under a name mapped back to the loopback interface.

import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { createServer as createSecureServer } from 'node:https';
import { spawnSync } from 'node:child_process';
import { createReadStream, existsSync, mkdtempSync, mkdirSync, readFileSync, statSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const TY = process.env.TAC_BIN ?? path.join(REPO, 'target/release/ty');
let PROJECT;
let server;
let browser;
let secure;
let secureContext;
const HOST = 'tachyon.test';
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json' };

const write = (relative, contents) => {
  const file = path.join(PROJECT, relative);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, contents);
};

const listen = (server) => new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', () => { server.off('error', reject); resolve(); });
});
const closeServer = async (server) => {
  if (!server) return;
  const closed = new Promise((resolve, reject) => server.close((error) => {
    if (error && error.code !== 'ERR_SERVER_NOT_RUNNING') reject(error);
    else resolve();
  }));
  server.closeAllConnections();
  await closed;
};

try {
PROJECT = mkdtempSync(path.join(tmpdir(), 'ty-storage-gate-'));
// `panel-detail` sits behind a branch that is false on a first visit, so its
// module is never imported during the first render. That is exactly the asset
// the offline cache would otherwise be missing.
write('client/pages/tac.html', `<script>let $clicks = 0; let $$theme = 'light'</script>
<main aria-label="Storage">
  <button id="bump" on:click="$clicks += 1">bump</button>
  <button id="dark" on:click="$$theme = 'dark'">dark</button>
  <p id="clicks">{$clicks}</p>
  <p id="theme">{$$theme}</p>
  <logic :if="$clicks > 2"><panel-detail hydrate="load" /></logic>
</main>
`);
write('client/pages/tac.js', 'export default class {}\n');
write('client/components/panel/detail/tac.html', '<p id="detail">detail</p>\n');
write('client/components/panel/detail/tac.js', 'export default class {}\n');

const built = spawnSync(TY, ['build', PROJECT], {
  encoding: 'utf8', timeout: 120_000, killSignal: 'SIGKILL', maxBuffer: 1024 * 1024,
});
if (built.status !== 0) {
  throw new Error(`storage fixture build failed: ${built.error?.message || built.stderr || built.stdout}`);
}

const root = path.join(PROJECT, 'dist');
let counter = 0;
let rangeRequests = 0;
const handle = (request, response) => {
  const url = new URL(request.url, 'http://localhost');
  if (url.pathname === '/api/range') {
    rangeRequests += 1;
    const partial = Boolean(request.headers.range) || url.searchParams.has('partial');
    response.writeHead(partial && !url.searchParams.has('mislabelled') ? 206 : 200, {
      'content-type': 'text/plain', 'cache-control': 'public, max-age=60',
      'x-request-number': String(rangeRequests),
      ...(partial ? { 'content-range': 'bytes 0-2/6' } : {}),
    });
    response.end(partial ? 'abc' : 'abcdef');
    return;
  }
  // A counter is the cheapest way to tell a cached answer from a fresh one:
  // any repeat of a previous value proves the network was not consulted.
  if (url.pathname === '/api/counter') {
    counter += 1;
    const cacheControl = url.searchParams.get('policy') || 'public, max-age=60';
    response.writeHead(200, { 'content-type': 'application/json', 'cache-control': cacheControl });
    response.end(JSON.stringify({ value: counter }));
    return;
  }
  let file = path.join(root, decodeURIComponent(url.pathname));
  if (existsSync(file) && statSync(file).isDirectory()) file = path.join(file, 'index.html');
  if (!existsSync(file)) { response.writeHead(404).end(); return; }
  response.writeHead(200, { 'content-type': MIME[path.extname(file)] ?? 'application/octet-stream' });
  createReadStream(file).pipe(response);
};

const fail = (message) => { console.error(`FAIL: ${message}`); process.exitCode = 1; };
const expect = (actual, wanted, label) => {
  if (actual === wanted) console.log(`    ok   ${label}: ${actual}`);
  else fail(`${label}: expected ${JSON.stringify(wanted)}, got ${JSON.stringify(actual)}`);
};

// ── Phase 1 and 2: Web Storage and IndexedDB, on loopback ─────────────────
server = createServer(handle);
await listen(server);
const origin = `http://127.0.0.1:${server.address().port}`;

browser = await chromium.launch({ timeout: 30_000 });
const context = await browser.newContext();
context.setDefaultTimeout(15_000);
context.setDefaultNavigationTimeout(15_000);

  const page = await context.newPage();
  await page.goto(`${origin}/`, { waitUntil: 'networkidle' });

  expect(await page.textContent('#clicks'), '0', 'declared default renders');
  await page.click('#bump');
  await page.click('#bump');
  await page.click('#bump');
  await page.click('#dark');
  expect(await page.textContent('#clicks'), '3', 'assignment renders');

  // A reload destroys every in-memory owner, so anything still here came back
  // out of storage.
  await page.reload({ waitUntil: 'networkidle' });
  expect(await page.textContent('#clicks'), '3', '$ field restored after reload');
  expect(await page.textContent('#theme'), 'dark', '$$ field restored after reload');
  // The restored value drives rendering, not just the field.
  expect(await page.textContent('#detail'), 'detail', 'restored state re-opened the branch');

  const stored = await page.evaluate(() => ({
    session: sessionStorage.getItem('tac:/client.js:/:$clicks'),
    local: localStorage.getItem('tac:/client.js:/:$$theme'),
    sessionHasLocalField: sessionStorage.getItem('tac:/client.js:/:$$theme'),
  }));
  expect(stored.session, '3', '$ field written to sessionStorage');
  expect(stored.local, '"dark"', '$$ field written to localStorage');
  expect(stored.sessionHasLocalField, null, '$$ field kept out of sessionStorage');

  // A second tab is a second session: sessionStorage is per-tab, localStorage
  // is shared. This is the whole reason the two prefixes exist.
  const second = await context.newPage();
  await second.goto(`${origin}/`, { waitUntil: 'networkidle' });
  expect(await second.textContent('#clicks'), '0', '$ field is per-tab');
  expect(await second.textContent('#theme'), 'dark', '$$ field is shared across tabs');
  await second.close();

  // A companion reaches all of this through `this.tac`, which must stay off
  // the enumerable surface: a hot update clones owner state, and a function
  // would make the whole snapshot throw.
  const binding = await page.evaluate(() => ({
    reachable: typeof __tachyonTac.instance.tac?.fetch === 'function',
    hidden: !Object.keys(__tachyonTac.instance).includes('tac'),
    clonable: (() => { try { structuredClone({ ...__tachyonTac.instance }); return true; } catch { return false; } })(),
  }));
  expect(binding.reachable, true, 'companion reaches this.tac.fetch');
  expect(binding.hidden, true, 'tac binding is not enumerable');
  expect(binding.clonable, true, 'owner state still clones for a hot update');

  // ── IndexedDB fetch cache ───────────────────────────────────────────────
  const api = `${origin}/api/counter`;
  const value = async (expression) => page.evaluate(expression, api);

  // An older runtime cached ambient-cookie responses in database version 1.
  // Upgrading must clear that cache, not expose it to a new anonymous read.
  await page.evaluate((url) => new Promise((resolve, reject) => {
    const opened = indexedDB.open('tachyon-fetch-cache', 1);
    opened.onupgradeneeded = () => opened.result.createObjectStore('responses', { keyPath: 'key' });
    opened.onerror = () => reject(opened.error);
    opened.onsuccess = () => {
      const transaction = opened.result.transaction('responses', 'readwrite');
      transaction.objectStore('responses').put({ key: `fetch:GET:${url}`, status: 200,
        headers: [['content-type', 'application/json']], updatedAt: Date.now(),
        body: new TextEncoder().encode(JSON.stringify({ value: -999 })).buffer });
      transaction.oncomplete = () => { opened.result.close(); resolve(); };
      transaction.onerror = () => { opened.result.close(); reject(transaction.error); };
    };
  }), api);

  const first = await value(async (url) => (await __tachyonTac.tac.fetch(url, { credentials: 'omit' }, { cache: 'cache-first' })).json());
  expect(first.value !== -999, true, 'legacy credentialed cache is discarded on privacy migration');
  const repeat = await value(async (url) => (await __tachyonTac.tac.fetch(url, { credentials: 'omit' }, { cache: 'cache-first' })).json());
  expect(repeat.value, first.value, 'cache-first served the stored response');

  const fresh = await value(async (url) => (await __tachyonTac.tac.fetch(url, { credentials: 'omit' }, { cache: 'network-first' })).json());
  expect(fresh.value > first.value, true, 'network-first went to the network');

  const unstored = await value(async (url) => {
    const before = (await __tachyonTac.tac.fetch(url, { credentials: 'omit' }, { cache: 'no-store' })).clone();
    const body = await before.json();
    const entries = await new Promise((resolve) => {
      const request = indexedDB.open('tachyon-fetch-cache');
      request.onsuccess = () => {
        const store = request.result.transaction('responses', 'readonly').objectStore('responses');
        const all = store.getAllKeys();
        all.onsuccess = () => { request.result.close(); resolve(all.result); };
        all.onerror = () => { request.result.close(); resolve([]); };
      };
      request.onerror = () => resolve([]);
    });
    return { body, entries };
  });
  expect(unstored.entries.includes(`fetch:GET:${api}`), true, 'cacheable reads are keyed by URL');

  // Invalidation is what makes cache-first safe to use at all.
  await page.evaluate((url) => __tachyonTac.tac.invalidate([`fetch:GET:${url}`]), api);
  const afterInvalidate = await value(async (url) => (await __tachyonTac.tac.fetch(url, { credentials: 'omit' }, { cache: 'cache-first' })).json());
  expect(afterInvalidate.value > fresh.value, true, 'invalidate forced a refetch');

  const privacy = await page.evaluate(async (api) => {
    const read = async (suffix, init) => {
      const url = `${api}?${suffix}`;
      const first = await (await __tachyonTac.tac.fetch(url, init, { cache: 'cache-first' })).json();
      const second = await (await __tachyonTac.tac.fetch(url, init, { cache: 'cache-first' })).json();
      return second.value > first.value;
    };
    return {
      cookies: await read('ambient=yes', {}),
      authorization: await read('auth=yes', { credentials: 'omit', headers: { authorization: 'test-fixture' } }),
      noStore: await read('policy=no-store', { credentials: 'omit' }),
      private: await read('policy=private', { credentials: 'omit' }),
      requestNoStore: await read('request-no-store=yes', { credentials: 'omit', cache: 'no-store' }),
    };
  }, api);
  for (const [name, bypassed] of Object.entries(privacy)) expect(bypassed, true, `${name} bypasses persistent cache`);

  const range = await page.evaluate(async (origin) => {
    const read = async (suffix, ranged = false) => {
      const response = await __tachyonTac.tac.fetch(`${origin}/api/range?${suffix}`, {
        credentials: 'omit', ...(ranged ? { headers: { Range: 'bytes=0-2' } } : {}),
      }, { cache: 'cache-first' });
      return { status: response.status, body: await response.text(), number: response.headers.get('x-request-number') };
    };
    const partial = await read('partial-first', true);
    const full = await read('partial-first');
    const warmed = await read('full-first');
    const rangedAfterFull = await read('full-first', true);
    const stillFull = await read('full-first');
    const ifRange = await __tachyonTac.tac.fetch(`${origin}/api/range?full-first`, {
      credentials: 'omit', headers: { 'If-Range': '"fixture-etag"' },
    }, { cache: 'cache-first' });
    const ifRangeNumber = ifRange.headers.get('x-request-number');
    await ifRange.arrayBuffer();
    const unsolicited = await read('partial=yes');
    const repeatedPartial = await read('partial=yes');
    const mislabelled = await read('partial=yes&mislabelled=yes');
    const repeatedMislabelled = await read('partial=yes&mislabelled=yes');
    return { partial, full, warmed, rangedAfterFull, stillFull, ifRangeNumber, unsolicited, repeatedPartial,
      mislabelled, repeatedMislabelled };
  }, origin);
  expect(range.partial.status, 206, 'Range gets a partial network response');
  expect(range.full.status, 200, 'partial response cannot poison a later full read');
  expect(range.full.body, 'abcdef', 'full read receives the complete body');
  expect(range.rangedAfterFull.status, 206, 'Range bypasses an existing full-response cache entry');
  expect(range.stillFull.number, range.warmed.number, 'Range leaves the complete cached representation intact');
  expect(range.ifRangeNumber !== range.warmed.number, true, 'If-Range also bypasses full-response cache lookup');
  expect(range.repeatedPartial.number !== range.unsolicited.number, true, 'unsolicited 206 responses are never cached');
  expect(range.repeatedMislabelled.number !== range.mislabelled.number, true, 'Content-Range responses are never cached even with status 200');

  // Persisted entries can outlive the runtime that wrote them. Old partial
  // responses must be rejected before they can stand in for a full read.
  for (const status of [200, 206]) {
    const restored = await page.evaluate(async ({ origin, status }) => {
      const url = `${origin}/api/range?legacy=${status}`;
      await new Promise((resolve, reject) => {
        const opened = indexedDB.open('tachyon-fetch-cache');
        opened.onerror = () => reject(opened.error);
        opened.onsuccess = () => {
          const transaction = opened.result.transaction('responses', 'readwrite');
          transaction.objectStore('responses').put({ key: `fetch:GET:${url}`, status,
            headers: status === 200 ? { 'content-range': 'bytes 0-2/6' } : {},
            updatedAt: Date.now(), body: new TextEncoder().encode('abc').buffer });
          transaction.oncomplete = () => { opened.result.close(); resolve(); };
          transaction.onerror = () => { opened.result.close(); reject(transaction.error); };
        };
      });
      return (await __tachyonTac.tac.fetch(url, { credentials: 'omit' }, { cache: 'cache-first' })).text();
    }, { origin, status });
    expect(restored, 'abcdef', `legacy partial response (${status}) is rejected before cache lookup returns`);
  }

  const aborted = await page.evaluate(async (url) => {
    const controller = new AbortController();
    controller.abort();
    try {
      await __tachyonTac.tac.fetch(url, { credentials: 'omit', signal: controller.signal });
      return false;
    } catch { return true; }
  }, api);
  expect(aborted, true, 'aborted request never falls back to stored data');

  // The payoff: with the network gone, a read still answers from IndexedDB.
  await context.setOffline(true);
  const offline = await value(async (url) => (await __tachyonTac.tac.fetch(url, { credentials: 'omit' }, { cache: 'network-first' })).json());
  expect(offline.value, afterInvalidate.value, 'network-first fell back to IndexedDB offline');

  const refused = await value(async (url) => {
    try { await __tachyonTac.tac.fetch(url, { credentials: 'omit' }, { cache: 'no-store' }); return 'resolved'; }
    catch { return 'threw'; }
  });
  expect(refused, 'threw', 'no-store never answers from storage');
  const offlineRange = await page.evaluate(async (origin) => {
    try {
      await __tachyonTac.tac.fetch(`${origin}/api/range?full-first`, {
        credentials: 'omit', headers: { Range: 'bytes=0-2' },
      });
      return 'resolved';
    } catch { return 'threw'; }
  }, origin);
  expect(offlineRange, 'threw', 'offline Range never falls back to a complete cached response');
  await context.setOffline(false);

  const blocked = await browser.newContext();
  await blocked.addInitScript(() => {
    for (const name of ['localStorage', 'sessionStorage', 'indexedDB', 'caches']) {
      Object.defineProperty(globalThis, name, { get() { throw new DOMException('Denied', 'SecurityError'); } });
    }
  });
  const deniedPage = await blocked.newPage();
  await deniedPage.goto(origin, { waitUntil: 'networkidle' });
  await deniedPage.click('#bump');
  expect(await deniedPage.textContent('#clicks'), '1', 'denied Web Storage does not break rendering');
  const deniedFetch = await deniedPage.evaluate(async (url) => (await __tachyonTac.tac.fetch(url, { credentials: 'omit' })).status, api);
  expect(deniedFetch, 200, 'denied IndexedDB degrades to successful network response');
  await blocked.close();
await browser.close();
browser = undefined;
await closeServer(server);
server = undefined;

// ── Phase 3: CacheStorage precache, over HTTPS so the worker registers ────
const key = path.join(PROJECT, 'ty-storage-key.pem');
const certificate = path.join(PROJECT, 'ty-storage-cert.pem');
const issued = spawnSync('openssl', [
  'req', '-x509', '-newkey', 'rsa:2048', '-nodes',
  '-keyout', key, '-out', certificate, '-days', '1',
  '-subj', `/CN=${HOST}`, '-addext', `subjectAltName=DNS:${HOST}`,
], { encoding: 'utf8', timeout: 30_000, killSignal: 'SIGKILL', maxBuffer: 1024 * 1024 });
if (issued.status !== 0) {
  throw new Error(`cannot issue a test certificate: ${issued.error?.message || issued.stderr || issued.stdout}`);
}

secure = createSecureServer({ key: readFileSync(key), cert: readFileSync(certificate) }, handle);
await listen(secure);
const secureOrigin = `https://${HOST}:${secure.address().port}`;
const profile = path.join(PROJECT, 'ty-storage-profile');
secureContext = await chromium.launchPersistentContext(profile, {
  timeout: 30_000,
  ignoreHTTPSErrors: true,
  args: [`--host-resolver-rules=MAP ${HOST} 127.0.0.1`, '--ignore-certificate-errors'],
});
secureContext.setDefaultTimeout(15_000);
secureContext.setDefaultNavigationTimeout(15_000);

  const securePage = await secureContext.newPage();
  await securePage.goto(`${secureOrigin}/`, { waitUntil: 'networkidle' });
  await securePage.evaluate(async () => {
    let timer;
    try {
      await Promise.race([
        navigator.serviceWorker.ready,
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Service worker readiness timed out')), 15_000); }),
      ]);
    } finally { clearTimeout(timer); }
  });
  // The worker only caches what the page fetched, so the reload is what gives
  // it a populated cache to warm from.
  await securePage.reload({ waitUntil: 'networkidle' });

  const detail = '/.tachyon/components/panel-detail.js';
  expect(await securePage.textContent('#clicks'), '0', 'fresh profile starts at the default');
  expect(await securePage.locator('#detail').count(), 0, 'branch is untaken, so the module is unused');

  const cached = await securePage.waitForFunction(async (asset) => {
    const names = await caches.keys();
    const name = names.find((value) => value.startsWith('tachyon-static-'));
    if (!name) return false;
    const cache = await caches.open(name);
    return Boolean(await cache.match(new URL(asset, location.href).href));
  }, detail).then(() => true).catch(() => false);
  expect(cached, true, 'unused component module precached at idle');

  // The assertion that matters: the branch opens with the network gone, and
  // the component that was never fetched still renders.
  await secureContext.setOffline(true);
  await securePage.click('#bump');
  await securePage.click('#bump');
  await securePage.click('#bump');
  expect(await securePage.textContent('#detail'), 'detail', 'precached component rendered offline');
} finally {
  try {
    const cleanup = await Promise.allSettled([
      browser?.close(), secureContext?.close(), closeServer(server), closeServer(secure),
    ]);
    for (const result of cleanup) {
      if (result.status === 'rejected') { console.error('storage cleanup failed:', result.reason); process.exitCode = 1; }
    }
  } finally {
    if (PROJECT) rmSync(PROJECT, { recursive: true, force: true });
  }
}

if (process.exitCode) console.error('storage gate failed');
else console.log('storage gate passed');
