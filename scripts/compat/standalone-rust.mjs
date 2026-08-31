#!/usr/bin/env node
// Runs the released standalone-binary smoke contract against the Rust `ty`.

import assert from 'node:assert/strict';
import { access, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { withServer } from '../release/server-probe.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const ty = process.env.TAC_BIN ?? path.join(repo, process.env.CARGO_TARGET_DIR ?? 'target', 'debug', process.platform === 'win32' ? 'ty.exe' : 'ty');
const workspace = await mkdtemp(path.join(tmpdir(), 'tachyon-rust-standalone-'));
const cache = path.join(workspace, 'cache');
const environment = { ...process.env, TAC_CACHE_DIR: cache };

async function exists(file) {
  try { await access(file); return true; } catch { return false; }
}

async function run(args, options = {}) {
  const child = spawn(ty, args, {
    cwd: options.cwd ?? workspace,
    env: options.env ?? environment,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  const code = await new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', resolve);
  });
  if (code !== 0) throw new Error(`ty ${args.join(' ')} failed (${code})\n${stdout}\n${stderr}`);
  return { stdout, stderr };
}

async function stop(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  const closed = new Promise((resolve) => child.once('close', resolve));
  child.kill('SIGTERM');
  await closed;
}

async function waitForHttp(url, deadline = Date.now() + 15_000) {
  let last;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return response;
      last = new Error(`HTTP ${response.status}`);
    } catch (error) { last = error; }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw last ?? new Error(`Timed out waiting for ${url}`);
}

let server;
let preview;
let browser;
try {
  assert.equal((await run(['--version'])).stdout.trim().length > 0, true);
  await run(['init', 'smoke']);
  const project = path.join(workspace, 'smoke');
  const expectedScaffold = [
    '.env.example', '.env.test', '.gitignore', 'README.md', 'jsconfig.json', 'package.json',
    'tachyon-env.d.ts', 'client/pages/tac.html', 'client/pages/tac.js', 'client/pages/tac.css',
    'client/components/hero/tac.html', 'client/components/hero/tac.css',
    'client/shared/assets/.gitkeep', 'client/shared/data/.gitkeep',
    'client/shared/scripts/imports.js', 'client/shared/styles/app.css',
    'server/routes/yon.js', 'server/data/.gitkeep', 'server/deps/.gitkeep',
  ];
  for (const file of expectedScaffold) assert.equal(await exists(path.join(project, file)), true, file);
  assert.equal(await exists(path.join(project, 'db')), false, 'removed database scaffold');
  for (const file of ['.env.example', '.env.test']) {
    const environment = await readFile(path.join(project, file), 'utf8');
    assert.equal(
      environment,
      'YON_PORT=8000\nYON_HOST=127.0.0.1\nYON_HOSTNAME=127.0.0.1\nYON_SKIP_BUNDLE=false\n',
      `${file} contains only current Rust settings`,
    );
  }

  await run(['bundle', '--target', 'web'], { cwd: project });
  assert.equal(await exists(path.join(project, 'dist', 'web', 'index.html')), true);
  const spa = await readFile(path.join(project, 'dist', 'web', 'spa-renderer.js'), 'utf8');
  assert.match(spa, /window\.__tc_rerender/);
  assert.doesNotMatch(spa, /__TACHYON_ASSET_PREFIX__|__tachyonPlaceholder|__tachyonShellPlaceholder/);
  assert.equal((await readdir(path.join(cache, 'runtime'), { withFileTypes: true })).some((entry) => entry.isDirectory()), true);
  await run(['cache', 'clean'], { cwd: project });
  assert.equal(await exists(path.join(cache, 'runtime')), false);

  server = spawn(ty, ['serve', '--port', '18777'], {
    cwd: project,
    env: environment,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const response = await waitForHttp('http://127.0.0.1:18777/');
  assert.deepEqual(await response.json(), { ok: true, framework: 'Tachyon' });
  const runtime = await waitForHttp('http://127.0.0.1:18777/spa-renderer.js');
  assert.match(runtime.headers.get('content-type') ?? '', /javascript/);

  await writeFile(path.join(project, 'client/pages/tac.html'), `
<script>let count = 0</script>
<main>
  <button on:click="count += 1">Add</button><p>Count: {count}</p>
  <p>Required: {required.join('|')}</p><p>Events: {events.join('|')}</p>
</main>
`);
  await writeFile(path.join(project, 'client/pages/tac.js'), `
export default class {
  required = ['page.state', 'mount.lifecycle']
  events = []
  @onMount
  initialize() { this.events = ['mounted'] }
}
`);
  await run(['bundle', '--target', 'web'], { cwd: project });
  preview = spawn(ty, ['preview', '--static', '--port', '18778'], {
    cwd: project,
    env: environment,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  await waitForHttp('http://127.0.0.1:18778/');
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  await page.goto('http://127.0.0.1:18778/');
  await page.getByText('Count: 0', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Add' }).click();
  await assert.doesNotReject(() => page.getByText('Count: 1', { exact: true }).waitFor());
  await page.getByText('Required: page.state|mount.lifecycle', { exact: true }).waitFor();
  await page.getByText('Events: mounted', { exact: true }).waitFor();
  await stop(preview);
  preview = undefined;

  const nativeTarget = process.platform === 'darwin' ? 'macos' : process.platform === 'win32' ? 'windows' : 'linux';
  await run(['bundle', '--native', '--target', nativeTarget], { cwd: project });
  const nativeRoot = path.join(project, 'dist', nativeTarget);
  assert.equal(await exists(path.join(nativeRoot, 'tachyon.host.json')), true);
  const hostManifest = JSON.parse(await readFile(path.join(nativeRoot, 'tachyon.host.json'), 'utf8'));
  assert.equal(hostManifest.schemaVersion, 3);
  assert.equal(hostManifest.renderMode, 'bundle');
  assert.equal(hostManifest.target, nativeTarget);
  assert.equal(hostManifest.entryRoute, '/');
  assert.deepEqual(hostManifest.hostCapabilities, ['companion.invoke']);

  // ADR 0018 retired the separate widget controller. Exercise the emitted
  // native document in Chromium; actual WebView transport has platform gates.
  await withServer(ty, ['preview', project, '--static', '--out-dir', path.join(nativeRoot, 'web'), '--port', '0'], async request => {
    const response = await request('/');
    assert.equal(response.status, 200);
    await page.goto(response.url);
    await page.getByText('Count: 0', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Add' }).click();
    await page.getByText('Count: 1', { exact: true }).waitFor();
    await page.getByText('Required: page.state|mount.lifecycle', { exact: true }).waitFor();
    await page.getByText('Events: mounted', { exact: true }).waitFor();
  });

  // OS access and host publication now belong to real compiled companions,
  // not mocked shortcut/content-surface verbs in the retired controller.
  const nativeAbi = spawnSync(process.execPath, [path.join(repo, 'scripts/native/companion-test.mjs')], {
    cwd: repo, env: { ...environment, TAC_BIN: ty }, encoding: 'utf8',
    timeout: 300_000, maxBuffer: 2 * 1024 * 1024,
  });
  assert.equal(nativeAbi.status, 0, `Native companion ABI failed: ${nativeAbi.error || nativeAbi.stderr || nativeAbi.stdout}`);
  process.stdout.write(nativeAbi.stdout);
  process.stdout.write(`PASS: standalone scaffold, server, web/native-bundle rendering and native ABI (${nativeTarget})\n`);
} finally {
  if (browser) await browser.close();
  await stop(preview);
  await stop(server);
  await rm(workspace, { recursive: true, force: true });
}
