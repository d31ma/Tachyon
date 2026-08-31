#!/usr/bin/env node
// Real Android WebView + Kotlin qualification. A debuggable APK permits CDP;
// assertions execute in Android's own JS engine and native process, not Node.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import './bridge-unit-test.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, { cwd: repo, encoding: 'utf8', timeout: 240000, maxBuffer: 2 * 1024 * 1024, ...options });
  assert.equal(result.status, 0, `${command} ${args.join(' ')}: ${result.error || result.stderr || result.stdout}`);
  return result.stdout.trim();
};
const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;
assert.ok(sdk, 'ANDROID_HOME or ANDROID_SDK_ROOT is required');
const adb = path.join(sdk, 'platform-tools', process.platform === 'win32' ? 'adb.exe' : 'adb');
const project = mkdtempSync(path.join(tmpdir(), 'tachyon-android-gate-'));
const packageName = 'dev.tachyon.phase_five';
const evidence = path.join(repo, 'target/phase5-android-evidence');
const write = (name, contents) => {
  const file = path.join(project, name);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, contents);
};
const pause = (ms) => new Promise(resolve => setTimeout(resolve, ms));
let browser;
let port;
try {
  write('tac.config.js', "export const application = {name:'Phase Five',id:'dev.tachyon.phase-five',version:'0.1.0',entryRoute:'/'};\n");
  write('client/shared/identity.txt', 'root-relative-native-asset');
  write('client/shared/bridge-probe.html', '<!doctype html><title>Native frame boundary probe</title><p>Frame content remains supported.</p>');
  for (const prefix of ['', 'items/_id/']) {
    write(`client/pages/${prefix}tac.html`, `<main aria-label="Android native evidence"><h1>${prefix ? 'Second route' : 'First route'}</h1><p>Built by {builtBy}</p><button on:click="increase()" aria-label="Increase count">Increase</button><output aria-label="Count">{count}</output><input aria-label="Your name" :value="name" on:input="name = $event.target.value" placeholder="Name"><a href="/items/7/">Second route</a></main>`);
    write(`client/pages/${prefix}tac.css`, 'h1 { color: rgb(12, 34, 56); }');
    write(`client/pages/${prefix}tac.kt`, `class Companion {
  var count: Int = 0
  var name: String = ""
    set(value) { Thread.sleep(100); field = value }
  val builtBy: String = "Kotlin"
  fun increase(): Int { count += 21; return count }
  fun processId(): Int = android.os.Process.myPid()
  fun delayedProcessId(): Int { Thread.sleep(1200); return android.os.Process.myPid() }
  fun announce() { tacPublish("native.event", "Kotlin") }
}
`);
  }
  let binary = process.env.TAC_BIN;
  if (!binary) {
    run('cargo', ['build', '--locked', '--bin', 'ty']);
    binary = path.join(JSON.parse(run('cargo', ['metadata', '--format-version', '1', '--no-deps'])).target_directory, 'debug/ty');
  }
  run(binary, ['build', project, '--target', 'android']);
  const out = path.join(project, 'dist/android');
  const host = JSON.parse(readFileSync(path.join(out, 'tachyon.host.json'), 'utf8'));
  assert.equal(host.schemaVersion, 3);
  assert.deepEqual(host.companions, [{ route: '/', language: 'Kotlin' }, { route: '/items/_id', language: 'Kotlin' }]);
  const assets = path.join(out, 'PhaseFive/project/app/src/main/assets');
  assert.equal(JSON.parse(readFileSync(path.join(assets, 'NativeIndex.json'), 'utf8')).contract_version, 2);
  assert.deepEqual(readFileSync(path.join(assets, 'WebBundle/index.html')), readFileSync(path.join(out, 'web/index.html')));
  run(adb, ['wait-for-device']);
  const apkEntries = run('unzip', ['-Z1', path.join(out, 'PhaseFive/PhaseFive.apk')]);
  assert.ok(apkEntries.split('\n').includes('assets/WebBundle/items/_id/index.html'), 'AAPT must retain dynamic route directories');
  run(adb, ['install', '-r', path.join(out, 'PhaseFive/PhaseFive.apk')]);
  run(adb, ['shell', 'am', 'force-stop', packageName]);
  run(adb, ['shell', 'am', 'start', '-W', '-n', `${packageName}/.MainActivity`]);
  let pid;
  for (let attempt = 0; attempt < 60; attempt++) {
    pid = run(adb, ['shell', 'pidof', packageName]);
    if (run(adb, ['shell', 'cat', '/proc/net/unix']).includes(`webview_devtools_remote_${pid}`)) break;
    await pause(500);
  }
  assert.match(pid, /^\d+$/);
  port = run(adb, ['forward', 'tcp:0', `localabstract:webview_devtools_remote_${pid}`]);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, {noDefaults:true});
  const context = browser.contexts()[0];
  const page = context.pages()[0];
  page.on('pageerror', error => console.error('Android page error:', error.message));
  page.on('console', message => { if (message.type() === 'error') console.error('Android console:', message.text()); });
  assert.ok(page, 'Android WebView exposes a live document');
  try { await page.getByRole('button', {name:'Increase count'}).waitFor(); }
  catch (error) { console.error('Android startup document:', page.url(), (await page.content()).slice(0,4096)); throw error; }
  assert.equal(await page.evaluate(async () => (await navigator.serviceWorker.getRegistrations()).length), 0, 'Native UI must not use a stale service worker');
  const count = () => page.getByRole('status', {name:'Count'});
  assert.equal(await count().textContent(), '0');
  await page.getByRole('button', {name:'Increase count'}).click();
  await page.waitForFunction(() => document.querySelector('output')?.textContent === '21');
  const nativePid = await page.evaluate(async () => JSON.parse(await globalThis.__tachyonNativeHostCall('companion.invoke', JSON.stringify({ route:'/', op:'call', name:'processId', args:[] }))).value);
  assert.equal(nativePid, Number(pid), 'Kotlin must run inside the actual Android app process');
  const wrongRoute = await page.evaluate(async () => JSON.parse(await globalThis.__tachyonNativeHostCall('companion.invoke', JSON.stringify({ route:'/items/_id', op:'get', name:'count' }))));
  assert.ok(wrongRoute.error, 'A page cannot impersonate another native route');
  // Exercise the actual Android transport from a sandboxed opaque-origin frame,
  // not the friendly page wrapper (which an adversarial frame can bypass).
  const sandboxProbe = await page.evaluate(() => new Promise(resolve => {
    const frame = document.createElement('iframe');
    frame.sandbox = 'allow-scripts';
    const timer = setTimeout(() => {
      removeEventListener('message', receive);
      frame.remove();
      resolve({ error:'Opaque iframe probe timed out' });
    }, 5000);
    const receive = event => {
      if (event.source !== frame.contentWindow || event.data?.probe !== 'android-frame') return;
      clearTimeout(timer);
      removeEventListener('message', receive);
      frame.remove();
      resolve(event.data);
    };
    addEventListener('message', receive);
    frame.srcdoc = `<script>
      const host = globalThis.__tachyonAndroidHost;
      let result = null;
      if (host && typeof host.call === 'function') {
        result = host.call('companion.invoke', JSON.stringify({route:'/',op:'call',name:'processId',args:[]}));
      }
      parent.postMessage({probe:'android-frame', exposed:!!host, result}, '*');
    <\/script>`;
    document.body.append(frame);
  }));
  assert.equal(sandboxProbe.exposed, false, `Opaque srcdoc must not receive native transport: ${JSON.stringify(sandboxProbe)}`);
  for (const [name, url, exposed] of [
    ['local-frame', 'https://appassets.tachyon.local/shared/bridge-probe.html', true],
    ['foreign-frame', 'https://foreign.invalid/bridge-probe.html', false],
  ]) {
    await page.evaluate(({ name, url }) => {
      const frame = document.createElement('iframe');
      frame.name = name;
      frame.src = url;
      document.body.append(frame);
    }, { name, url });
    let frame;
    for (let attempt = 0; attempt < 40; attempt++) {
      frame = page.frames().find(candidate => candidate.name() === name && candidate.url() === url);
      if (frame) break;
      await pause(100);
    }
    assert.ok(frame, `${name} document must load for the actual frame boundary probe`);
    if (name === 'local-frame') assert.match(await frame.locator('body').textContent(), /Frame content remains supported/);
    const result = await frame.evaluate(() => new Promise(resolve => {
      const host = globalThis.__tachyonAndroidHost;
      if (!host) { resolve({ exposed: false, reply: null }); return; }
      const id = '777777';
      const timer = setTimeout(() => resolve({ exposed: true, reply: null }), 750);
      host.onmessage = event => {
        clearTimeout(timer);
        resolve({ exposed: true, reply: event.data });
      };
      host.postMessage(JSON.stringify({
        id, capability: 'companion.invoke',
        payload: JSON.stringify({ route: '/', op: 'call', name: 'processId', args: [] }),
        // Caller-supplied frame/origin claims must never authorize this call.
        isMainFrame: true, sourceOrigin: 'https://appassets.tachyon.local',
      }));
    }));
    assert.equal(result.exposed, exposed, `${name} transport origin allowlist`);
    assert.equal(result.reply, null, `${name} cannot invoke even with spoofed trusted origin/main-frame fields`);
    await page.evaluate(name => document.querySelector(`iframe[name="${name}"]`).remove(), name);
  }
  // Raw main-frame probes bypass the wrapper's own limits and exercise native
  // admission. Following each refusal, a valid native call proves liveness.
  const rawMain = async (extra, payload = JSON.stringify({ route:'/', op:'call', name:'processId', args:[] }), wire = null) =>
    page.evaluate(({ extra, payload, wire }) => new Promise(resolve => {
      const host = globalThis.__tachyonAndroidHost;
      const id = String(800000 + Math.floor(Math.random() * 100000));
      const receive = event => {
        const reply = JSON.parse(event.data);
        if (reply.id !== id) return;
        clearTimeout(timer);
        host.removeEventListener('message', receive);
        resolve(JSON.parse(reply.result));
      };
      const timer = setTimeout(() => {
        host.removeEventListener('message', receive);
        resolve(null);
      }, 750);
      host.addEventListener('message', receive);
      host.postMessage(wire === null ? JSON.stringify({ id, capability:'companion.invoke', payload, ...extra }) : wire.replace('__ID__', id));
    }), { extra, payload, wire });
  assert.equal((await rawMain({})).value, Number(pid), 'Raw valid main-frame call is authorized');
  const nested = depth => '['.repeat(depth) + '0' + ']'.repeat(depth);
  assert.equal((await rawMain({ ignored: JSON.parse(nested(63)) })).value, Number(pid), 'Envelope depth64 is accepted');
  assert.equal(await rawMain({ ignored: JSON.parse(nested(64)) }), null, 'Envelope depth65 is rejected');
  const request = { route:'/', op:'call', name:'processId', args:[] };
  const lenientEnvelope = `{id:"__ID__",capability:"companion.invoke",payload:${JSON.stringify(JSON.stringify(request))},a"b:1,"x":${nested(65)},c"d:2}`;
  assert.equal(await rawMain({}, undefined, lenientEnvelope), null, 'Unquoted Android JSON keys cannot hide depth from the admission scanner');
  assert.equal((await rawMain({}, JSON.stringify({ ...request, ignored:JSON.parse(nested(63)) }))).value, Number(pid), 'Payload depth64 is accepted');
  assert.equal(await rawMain({}, JSON.stringify({ ...request, ignored:JSON.parse(nested(64)) })), null, 'Payload depth65 is rejected');
  assert.equal(await rawMain({}, JSON.stringify({ ...request, ignored:'é'.repeat(32769) })), null, 'Native UTF8 byte limit is enforced');
  assert.equal(await rawMain({ ignored:'é'.repeat(65537) }), null, 'Native outer-envelope UTF8 byte limit is enforced');
  assert.equal((await rawMain({})).value, Number(pid), 'Rejected raw calls do not crash or disable the host');
  await page.evaluate(async () => {
    const receive = globalThis.__tachyonCompanionPublish;
    globalThis.__tachyonCompanionPublish = event => { globalThis.nativeEvidence = event; receive(event); };
    await globalThis.__tachyonNativeHostCall('companion.invoke', JSON.stringify({route:'/',op:'call',name:'announce',args:[]}));
  });
  await page.waitForFunction(() => globalThis.nativeEvidence?.name === 'native.event');
  assert.equal(await page.evaluate(() => globalThis.nativeEvidence.value), 'Kotlin');
  mkdirSync(evidence, {recursive:true});
  await page.screenshot({path:path.join(evidence,'android-first-route.png')});
  await page.addInitScript(() => {
    globalThis.lateNativeReplies = [];
    globalThis.__tachyonAndroidHost?.addEventListener('message', event => {
      if (JSON.parse(event.data).id === '999997') globalThis.lateNativeReplies.push(event.data);
    });
  });
  await page.evaluate(() => globalThis.__tachyonAndroidHost.postMessage(JSON.stringify({
    id:'999997', capability:'companion.invoke',
    payload:JSON.stringify({route:'/', op:'call', name:'delayedProcessId', args:[]}),
  })));
  await pause(100);
  await page.getByRole('link', {name:'Second route'}).click();
  try { await page.getByRole('heading', {name:'Second route'}).waitFor(); }
  catch (error) { console.error('Android dynamic route document:', page.url(), (await page.content()).slice(0,4096)); throw error; }
  assert.equal(await page.getByRole('heading', {name:'Second route'}).evaluate(element => getComputedStyle(element).color), 'rgb(12, 34, 56)', 'Dynamic-route stylesheet resolves to its captured bundle asset');
  assert.equal(await page.evaluate(async () => (await fetch('/shared/identity.txt')).text()), 'root-relative-native-asset');
  await pause(1500);
  assert.deepEqual(await page.evaluate(() => globalThis.lateNativeReplies), [], 'A native reply from the retired document cannot reach the new route');
  assert.equal(await count().textContent(), '0', 'Native route state must be isolated');
  await page.getByRole('button', {name:'Increase count'}).click();
  await page.waitForFunction(() => document.querySelector('output')?.textContent === '21');
  await page.goto('https://appassets.tachyon.local/');
  try { await page.getByRole('heading', {name:'First route'}).waitFor(); }
  catch (error) { console.error('Android root document:', (await page.content()).slice(0,4096)); throw error; }
  assert.equal(await count().textContent(), '21', 'First route state survives native page navigation');
  console.log('PASS: Android main-frame/origin/depth/byte/navigation boundary probes');
  const input = page.getByRole('textbox', {name:'Your name'});
  await input.focus();
  await input.evaluate(element => { globalThis.focusEvidence = element; });
  await input.pressSequentially('Ada Lovelace', {delay:40});
  assert.equal(await input.inputValue(), 'Ada Lovelace');
  assert.equal(await input.evaluate(element => document.activeElement === element && globalThis.focusEvidence === element), true, 'Native input must retain DOM identity and focus through reactive writes');
  // A native method waits for outstanding assignments, so this also proves
  // the typed value reached Kotlin after its deliberately delayed setters.
  await page.getByRole('button', {name:'Increase count'}).click();
  await page.waitForFunction(() => document.querySelector('output')?.textContent === '42');
  assert.equal(await page.evaluate(async () => JSON.parse(await globalThis.__tachyonNativeHostCall(
    'companion.invoke', JSON.stringify({route:'/',op:'get',name:'name'}))).value), 'Ada Lovelace');
  writeFileSync(path.join(evidence,'report.json'), JSON.stringify({target:'android',host,pid:Number(pid),runtime:await page.evaluate(() => navigator.userAgent),checks:['Kotlin compilation','live WebView','field mutation','native OS API','route isolation','route ownership rejection','opaque sandboxed srcdoc bridge absence','local subframe rejection despite spoofed origin and frame claims','foreign origin bridge absence','raw authorized main-frame call','outer and inner JSON depth64/65 budgets','native UTF8 payload budget','bounded async pending queue and navigation retirement','native publish','accessible controls','per-character input focus and DOM identity','stale service worker retirement','packaged dynamic route','dynamic route stylesheet','root-relative shared asset']},null,2)+'\n');
  console.log(`PASS: Android WebView and Kotlin native evidence: ${evidence}`);
} finally {
  if (browser) await browser.close();
  if (port) run(adb, ['forward', '--remove', `tcp:${port}`]);
  run(adb, ['shell', 'am', 'force-stop', packageName]);
  rmSync(project, {recursive:true,force:true});
}
