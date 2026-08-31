#!/usr/bin/env node
// Execute the generated transport and JSON budget scanner, without a device.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const source = readFileSync(path.join(repo, 'crates/tachyon-core/src/native/android.rs'), 'utf8');
const shim = source.match(/const ANDROID_BRIDGE_SHIM: &str = r"([\s\S]*?)";/)?.[1];
assert.ok(shim, 'Test must execute the production transport');
const hostSource = readFileSync(path.join(repo, 'crates/tachyon-core/src/native/host.rs'), 'utf8');
const sharedShim = hostSource.match(/const NATIVE_SHIM: &str = r"([\s\S]*?)";/)?.[1].replace('__WINDOW_CONTROLS__', '[]');
assert.ok(sharedShim, 'Test must execute the same shared/Android concatenation as the host');

function transport({ available = true, main = true } = {}) {
  const posted = [];
  const timers = new Map();
  const events = new Map();
  const host = { postMessage: value => posted.push(JSON.parse(value)) };
  let timerId = 0;
  const context = vm.createContext({
    __tachyonAndroidHost: available ? host : undefined,
    TextEncoder,
    setTimeout: (callback, delay) => {
      assert.equal(delay, 10000);
      timers.set(++timerId, callback);
      return timerId;
    },
    clearTimeout: id => timers.delete(id),
    addEventListener: (name, callback) => events.set(name, callback),
  });
  vm.runInContext(`globalThis.top = ${main ? 'globalThis' : '{}'};`, context);
  vm.runInContext(sharedShim + shim, context);
  return { call: context.__tachyonHostPost, host, posted, timers, events };
}

for (const options of [{ available: false }, { main: false }]) {
  const bridge = transport(options);
  await assert.rejects(bridge.call('companion.invoke', '{}'), /unavailable/);
  assert.equal(bridge.posted.length, 0);
}
{
  const bridge = transport();
  const first = bridge.call('companion.invoke', '{}');
  const second = bridge.call('route.open', '{}');
  bridge.host.onmessage({ data: 'not-json' });
  bridge.host.onmessage({ data: JSON.stringify({ id: 'unknown', result: '{}' }) });
  bridge.host.onmessage({ data: JSON.stringify({ id: bridge.posted[1].id, result: '{"second":true}' }) });
  bridge.host.onmessage({ data: JSON.stringify({ id: bridge.posted[0].id, result: '{"first":true}' }) });
  assert.equal(await first, '{"first":true}');
  assert.equal(await second, '{"second":true}');
  assert.equal(bridge.timers.size, 0);
  await assert.rejects(bridge.call('x'.repeat(65), '{}'), /oversized/);
  await assert.rejects(bridge.call('companion.invoke', 'é'.repeat(32769)), /oversized/);
  await assert.rejects(bridge.call('companion.invoke', 'a'.repeat(65537)), /oversized/);
}
{
  const bridge = transport();
  const pending = Array.from({ length: 128 }, () => bridge.call('companion.invoke', '{}'));
  const outcomes = Promise.allSettled(pending);
  await assert.rejects(bridge.call('companion.invoke', '{}'), /Too many/);
  assert.equal(bridge.posted.length, 128);
  bridge.events.get('pagehide')();
  assert.ok((await outcomes).every(result => result.status === 'rejected' && /navigated/.test(result.reason)));
  assert.equal(bridge.timers.size, 0);
  await assert.rejects(bridge.call('companion.invoke', '{}'), /unavailable/);
  bridge.events.get('pageshow')();
  const pendingAgain = bridge.call('companion.invoke', '{}');
  const timedOut = assert.rejects(pendingAgain, /timed out/);
  [...bridge.timers.values()][0]();
  await timedOut;
}

// Compile the exact Java guard, rather than a JavaScript reimplementation.
const guardStart = source.indexOf('  private static boolean withinByteLimit(');
const guardEnd = source.indexOf('  private static synchronized String companionInvoke(', guardStart);
assert.ok(guardStart > 0 && guardEnd > guardStart);
const guards = source.slice(guardStart, guardEnd);
const cases = [
  ['[]', 65536, true],
  [JSON.stringify({ value:'normal "quoted" value with \\backslash' }), 65536, true],
  ['{a"b:1,"x":' + '['.repeat(65) + '0' + ']'.repeat(65) + ',c"d:2}', 65536, false],
  ['['.repeat(64) + '0' + ']'.repeat(64), 65536, true],
  ['['.repeat(65) + '0' + ']'.repeat(65), 65536, false],
  ['['.repeat(32000) + '0' + ']'.repeat(32000), 65536, false],
  [JSON.stringify({ value: '[{'.repeat(1000) + '\\"' }), 65536, true],
  ['{"x":"' + 'a'.repeat(65528) + '"}', 65536, true],
  ['{"x":"' + 'a'.repeat(65529) + '"}', 65536, false],
  ['{"x":"' + 'é'.repeat(32765) + '"}', 65536, false],
  ["{'x':'\"','y':" + '['.repeat(65) + '0' + ']'.repeat(65) + ", 'z':'\"'}", 65536, false],
  ['//"\n' + '['.repeat(65) + '0' + ']'.repeat(65) + '\n//"', 65536, false],
  ['/*"*/' + '['.repeat(65) + '0' + ']'.repeat(65) + '/*"*/', 65536, false],
  ['#"\n' + '['.repeat(65) + '0' + ']'.repeat(65) + '\n#"', 65536, false],
  ['{"unfinished":"value}', 65536, false],
  [']', 65536, false],
];
const directory = mkdtempSync(path.join(tmpdir(), 'tachyon-android-budget-'));
try {
  const checks = cases.map(([value, limit, expected], index) =>
    `if (boundedJson(new String(java.util.Base64.getDecoder().decode("${Buffer.from(value).toString('base64')}"), StandardCharsets.UTF_8), ${limit}) != ${expected}) throw new AssertionError("budget case ${index}");`);
  // Keep class-file UTF8 constants below 64 KiB even for the oversized cases.
  const chunked = checks.map(check => check.replace(/decode\("([A-Za-z0-9+/=]+)"\)/,
    (_, encoded) => `decode(String.join("", ${encoded.match(/.{1,16000}/g).map(part => JSON.stringify(part)).join(',')}))`));
  const file = path.join(directory, 'BridgeBudgetTest.java');
  writeFileSync(file, `import java.nio.charset.StandardCharsets;\npublic final class BridgeBudgetTest {\n${guards}\npublic static void main(String[] args) {\n${chunked.join('\n')}\n}\n}\n`);
  const java = process.env.JAVA_HOME ? path.join(process.env.JAVA_HOME, 'bin/java') : 'java';
  const result = spawnSync(java, [file], { encoding: 'utf8', timeout: 60000 });
  assert.equal(result.status, 0, result.stderr || result.error?.message);
} finally {
  rmSync(directory, { recursive: true, force: true });
}
console.log(`PASS: Android transport failure paths and ${cases.length} actual Java JSON-budget cases`);
