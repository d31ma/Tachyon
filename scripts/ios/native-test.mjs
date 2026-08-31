#!/usr/bin/env node
// Actual iOS Simulator qualification: JavaScript drives the rendered WKWebView,
// and compiled Swift writes bounded proof from the app process into its sandbox.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

assert.equal(process.platform, 'darwin', 'iOS qualification requires macOS and Xcode');
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const project = mkdtempSync(path.join(tmpdir(), 'tachyon-ios-gate-'));
// A private device set avoids altering the user's saved simulators or relying
// on a relocated default device directory being mounted and writable.
const deviceSet = path.join(project, 'simulator-devices');
const appId = 'dev.tachyon.ios-gate';
const evidence = path.join(repo, 'target/phase5-ios-evidence');
const run = (program, args, timeout = 120000) => {
  const result = spawnSync(program, args, {
    cwd: repo, encoding: 'utf8', timeout, maxBuffer: 2 * 1024 * 1024,
  });
  assert.equal(result.status, 0, program + ' ' + args.join(' ') + ': ' +
    (result.error || result.stderr || result.stdout));
  return result.stdout.trim();
};
const simctl = (...args) => run('/usr/bin/xcrun', ['simctl', '--set', deviceSet, ...args]);
const write = (relative, source) => {
  const file = path.join(project, relative);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, source);
};
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
let device;
const cleanup = () => {
  if (device) {
    for (const args of [['terminate', device, appId], ['shutdown', device], ['delete', device]]) {
      spawnSync('/usr/bin/xcrun', ['simctl', '--set', deviceSet, ...args], { timeout: 30000, stdio: 'ignore' });
    }
    device = undefined;
  }
  rmSync(project, { recursive: true, force: true });
};
process.once('SIGINT', () => { cleanup(); process.exit(130); });
process.once('SIGTERM', () => { cleanup(); process.exit(143); });

try {
  run(process.execPath, [path.join(repo, 'scripts/native/apple-protocol-test.mjs')], 90000);
  mkdirSync(deviceSet);
  write('tac.config.js', "export const application = { name:'NativeIOSGate', id:'dev.tachyon.ios-gate', version:'0.0.1', entryRoute:'/' };\n");
  write('client/shared/ios-gate.js', "export const marker = 'Shared module ready';\n");
  write('client/shared/ios-gate.css', 'body { --ios-gate: ready; font-family: sans-serif; padding: 24px; } button,input { min-height: 44px; }\n');
  for (const [prefix, label, value] of [['', 'root', 7], ['items/_id/', 'second', 9]]) {
    write('client/pages/' + prefix + 'tac.html', `<!doctype html><html><head>
<link rel="stylesheet" href="/shared/ios-gate.css"><title>iOS native gate</title></head><body>
<main><h1 id="heading">${label} route</h1><p id="marker">{marker}</p>
<p id="count">{count}</p><p id="signal">{signal}</p>
<button id="update" on:click="count = ${value}">Update native</button>
<input id="name" :value="name" on:input="name = $event.target.value" aria-label="Name">
<a id="next" href="${prefix ? '/' : '/items/7/'}">Next route</a></main></body></html>`);
    write('client/pages/' + prefix + 'tac.js', 'const pageLabel = ' + JSON.stringify(label) + ';\n' + String.raw`import { marker } from '/shared/ios-gate.js';
export default class {
  marker = marker;
  name = '';
  @subscribe('ios.event')
  signal = 'waiting';
  @onMount
  mounted() { setTimeout(() => { void this.probe(); }, 50); }
  async until(check) {
    for (let attempt = 0; attempt < 100; attempt++) {
      if (check()) return;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw new Error('Rendered iOS evidence did not become ready');
  }
  async probe() {
    try {
      await this.until(() => document.querySelector('#heading')?.textContent === pageLabel + ' route');
      const returned = pageLabel === 'root' && this.count === 7;
      if (!returned && this.count !== 0) throw new Error('Native route state leaked');
      if (document.querySelector('#marker').textContent !== 'Shared module ready') throw new Error('Shared JS');
      await this.until(() => getComputedStyle(document.body).getPropertyValue('--ios-gate').trim() === 'ready');
      document.querySelector('#update').click();
      const expected = pageLabel === 'root' ? 7 : 9;
      await this.until(() => document.querySelector('#count')?.textContent === String(expected));
      if (await this.doubled() !== expected * 2) throw new Error('Native method');
      const input = document.querySelector('#name');
      input.focus();
      input.value = 'Ada Lovelace';
      input.dispatchEvent(new Event('input', { bubbles: true }));
      await this.until(() => this.name === 'Ada Lovelace');
      await new Promise(resolve => setTimeout(resolve, 100));
      if (document.querySelector('#name') !== input || document.activeElement !== input) throw new Error('Input focus');
      let routeRejected = false;
      try {
        const rejected = JSON.parse(await globalThis.__tachyonNativeHostCall(
          'companion.invoke', JSON.stringify({ route:'/not-this-route', op:'init' })));
        routeRejected = Boolean(rejected.error);
      } catch { routeRejected = true; }
      if (!routeRejected) throw new Error('Native route impersonation accepted');
      const ownRoute = pageLabel === 'root' ? '/' : '/items/_id';
      const otherRoute = pageLabel === 'root' ? '/items/_id' : '/';
      for (const spelling of ['route', '\\u0072oute']) {
        let duplicateRejected = false;
        const ambiguous = '{"route":' + JSON.stringify(ownRoute) + ',"' + spelling + '":' +
          JSON.stringify(otherRoute) + ',"op":"get","name":"count"}';
        try {
          duplicateRejected = Boolean(JSON.parse(await globalThis.__tachyonNativeHostCall(
            'companion.invoke', ambiguous)).error);
        } catch { duplicateRejected = true; }
        if (!duplicateRejected) throw new Error('Duplicate route key crossed the native guard');
      }
      const unicode = JSON.parse(await globalThis.__tachyonNativeHostCall('companion.invoke',
        JSON.stringify({ '\u0301': '😀', route: ownRoute, op: 'get', name: 'count' })));
      if (unicode.value !== expected) throw new Error('Unicode JSON failed at native boundary');
      await this.announce();
      await this.until(() => document.querySelector('#signal')?.textContent === 'received');
      this.phase = returned ? 'returned' : 'initial';
      this.proof = JSON.stringify({ route: pageLabel, count: this.count,
        doubled: await this.doubled(), nativePid: await this.processId(),
        sharedAssets: true, renderedInteraction: true, focusPreserved: true,
        routeRejected: true, duplicateRouteRejected: true, unicodeBoundaryPassed: true,
        publishReceived: true });
      await this.record();
      if (!returned) document.querySelector('#next').click();
    } catch (error) {
      this.phase = 'error';
      this.proof = JSON.stringify({ error: String(error) });
      await this.record();
    }
  }
}
`);
    write('client/pages/' + prefix + 'tac.swift', `import Foundation
final class Companion {
    var count: Int = 0
    var phase: String = "initial"
    var proof: String = ""
    func doubled() -> Int { count * 2 }
    func processId() -> Int { Int(ProcessInfo.processInfo.processIdentifier) }
    func announce() { tacPublish("ios.event", "received") }
    func record() -> Int {
        let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let safePhase = ["initial", "returned", "error"].contains(phase) ? phase : "error"
        let destination = directory.appendingPathComponent("${label}-" + safePhase + ".json")
        let record: [String: Any] = ["pid": Int(ProcessInfo.processInfo.processIdentifier),
                                    "count": count, "proof": proof]
        if let bytes = try? JSONSerialization.data(withJSONObject: record, options: [.sortedKeys]) {
            try? bytes.write(to: destination, options: .atomic)
        }
        return Int(ProcessInfo.processInfo.processIdentifier)
    }
}
`);
  }
  let binary = process.env.TAC_BIN;
  if (!binary) {
    run('cargo', ['build', '--locked', '--bin', 'ty'], 240000);
    const metadata = JSON.parse(run('cargo', ['metadata', '--format-version', '1', '--no-deps']));
    binary = path.join(metadata.target_directory, 'debug/ty');
  }
  console.log('Building actual iOS host with two route-scoped Swift companions...');
  run(binary, ['build', project, '--target', 'ios'], 240000);
  const output = path.join(project, 'dist/ios');
  const bundle = path.join(output, 'NativeIOSGate.app');
  const manifest = JSON.parse(readFileSync(path.join(output, 'tachyon.host.json'), 'utf8'));
  assert.equal(manifest.schemaVersion, 3);
  assert.deepEqual(manifest.companions, [
    { language: 'Swift', route: '/' }, { language: 'Swift', route: '/items/_id' },
  ]);
  assert.deepEqual(readFileSync(path.join(output, 'web/index.html')),
    readFileSync(path.join(bundle, 'WebBundle/index.html')));

  const runtimes = JSON.parse(simctl('list', 'runtimes', '--json')).runtimes;
  const runtime = runtimes.filter(item => item.isAvailable && item.identifier.includes('.iOS-'))
    .sort((a, b) => b.version.localeCompare(a.version, undefined, { numeric: true }))[0];
  assert.ok(runtime, 'an installed iOS simulator runtime is required');
  const types = runtime.supportedDeviceTypes ??
    JSON.parse(simctl('list', 'devicetypes', '--json')).devicetypes;
  const phone = types.find(item => item.name === 'iPhone SE (3rd generation)') ??
    types.find(item => item.productFamily === 'iPhone');
  assert.ok(phone, 'the installed runtime needs an available iPhone device type');
  device = simctl('create', 'TachyonGate-' + randomUUID(), phone.identifier, runtime.identifier);
  assert.match(device, /^[A-Fa-f0-9-]{36}$/);
  console.log('Booting isolated ' + phone.name + ' with iOS ' + runtime.version + '...');
  simctl('boot', device);
  run('/usr/bin/xcrun', ['simctl', '--set', deviceSet, 'bootstatus', device, '-b'], 240000);
  simctl('install', device, bundle);
  const launch = simctl('launch', device, appId);
  const pid = Number(launch.match(/:\s*(\d+)\s*$/)?.[1]);
  assert.ok(Number.isSafeInteger(pid) && pid > 0, 'simctl must report the actual app PID');
  const container = simctl('get_app_container', device, appId, 'data');
  const documents = path.join(container, 'Documents');
  const required = ['root-initial.json', 'second-initial.json', 'root-returned.json'];
  const deadline = Date.now() + 60000;
  while (Date.now() < deadline) {
    for (const label of ['root', 'second']) {
      const failure = path.join(documents, label + '-error.json');
      assert.ok(!existsSync(failure), existsSync(failure) ? readFileSync(failure, 'utf8') : '');
    }
    if (required.every(name => existsSync(path.join(documents, name)))) break;
    await pause(250);
  }
  mkdirSync(evidence, { recursive: true });
  const proofs = required.map(name => {
    const file = path.join(documents, name);
    assert.ok(existsSync(file), 'real iOS WebView did not produce ' + name);
    const native = JSON.parse(readFileSync(file, 'utf8'));
    const proof = JSON.parse(native.proof);
    assert.equal(native.pid, pid, 'proof must originate in the launched simulator app');
    assert.equal(proof.nativePid, pid, 'JavaScript must call into that native process');
    assert.equal(native.count, name.startsWith('second') ? 9 : 7);
    assert.equal(proof.count, native.count);
    assert.equal(proof.doubled, native.count * 2);
    for (const key of ['sharedAssets', 'renderedInteraction', 'focusPreserved', 'routeRejected',
      'duplicateRouteRejected', 'unicodeBoundaryPassed', 'publishReceived']) {
      assert.equal(proof[key], true, name + ': ' + key);
    }
    return { file: name, ...native, proof };
  });
  simctl('io', device, 'screenshot', path.join(evidence, 'ios-returned-root.png'));
  writeFileSync(path.join(evidence, 'report.json'), JSON.stringify({
    runtime: runtime.version, deviceType: phone.name, binary, pid, proofs,
  }, null, 2) + '\n');
  console.log('PASS: real iOS WKWebView render/input/focus, Swift ABI/OS/publish, shared assets, dynamic routes, and isolation');
} finally {
  cleanup();
}
