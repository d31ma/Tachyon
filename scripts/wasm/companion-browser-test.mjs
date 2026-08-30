#!/usr/bin/env node
// End-to-end gate for wasm companions (ADR 0011).
//
// A module that compiles and ships but never resolves a value in the page is
// the defect this guards, so nothing is asserted from the generated HTML: the
// page is loaded in a real browser and every client component is driven.
//
// One fixture per language, all in one project and one page, so the two module
// shapes — a bare module from rustc, a glued one from a WasmGC toolchain — are
// proven to be indistinguishable to everything above the Tac client runtime. A
// language whose toolchain this machine lacks is reported and skipped, never
// silently dropped.

import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { spawnSync } from 'node:child_process';
import { createReadStream, existsSync, mkdirSync, statSync, copyFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '../..');
const TY = process.env.TAC_BIN ?? path.join(REPO, 'target/release/ty');
const PROJECT = path.join(tmpdir(), 'ty-wasm-companion-gate');
const MIME = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.mjs': 'text/javascript',
  '.css': 'text/css',
  '.wasm': 'application/wasm',
};

// Every language whose companion the build can compile, with the value its
// fixture reports so a panel cannot pass on another language's module.
const LANGUAGES = [
  { extension: 'rs', component: 'rust-panel', label: 'from rust' },
  { extension: 'dart', component: 'dart-panel', label: 'from dart' },
  { extension: 'kt', component: 'kotlin-panel', label: 'from kotlin' },
  { extension: 'swift', component: 'swift-panel', label: 'from swift' },
  { extension: 'cs', component: 'sharp-panel', label: 'from c#' },
];

const write = (relative, contents) => {
  const file = path.join(PROJECT, relative);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, contents);
};

const panel = ({ component }) => `<section>
  <p id="${component}-count">{count}</p>
  <p id="${component}-label">{label}</p>
  <p id="${component}-doubled">{doubled()}</p>
  <button id="${component}-bump" on:click="count += 1">bump</button>
</section>
`;

rmSync(PROJECT, { recursive: true, force: true });
for (const language of LANGUAGES) {
  write(`client/components/${language.component}/tac.html`, panel(language));
  copyFileSync(
    path.join(HERE, `abi-fixture.${language.extension}`),
    path.join(PROJECT, `client/components/${language.component}/tac.${language.extension}`),
  );
}

// ty doctor answers whether this machine can build each language, which is the
// difference between a gate that failed and a toolchain that is absent.
const report = spawnSync(TY, ['doctor', '--json', PROJECT], { encoding: 'utf8' });
const toolchains = JSON.parse(report.stdout || '{"toolchains":[]}').toolchains ?? [];
const ready = (language) =>
  toolchains.some(
    (entry) => entry.requirement.startsWith(`tac.${language.extension} `) && entry.state.state === 'ready',
  );
const skipped = LANGUAGES.filter((language) => !ready(language));
const testing = LANGUAGES.filter(ready);
for (const language of skipped) {
  console.log(`  skip   ${language.extension}: toolchain not ready on this machine`);
  rmSync(path.join(PROJECT, `client/components/${language.component}`), { recursive: true, force: true });
}
if (testing.length === 0) {
  console.error('no wasm toolchain is ready; run ty doctor');
  process.exit(1);
}

write(
  'client/pages/tac.html',
  `<main aria-label="Wasm">\n${testing.map((language) => `  <${language.component} hydrate="load" />`).join('\n')}\n</main>\n`,
);

const built = spawnSync(TY, ['build', PROJECT], { encoding: 'utf8' });
if (built.status !== 0) {
  console.error(built.stderr || built.stdout);
  console.error('build failed; run ty doctor to check the wasm toolchain');
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

let failed = 0;
const expect = (actual, wanted, label) => {
  if (actual === wanted) console.log(`    ok   ${label}: ${actual}`);
  else { failed += 1; console.error(`  FAIL   ${label}: expected ${JSON.stringify(wanted)}, got ${JSON.stringify(actual)}`); }
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
  // A .NET companion boots a runtime before it can answer, which is slower than
  // instantiating a module. Wait for the authored panels as well as the
  // expression placeholders to settle: checking only for an absent placeholder
  // can win the race before the client runtime has rendered its first node.
  await page
    .waitForFunction(
      (components) =>
        components.every((component) => document.querySelector(`#${component}-count`))
        && !document.querySelector('tachyon-expr'),
      testing.map(({ component }) => component),
      { timeout: 60000 },
    )
    .catch(() => {
      failed += 1;
      console.error(`  FAIL   a component never resolved${errors.length ? `: ${errors[0]}` : ''}`);
    });

  for (const language of testing) {
    const id = language.component;
    console.log(`\n  ${language.extension}`);

    // Values come from a module the language's own compiler produced, through
    // one JSON protocol, with no bindgen anywhere.
    expect(await page.textContent(`#${id}-count`), '6', 'field read from wasm');
    expect(await page.textContent(`#${id}-label`), language.label, 'string field from wasm');
    expect(await page.textContent(`#${id}-doubled`), '12', 'method call into wasm');

    // An assigning binding writes through the ABI and the client rerenders, so
    // a wasm companion behaves exactly like a JavaScript one.
    await page.click(`#${id}-bump`);
    await page.waitForFunction(
      (selector) => document.querySelector(selector).textContent === '7',
      `#${id}-count`,
      { timeout: 5000 },
    );
    expect(await page.textContent(`#${id}-count`), '7', 'assignment writes into wasm');
    expect(await page.textContent(`#${id}-doubled`), '14', 'method observes the written field');

    const component = `tachyon-component[data-tachyon-component="${id}"]`;
    expect(await page.getAttribute(component, 'data-tachyon-active'), 'true', 'component mounted');
    expect(await page.getAttribute(component, 'data-tachyon-mount-error'), null, 'no mount error');
  }
  expect(errors.length, 0, `no console errors${errors.length ? `: ${errors[0]}` : ''}`);
}
finally {
  await browser.close();
  server.close();
}

if (failed) process.exit(1);
console.log(`\nwasm companion gate passed for ${testing.map((language) => language.extension).join(', ')}`);
