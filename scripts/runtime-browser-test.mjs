#!/usr/bin/env node
// Execute the emitted runtime from TAC_BIN, including published release binaries.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createReadStream, existsSync, mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
let project;
let server;
let browser;
const binary = process.env.TAC_BIN ?? path.join(repo, 'target/release/ty');
const write = (name, source) => {
  const file = path.join(project, name);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, source);
};
const closeServer = async () => {
  if (!server) return;
  const closed = new Promise((resolve, reject) => server.close((error) => {
    if (error && error.code !== 'ERR_SERVER_NOT_RUNNING') reject(error);
    else resolve();
  }));
  server.closeAllConnections();
  await closed;
};

try {
project = mkdtempSync(path.join(tmpdir(), 'ty-runtime-gate-'));
const iconSource = '/shared/icon" data-manifest-probe="&marker.png';
const iconSizes = '32x32"><script type="module" src="/shared/manifest-injection.js"></script><link sizes="32x32';
write('manifest.json', JSON.stringify({ icons: [{ src: iconSource, sizes: iconSizes }] }));
write('client/shared/manifest-injection.js', 'globalThis.manifestInjectionExecuted = true;');
write('client/pages/tac.html', `<main><p id="count">{$count}</p><p id="seen">{seen}</p>
<button id="increment" on:click="increment()">Increment</button>
<button id="hide" on:click="visible = false">Hide</button>
<logic :if="visible"><signal-panel hydrate="load" /></logic></main>`);
write('client/pages/tac.js', `export default class {
  @publish('counter')
  $count = 2;
  @subscribe('counter')
  seen = 0;
  visible = true;
  increment() { this.$count += 1; }
  @publish('returned')
  async answer() { return 42; }
  @publish('rejected')
  async reject() { throw new Error('private text'); }
}`);
write('client/components/signal/panel/tac.html', '<p id="panel">{seen}</p>');
write('client/components/signal/panel/tac.js', `export default class {
  @subscribe('counter')
  seen = 0;
  @subscribe('method')
  receive(value) { document.documentElement.dataset.delivery = String(value); }
}`);
write('client/pages/native/tac.html', '<main><p id="native-value">{value}</p><input aria-label="Native name" :value="value" on:input="value = $event.target.value"><button id="native-append" on:click="value += &quot;!&quot;">Append</button><button id="native-argument" on:click="act(7)">Call</button><input id="native-explicit-event" on:input="act($event.target.value)"></main>');
write('client/pages/native/tac.js', "export default class { value = 'browser'; act() { throw new Error('native transport was not selected'); } }");
write('client/pages/loops/tac.html', '\ufeff' + `<main>
<section id="ascending"><loop :for="let i = start; i < limit; i += step"><span>{i}</span></loop></section>
<section id="descending"><loop :for="let i = 4; i >= 0; i -= 2"><span>{i}</span></loop></section>
<section id="inclusive"><loop :for="let i = 0; i <= 2; i++"><span>{i}</span></loop></section>
<section id="nested"><loop :for="group of groups"><loop :for="let i = 0; i < group.length; i++"><span>{group.length}:{i}:{$index}</span></loop></loop></section>
<section id="empty-nested"><loop :for="let i = 0; i < emptyLimit; i++"><loop :for="let j = 0; j < emptyLimit; j++"></loop></loop></section>
<p id="entities" title="&amp;lt;">A&nbsp;&amp;&lt;tag&gt;&amp;amp;</p>
</main>`);
write('client/pages/loops/tac.js', 'export default class { start = 0; limit = 3; step = 1; emptyLimit = 0; groups = [[1, 2], [3]]; }');
write('client/pages/mounts/tac.html', '<main><logic :if="show"><delayed-idle hydrate="idle"/><delayed-click hydrate="interaction"/><div id="spacer"></div><delayed-visible hydrate="visible"/></logic></main>');
write('client/pages/mounts/tac.css', '#spacer { height: 5000px; }');
write('client/pages/mounts/tac.js', 'export default class { show = true; }');
write('client/pages/focus-boundaries/tac.html', `<main><p id="boundary-status">{status}</p>
<section id="rows"><loop :for="row of rows"><input aria-label="Row" :value="row.label" on:input="edit(row.id, $event.target.value)"></loop></section>
<unknown-wrap><input aria-label="Wrapped" :value="text" on:input="text = $event.target.value"></unknown-wrap>
<observed-wrap :mode="mode"><input aria-label="Observed" :value="text" on:input="text = $event.target.value"></observed-wrap>
</main>`);
write('client/pages/focus-boundaries/tac.js', `export default class {
  rows = [{ id: 1, label: 'First' }]; text = 'abcdef'; mode = 'before'; status = 'before';
  edit(_event, id, value) { this.rows.find((row) => row.id === id).label = value; }
}`);
for (const name of ['idle', 'click', 'visible']) {
  write(`client/components/delayed/${name}/tac.html`, `<button>${name}</button>`);
  write(`client/components/delayed/${name}/tac.js`, `export default class { mount() { globalThis.delayedActivations.push('${name}'); } }`);
}
write('client/pages/focus/tac.html', `<main><h1>Focus regression</h1>
<label>Customer name<input aria-label="Customer name" :value="customer" on:input="customer = $event.target.value"></label>
<output id="customer">{customer}</output>
<input id="identified" aria-label="Identified input" :value="identified" on:input="identified = $event.target.value">
<textarea aria-label="Notes" on:input="notes = $event.target.value">{notes}</textarea>
<logic :if="visible"><focus-shell><focus-editor hydrate="load" /></focus-shell></logic>
<button on:click="visible = false">Remove editor</button></main>`);
write('client/pages/focus/tac.js', `customElements.define('focus-shell', class extends HTMLElement {
  connectedCallback() {
    if (this.querySelector('slot')) return;
    const slot = document.createElement('slot');
    slot.append(...this.childNodes);
    const wrapper = document.createElement('header');
    wrapper.append(slot);
    this.append(wrapper);
  }
});
export default class { customer = ''; identified = ''; notes = ''; visible = true; }`);
write('client/components/focus/editor/tac.html', '<section><label>Component input<input aria-label="Component input" :value="value" on:input="value = $event.target.value"></label><output id="component-value">{value}</output></section>');
write('client/components/focus/editor/tac.js', 'export default class { value = ""; }');
const build = spawnSync(binary, ['build', project], {
  encoding: 'utf8', timeout: 120_000, killSignal: 'SIGKILL', maxBuffer: 1024 * 1024,
});
assert.ifError(build.error);
assert.equal(build.status, 0, build.stderr || build.stdout);

const output = path.join(project, 'dist');
server = createServer((request, response) => {
  const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
  let file = path.resolve(output, `.${pathname}`);
  if (!file.startsWith(`${output}${path.sep}`) && file !== output) { response.writeHead(403).end(); return; }
  if (existsSync(file) && statSync(file).isDirectory()) file = path.join(file, 'index.html');
  if (!existsSync(file)) { response.writeHead(404).end(); return; }
  response.setHeader('content-type', path.extname(file) === '.js' ? 'text/javascript' : path.extname(file) === '.css' ? 'text/css' : 'text/html');
  response.setHeader('content-security-policy', "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'");
  createReadStream(file).pipe(response);
});
await new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', () => { server.off('error', reject); resolve(); });
});
const origin = `http://127.0.0.1:${server.address().port}`;
browser = await chromium.launch({ timeout: 30_000 });
  const page = await browser.newPage();
  page.setDefaultTimeout(10000);
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto(origin);
  await page.waitForSelector('#panel');
  const manifestHead = await page.evaluate(() => ({
    executed: globalThis.manifestInjectionExecuted === true,
    injectedScripts: document.querySelectorAll('script[src="/shared/manifest-injection.js"]').length,
    injectedAttributes: document.querySelectorAll('[data-manifest-probe]').length,
    icons: document.querySelectorAll('link[rel="icon"]').length,
    href: document.querySelector('link[rel="icon"]')?.getAttribute('href'),
    sizes: document.querySelector('link[rel="icon"]')?.getAttribute('sizes'),
    appleHref: document.querySelector('link[rel="apple-touch-icon"]')?.getAttribute('href'),
  }));
  assert.deepEqual(manifestHead, { executed: false, injectedScripts: 0, injectedAttributes: 0,
    icons: 1, href: iconSource, sizes: iconSizes, appleHref: iconSource },
  'manifest values stay literal attributes and cannot inject executable same-origin markup');
  assert.equal(await page.textContent('#seen'), '2', 'field subscriber receives retained default');
  assert.equal(await page.textContent('#panel'), '2', 'component decorator lowered without inline state');
  await page.click('#increment');
  await page.waitForFunction(() => document.querySelector('#panel')?.textContent === '3');
  await page.reload();
  await page.waitForFunction(() => document.querySelector('#seen')?.textContent === '3');
  assert.equal(await page.textContent('#count'), '3', 'publish decorator preserves persistent field descriptor');
  const result = await page.evaluate(async () => {
    const { instance, tac } = __tachyonTac;
    const answer = await instance.answer();
    try { await instance.reject(); } catch {}
    let delivered = 0;
    const stop1 = tac.subscribe('failure', () => { throw new Error('private synchronous'); });
    const stop2 = tac.subscribe('failure', async () => { throw new Error('private asynchronous'); });
    const stop3 = tac.subscribe('failure', () => { delivered += 1; });
    tac.publish('failure', null);
    stop1(); stop2(); stop3();
    const controller = new AbortController();
    tac.subscribe('aborted', () => { delivered += 100; }, { signal: controller.signal });
    controller.abort();
    tac.publish('aborted', null);
    const sample = { nested: 1 };
    tac.publish('snapshot', sample);
    sample.nested = 2;
    const retained = tac.retained('snapshot');
    retained.nested = 3;
    return { answer, published: tac.retained('returned'), rejected: tac.retained('rejected'), delivered,
      snapshot: tac.retained('snapshot').nested, hidden: !Object.keys(instance).includes('tac'),
      cloned: structuredClone({ ...instance }).$count };
  });
  assert.deepEqual(result, { answer: 42, published: 42, rejected: undefined, delivered: 1, snapshot: 1, hidden: true, cloned: 3 });
  await page.evaluate(() => __tachyonTac.tac.publish('method', 'before'));
  assert.equal(await page.getAttribute('html', 'data-delivery'), 'before');
  await page.evaluate(() => __tachyonTac.hotUpdate(['signal-panel'], 'runtime-test'));
  await page.evaluate(() => __tachyonTac.tac.publish('method', 'after-hot'));
  assert.equal(await page.getAttribute('html', 'data-delivery'), 'after-hot');
  await page.click('#hide');
  await page.waitForFunction(() => !document.querySelector('#panel'));
  await page.evaluate(() => __tachyonTac.tac.publish('method', 'removed'));
  assert.equal(await page.getAttribute('html', 'data-delivery'), 'after-hot', 'removed subscriber disposed');
  assert.deepEqual(errors, [], 'no uncaught runtime errors or CSP violations');

  const focus = await browser.newPage();
  focus.setDefaultTimeout(10000);
  focus.on('pageerror', (error) => errors.push(error.message));
  await focus.goto(`${origin}/focus/`);
  const customer = focus.getByRole('textbox', { name: 'Customer name', exact: true });
  await customer.click();
  await focus.evaluate(() => { globalThis.originalInput = document.activeElement; });
  await customer.pressSequentially('Customer', { delay: 35 });
  assert.equal(await customer.inputValue(), 'Customer', 'typing must not drop characters from an input without an ID');
  await focus.waitForFunction(() => document.querySelector('#customer')?.textContent === 'Customer');
  assert.equal(await focus.evaluate(() => document.activeElement === originalInput && originalInput.isConnected), true,
    'reactive updates retain the focused DOM node, not only its ID');
  await customer.press('Home');
  await customer.press('ArrowRight');
  await customer.pressSequentially('X', { delay: 35 });
  assert.equal(await customer.inputValue(), 'CXustomer', 'middle insertion preserves caret');
  await focus.waitForFunction(() => document.querySelector('#customer')?.textContent === 'CXustomer');
  assert.equal(await customer.evaluate((input) => input.selectionStart), 2);
  const identified = focus.getByRole('textbox', { name: 'Identified input', exact: true });
  await identified.fill('abcdef');
  await identified.evaluate((input) => input.setSelectionRange(1, 4, 'backward'));
  await focus.evaluate(() => __tachyonTac.render());
  assert.deepEqual(await identified.evaluate((input) => [input.selectionStart, input.selectionEnd, input.selectionDirection]), [1, 4, 'backward']);
  const notes = focus.getByRole('textbox', { name: 'Notes', exact: true });
  await notes.click();
  await notes.pressSequentially('line one', { delay: 35 });
  await notes.press('Enter');
  await notes.pressSequentially('line two', { delay: 35 });
  assert.equal(await notes.inputValue(), 'line one\nline two', 'textarea typing and line breaks retain focus');
  const componentInput = focus.getByRole('textbox', { name: 'Component input', exact: true });
  await componentInput.click();
  await focus.evaluate(() => { globalThis.originalComponentInput = document.activeElement; });
  await componentInput.pressSequentially('component', { delay: 35 });
  await focus.waitForFunction(() => document.querySelector('#component-value')?.textContent === 'component');
  assert.equal(await focus.evaluate(() => document.activeElement === originalComponentInput && originalComponentInput.isConnected), true,
    'focused descendants remain connected through component host updates');
  await componentInput.dispatchEvent('compositionstart', { data: '' });
  await focus.evaluate(() => __tachyonTac.render());
  assert.equal(await focus.evaluate(() => document.activeElement === originalComponentInput), true, 'composition does not lose the composing node');
  await componentInput.dispatchEvent('compositionend', { data: '' });
  await focus.evaluate(async () => { __tachyonTac.instance.visible = false; await __tachyonTac.render(); });
  assert.equal(await componentInput.count(), 0, 'removing a focused control still removes its component');
  assert.equal(await focus.evaluate(() => originalComponentInput.isConnected), false);
  assert.deepEqual(errors, [], 'focus updates do not introduce uncaught errors');

  const boundaries = await browser.newPage();
  await boundaries.addInitScript(() => {
    globalThis.observedLiveMutations = 0;
    customElements.define('unknown-wrap', class extends HTMLElement {
      connectedCallback() {
        if (this.querySelector('[data-vendor]')) return;
        const wrapper = document.createElement('div');
        wrapper.dataset.vendor = 'unknown';
        wrapper.append(...this.childNodes);
        this.append(wrapper);
      }
    });
    customElements.define('observed-wrap', class extends HTMLElement {
      static observedAttributes = ['mode'];
      attributeChangedCallback() {
        if (!this.isConnected) return;
        observedLiveMutations += 1;
        this.replaceChildren(document.createTextNode('vendor replaced editing subtree'));
      }
    });
  });
  await boundaries.goto(`${origin}/focus-boundaries/`);
  const row = boundaries.getByRole('textbox', { name: 'Row', exact: true });
  await row.click();
  const replacedRow = await boundaries.evaluate(async () => {
    const old = document.activeElement;
    __tachyonTac.instance.rows = [{ id: 2, label: 'Replacement' }];
    await __tachyonTac.render();
    const current = document.querySelector('#rows input');
    return { oldConnected: old.isConnected, focusTransferred: document.activeElement === current, value: current.value };
  });
  assert.deepEqual(replacedRow, { oldConnected: false, focusTransferred: false, value: 'Replacement' },
    'replacing an unkeyed lexical row must not transfer its editing focus to another record');
  const wrapped = boundaries.getByRole('textbox', { name: 'Wrapped', exact: true });
  await wrapped.click();
  await wrapped.evaluate((input) => input.setSelectionRange(1, 3, 'backward'));
  const wrapperFallback = await boundaries.evaluate(async () => {
    const old = document.activeElement;
    __tachyonTac.instance.status = 'after-wrapper';
    await __tachyonTac.render();
    const current = document.querySelector('[aria-label="Wrapped"]');
    return { replaced: old !== current && !old.isConnected, focused: document.activeElement === current,
      selection: [current.selectionStart, current.selectionEnd, current.selectionDirection],
      status: document.querySelector('#boundary-status').textContent };
  });
  assert.deepEqual(wrapperFallback, { replaced: true, focused: true, selection: [1, 3, 'backward'], status: 'after-wrapper' },
    'an unknown wrapper falls back to a complete update without a partial commit');
  const observed = boundaries.getByRole('textbox', { name: 'Observed', exact: true });
  await observed.click();
  await observed.evaluate((input) => input.setSelectionRange(2, 2));
  const attributeFallback = await boundaries.evaluate(async () => {
    const old = document.activeElement;
    __tachyonTac.instance.mode = 'after';
    __tachyonTac.instance.status = 'after-observed';
    await __tachyonTac.render();
    const current = document.querySelector('[aria-label="Observed"]');
    return { replaced: old !== current && !old.isConnected, focused: document.activeElement === current,
      caret: current.selectionStart, liveMutations: observedLiveMutations,
      mode: document.querySelector('observed-wrap').getAttribute('mode'),
      status: document.querySelector('#boundary-status').textContent };
  });
  assert.deepEqual(attributeFallback, { replaced: true, focused: true, caret: 2, liveMutations: 0, mode: 'after', status: 'after-observed' },
    'observed-attribute fallback is decided before touching the connected custom element');

  const mounts = await browser.newPage();
  await mounts.addInitScript(() => {
    globalThis.delayedActivations = [];
    globalThis.cancelledIdle = [];
    globalThis.queuedIdle = new Map();
    globalThis.idleSequence = 0;
    // Hold browser scheduling at this external boundary to deterministically
    // remove the component before an already-queued callback is delivered.
    globalThis.requestIdleCallback = (callback) => { const id = ++idleSequence; queuedIdle.set(id, callback); return id; };
    globalThis.cancelIdleCallback = (id) => { cancelledIdle.push(id); queuedIdle.delete(id); };
    const Observer = globalThis.IntersectionObserver;
    globalThis.disconnectedObservers = 0;
    globalThis.IntersectionObserver = class extends Observer {
      disconnect() { disconnectedObservers += 1; super.disconnect(); }
    };
  });
  await mounts.goto(`${origin}/mounts/`);
  await mounts.waitForSelector('delayed-click, tachyon-component[data-tachyon-component="delayed-click"]');
  const removedMounts = await mounts.evaluate(async () => {
    const before = [...queuedIdle.values()];
    const interaction = document.querySelector('[data-tachyon-component="delayed-click"] button');
    __tachyonTac.instance.show = false;
    await __tachyonTac.render();
    // Deliver a stale callback and a stale detached event as an adversarial
    // scheduler could. Neither may activate a disposed owner.
    for (const callback of before) await callback({ didTimeout: false, timeRemaining: () => 50 });
    interaction.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true }));
    await Promise.resolve();
    return { activated: delayedActivations, idleCancelled: cancelledIdle.length > 0,
      visibleDisconnected: disconnectedObservers > 0, hosts: document.querySelectorAll('tachyon-component').length };
  });
  assert.deepEqual(removedMounts, { activated: [], idleCancelled: true, visibleDisconnected: true, hosts: 0 },
    'removed component schedules are cancelled and stale activation is rejected');

  const loops = await browser.newPage();
  await loops.goto(`${origin}/loops/`);
  await loops.waitForSelector('#entities');
  const values = async (selector) => loops.locator(`${selector} span`).allTextContents();
  assert.deepEqual(await values('#ascending'), ['0', '1', '2']);
  assert.deepEqual(await values('#descending'), ['4', '2', '0']);
  assert.deepEqual(await values('#inclusive'), ['0', '1', '2']);
  assert.deepEqual(await values('#nested'), ['2:0:0', '2:1:1', '1:0:0']);
  assert.equal(await loops.textContent('#entities'), 'A\u00a0&<tag>&amp;', 'BOM stripped and text entities decoded once');
  assert.equal(await loops.getAttribute('#entities', 'title'), '&lt;', 'attribute entities decoded once');
  await loops.evaluate(async () => { __tachyonTac.instance.limit = 5; await __tachyonTac.render(); });
  assert.deepEqual(await values('#ascending'), ['0', '1', '2', '3', '4'], 'counted bound rerenders');
  for (const step of [0, -1, Infinity, NaN]) {
    await loops.evaluate(async (value) => { __tachyonTac.instance.step = value; await __tachyonTac.render(); }, step);
    assert.deepEqual(await values('#ascending'), [], `invalid dynamic step ${step} terminates`);
  }
  const bounded = await loops.evaluate(async () => {
    __tachyonTac.instance.step = 1;
    __tachyonTac.instance.limit = 10001;
    let timer;
    try {
      return await Promise.race([
        __tachyonTac.render().then(() => 'unexpected-success', (error) => error instanceof RangeError ? 'bounded-error' : 'wrong-error'),
        new Promise((resolve) => { timer = setTimeout(() => resolve('timeout'), 5000); }),
      ]);
    } finally { clearTimeout(timer); }
  });
  assert.equal(bounded, 'bounded-error', 'iteration cap fails rather than hanging the renderer');
  assert.equal(await loops.getAttribute('html', 'data-tachyon-render-error'), 'render_failed');
  await loops.evaluate(async () => { __tachyonTac.instance.limit = 3; await __tachyonTac.render(); });
  assert.deepEqual(await values('#ascending'), ['0', '1', '2'], 'renderer recovers after bounded failure');
  assert.equal(await loops.getAttribute('html', 'data-tachyon-render-error'), null);

  const nestedBounded = await loops.evaluate(async () => {
    __tachyonTac.instance.emptyLimit = 10000;
    let timer;
    try {
      return await Promise.race([
        __tachyonTac.render().then(() => 'unexpected-success', (error) => error instanceof RangeError ? 'bounded-error' : 'wrong-error'),
        new Promise((resolve) => { timer = setTimeout(() => resolve('timeout'), 5000); }),
      ]);
    } finally { clearTimeout(timer); }
  });
  assert.equal(nestedBounded, 'bounded-error', 'aggregate iteration budget bounds nested empty loops as well as emitted nodes');
  await loops.evaluate(async () => { __tachyonTac.instance.emptyLimit = 0; await __tachyonTac.render(); });

  // Mock only the real external native-host transport, never renderer internals.
  const native = await browser.newPage();
  await native.addInitScript(() => {
    globalThis.nativeCalls = [];
    let stored = 'native-ready';
    globalThis.__tachyonCompanionQueue = [{ name: 'early', value: 'queued' }];
    globalThis.__tachyonNativeHostCall = async (_capability, payload) => {
      const request = JSON.parse(payload);
      nativeCalls.push(request);
      if (request.op === 'set') stored = request.value;
      const value = request.op === 'init' ? { fields: ['value', '__proto__'], methods: ['act'] }
        : request.op === 'get' ? stored : request.args?.[0];
      return JSON.stringify({ value });
    };
  });
  await native.goto(`${origin}/native/`);
  await native.waitForSelector('#native-value');
  assert.equal(await native.textContent('#native-value'), 'native-ready');
  const bridge = await native.evaluate(async () => {
    const atStart = nativeCalls.filter((call) => call.op === 'call').length;
    const answer = await __tachyonTac.instance.act('called');
    __tachyonTac.instance.value = 'changed';
    await __tachyonTac.render();
    return { atStart, answer, queued: __tachyonTac.tac.retained('early'),
      routes: nativeCalls.every((call) => call.route === '/native'),
      reserved: nativeCalls.some((call) => call.name === '__proto__'),
      set: nativeCalls.some((call) => call.op === 'set' && call.value === 'changed'),
      rendered: document.querySelector('#native-value').textContent };
  });
  assert.deepEqual(bridge, { atStart: 0, answer: 'called', queued: 'queued', routes: true, reserved: false, set: true, rendered: 'changed' });
  await native.click('#native-argument');
  await native.waitForFunction(() => nativeCalls.some((call) => call.op === 'call' && call.args?.[0] === 7));
  await native.fill('#native-explicit-event', 'authored');
  await native.waitForFunction(() => nativeCalls.some((call) => call.op === 'call' && call.args?.[0] === 'authored'));
  assert.deepEqual(await native.evaluate(() => nativeCalls.filter((call) => call.op === 'call').map((call) => call.args)),
    [['called'], [7], ['authored']], 'native events pass only authored arguments; direct calls are unchanged');

  // A native transport may acknowledge an older edit after a newer keypress.
  // Hold replies at that external boundary to reproduce the ordering exactly.
  const delayedNative = await browser.newPage();
  await delayedNative.addInitScript(() => {
    let stored = '';
    globalThis.delayedHost = {
      hold: false, pending: [],
      releaseOne() { this.pending.shift()?.resolve(); },
      releaseAll() { this.hold = false; while (this.pending.length) this.releaseOne(); },
    };
    globalThis.__tachyonNativeHostCall = async (_capability, payload) => {
      const request = JSON.parse(payload);
      if (request.op === 'set') stored = request.value;
      const value = request.op === 'init' ? { fields: ['value'], methods: ['act'] }
        : request.op === 'get' ? stored : request.args?.[0];
      const reply = JSON.stringify({ value });
      if (request.op === 'get' && delayedHost.hold) {
        return new Promise(resolve => delayedHost.pending.push({ value, resolve: () => resolve(reply) }));
      }
      return reply;
    };
  });
  await delayedNative.goto(`${origin}/native/`);
  const nativeName = delayedNative.getByRole('textbox', { name: 'Native name', exact: true });
  await nativeName.click();
  await delayedNative.evaluate(() => { globalThis.originalNativeInput = document.activeElement; delayedHost.hold = true; });
  await nativeName.pressSequentially('A');
  await delayedNative.waitForFunction(() => delayedHost.pending[0]?.value === 'A');
  await nativeName.pressSequentially('d');
  await delayedNative.evaluate(() => delayedHost.releaseOne());
  await delayedNative.waitForFunction(() => delayedHost.pending[0]?.value === 'Ad');
  await delayedNative.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  assert.equal(await nativeName.inputValue(), 'Ad', 'an older native acknowledgement cannot erase a newer edit');
  assert.equal(await delayedNative.evaluate(() => document.activeElement === originalNativeInput && originalNativeInput.isConnected), true);
  await nativeName.pressSequentially('a');
  await delayedNative.evaluate(() => delayedHost.releaseAll());
  await delayedNative.waitForFunction(() => document.querySelector('#native-value')?.textContent === 'Ada');
  assert.equal(await nativeName.inputValue(), 'Ada');

  await delayedNative.evaluate(() => { delayedHost.hold = true; void __tachyonTac.instance.act('refresh'); });
  await delayedNative.waitForFunction(() => delayedHost.pending[0]?.value === 'Ada');
  await nativeName.pressSequentially('X');
  await delayedNative.waitForFunction(() => delayedHost.pending.length === 2);
  await delayedNative.evaluate(() => delayedHost.releaseOne());
  await delayedNative.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  assert.equal(await nativeName.inputValue(), 'AdaX', 'method refresh cannot overwrite an in-flight native edit');
  await delayedNative.click('#native-append');
  await delayedNative.evaluate(() => delayedHost.releaseAll());
  await delayedNative.waitForFunction(() => document.querySelector('#native-value')?.textContent === 'AdaX!');
  assert.equal(await nativeName.inputValue(), 'AdaX!', 'a subsequent user action sees the newest native field edit');
  console.log('Tac runtime browser gate passed: decorators, persistence composition, signals, HMR disposal, counted loops/bounds, BOM/entities, native transport, strict CSP.');
} finally {
  try {
    const cleanup = await Promise.allSettled([browser?.close(), closeServer()]);
    for (const result of cleanup) {
      if (result.status === 'rejected') { console.error('runtime cleanup failed:', result.reason); process.exitCode = 1; }
    }
  } finally {
    if (project) rmSync(project, { recursive: true, force: true });
  }
}
