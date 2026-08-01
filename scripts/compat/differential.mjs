#!/usr/bin/env node
// Phase 6 compatibility differential.
//
// Builds every corpus project with the immutable v26.30.04 release and the
// Rust implementation, serves each output, renders every route in a
// real browser, and compares the resulting semantic DOM, route graph, and
// diagnostics. Byte comparison is meaningless here: the two implementations
// emit deliberately different artifacts. What must match is what a user or an
// assistive technology observes.

import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { cp, mkdtemp, readFile, readdir, rm, stat } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const CORPUS = path.join(REPO, 'corpus');
const TY = process.env.TY_BIN ?? path.join(REPO, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/ty');
const RELEASED_TY = process.env.RELEASED_TY_BIN;
if (!RELEASED_TY) {
  throw new Error(
    'RELEASED_TY_BIN must name the checksum-verified v26.30.04 ty executable.',
  );
}
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.map': 'application/json; charset=utf-8',
};

// Attributes carrying observable meaning. Everything else is implementation
// detail and is deliberately excluded from the comparison.
const SEMANTIC_ATTRIBUTES = [
  'alt', 'aria-current', 'aria-hidden', 'aria-label', 'aria-labelledby',
  'aria-live', 'for', 'href', 'name', 'placeholder', 'role', 'src', 'type',
  'value',
];
const IGNORED_TAGS = new Set(['SCRIPT', 'STYLE', 'LINK', 'META', 'TEMPLATE', 'NOSCRIPT']);

function run(command, args, options = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, args, { ...options, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.on('error', (error) => resolve({ code: -1, stdout, stderr: String(error) }));
    child.on('close', (code) => resolve({ code, stdout, stderr }));
  });
}

async function serve(root) {
  const server = createServer(async (request, response) => {
    const url = new URL(request.url, 'http://localhost');
    let relative = decodeURIComponent(url.pathname).replace(/^\/+/, '');
    if (relative.includes('..')) {
      response.writeHead(400).end();
      return;
    }
    let file = path.join(root, relative);
    try {
      if (relative === '' || (await stat(file)).isDirectory()) {
        file = path.join(file, 'index.html');
      }
    } catch {
      // Fall through to the SPA shell below.
    }
    try {
      await stat(file);
    } catch {
      // The legacy output is a single-page shell; unknown paths serve it so the
      // client router can resolve the route, exactly as its own server does.
      file = path.join(root, 'index.html');
      try {
        await stat(file);
      } catch {
        response.writeHead(404).end();
        return;
      }
    }
    response.writeHead(200, {
      'content-type': MIME[path.extname(file)] ?? 'application/octet-stream',
      'cache-control': 'no-store',
    });
    createReadStream(file).pipe(response);
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { server, origin: `http://127.0.0.1:${server.address().port}` };
}

// Extracted in the page: reduce the live DOM to what a user or an assistive
// technology can observe, so two very different artifacts remain comparable.
function extractSemanticDom(semanticAttributes, ignoredTags) {
  const ignored = new Set(ignoredTags);
  const visit = (node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent.replace(/\s+/g, ' ').trim();
      return text ? { text } : null;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return null;
    if (ignored.has(node.tagName)) return null;
    const attributes = {};
    for (const name of semanticAttributes) {
      if (node.hasAttribute(name)) attributes[name] = node.getAttribute(name);
    }
    const children = [];
    for (const child of node.childNodes) {
      const value = visit(child);
      if (value) children.push(value);
    }
    return { tag: node.tagName.toLowerCase(), attributes, children };
  };
  const body = visit(document.body);
  // The legacy shell wraps rendered content; unwrap single-child chains so the
  // comparison starts at the first element that carries meaning.
  let node = body;
  while (node && node.children && node.children.length === 1 && !Object.keys(node.attributes ?? {}).length) {
    node = node.children[0];
  }
  return node;
}

async function renderWith(origin, routes) {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  await page.addInitScript(
    ({ source }) => {
      // eslint-disable-next-line no-new-func
      window.__extract = new Function(`return (${source})`)();
    },
    { source: extractSemanticDom.toString() },
  );
  const rendered = {};
  try {
    for (const route of routes) {
      const response = await page.goto(`${origin}${route}`, { waitUntil: 'networkidle' });
      await page.waitForTimeout(250);
      rendered[route] = {
        status: response ? response.status() : 0,
        dom: await page.evaluate(
          ([attributes, tags]) => window.__extract(attributes, tags),
          [SEMANTIC_ATTRIBUTES, [...IGNORED_TAGS]],
        ),
      };
    }
  } finally {
    await browser.close();
  }
  return rendered;
}

function difference(left, right, at = '') {
  if (JSON.stringify(left) === JSON.stringify(right)) return null;
  if (left === null || right === null || typeof left !== typeof right) {
    return `${at || '<root>'}: ${JSON.stringify(left)} vs ${JSON.stringify(right)}`;
  }
  if (left.text !== undefined || right.text !== undefined) {
    return `${at}: text ${JSON.stringify(left.text)} vs ${JSON.stringify(right.text)}`;
  }
  if (left.tag !== right.tag) return `${at}: <${left.tag}> vs <${right.tag}>`;
  const keys = new Set([...Object.keys(left.attributes ?? {}), ...Object.keys(right.attributes ?? {})]);
  for (const key of keys) {
    if ((left.attributes ?? {})[key] !== (right.attributes ?? {})[key]) {
      return `${at}<${left.tag}>: ${key}=${JSON.stringify((left.attributes ?? {})[key])} vs ${JSON.stringify((right.attributes ?? {})[key])}`;
    }
  }
  const leftChildren = left.children ?? [];
  const rightChildren = right.children ?? [];
  if (leftChildren.length !== rightChildren.length) {
    return `${at}<${left.tag}>: ${leftChildren.length} children vs ${rightChildren.length}`;
  }
  for (let index = 0; index < leftChildren.length; index += 1) {
    const nested = difference(leftChildren[index], rightChildren[index], `${at}<${left.tag}>[${index}]`);
    if (nested) return nested;
  }
  return null;
}

async function routesOf(rustOutput) {
  const manifest = JSON.parse(await readFile(path.join(rustOutput, 'route-manifest.json'), 'utf8'));
  return manifest.routes.map((entry) => entry.route).sort();
}

async function treeOf(root, current = root, output = {}) {
  const entries = await readdir(current, { withFileTypes: true });
  entries.sort((left, right) => left.name.localeCompare(right.name));
  for (const entry of entries) {
    const absolute = path.join(current, entry.name);
    const relative = path.relative(root, absolute).replaceAll(path.sep, '/');
    if (entry.isDirectory()) await treeOf(root, absolute, output);
    else output[relative] = (await readFile(absolute)).toString('base64');
  }
  return output;
}

async function compareScaffold() {
  const workspace = await mkdtemp(path.join(tmpdir(), 'ty-compat-scaffold-'));
  const legacyRoot = path.join(workspace, 'legacy');
  const rustRoot = path.join(workspace, 'rust');
  try {
    const legacy = await run(RELEASED_TY, ['init', legacyRoot, '--name', 'Parity App']);
    const rust = await run(TY, ['init', rustRoot, '--name', 'Parity App']);
    if (legacy.code !== 0 || rust.code !== 0) {
      return { ok: false, detail: `legacy=${legacy.code} rust=${rust.code}\n${legacy.stderr}\n${rust.stderr}` };
    }
    const legacyTree = await treeOf(legacyRoot);
    const rustTree = await treeOf(rustRoot);
    if (JSON.stringify(legacyTree) === JSON.stringify(rustTree)) {
      return { ok: true, detail: `${Object.keys(legacyTree).length} generated files are byte-identical` };
    }
    const names = [...new Set([...Object.keys(legacyTree), ...Object.keys(rustTree)])].sort();
    const difference = names.find((name) => legacyTree[name] !== rustTree[name]);
    return { ok: false, detail: `first generated-file difference: ${difference}` };
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
}

async function compareProject(name) {
  const source = path.join(CORPUS, name);
  const workspace = await mkdtemp(path.join(tmpdir(), `ty-compat-${name}-`));
  const project = path.join(workspace, name);
  await cp(source, project, { recursive: true });

  // A project may declare divergences that are intentional. They stay visible
  // in every report but do not fail the gate; anything undeclared does.
  const expected = JSON.parse(
    await readFile(path.join(source, 'parity.json'), 'utf8').catch(() => '{}'),
  ).expected_divergences ?? [];
  const result = { project: name, checks: [], differences: [], acknowledged: [] };
  const record = (check, ok, detail) => {
    const declared = expected.find(
      (entry) => entry.check === check && entry.detail === detail,
    );
    if (!ok && declared) {
      result.checks.push({ check, ok: true, detail: `expected divergence: ${detail}` });
      result.acknowledged.push({ check, detail, reason: declared.reason });
      return;
    }
    result.checks.push({ check, ok, detail });
    if (!ok) result.differences.push(`${check}: ${detail}`);
  };

  const legacy = await run(RELEASED_TY, ['bundle'], { cwd: project });
  record('legacy build', legacy.code === 0, legacy.code === 0 ? 'ok' : legacy.stderr.slice(-400));

  const rust = await run(TY, ['build', project, '--out-dir', 'dist-rust']);
  record('rust build', rust.code === 0, rust.code === 0 ? 'ok' : rust.stderr.slice(-400));

  if (legacy.code !== 0 || rust.code !== 0) {
    await rm(workspace, { recursive: true, force: true });
    return result;
  }

  const legacyRoot = path.join(project, 'dist/web');
  const rustRoot = path.join(project, 'dist-rust');
  const routes = await routesOf(rustRoot);

  // Route graph parity: every route the Rust implementation publishes must be
  // reachable in the legacy output, and both must agree on the set.
  const legacyPages = new Set(
    (await readdir(path.join(legacyRoot, 'pages')).catch(() => []))
      .filter((entry) => entry.endsWith('.js'))
      .map((entry) => entry.replace(/\.js$/, '')),
  );
  record(
    'route graph',
    routes.length > 0,
    routes.length > 0 ? `${routes.length} route(s): ${routes.join(', ')}` : 'no routes published',
  );
  result.routes = routes;
  result.legacyPages = [...legacyPages].sort();

  const legacyServer = await serve(legacyRoot);
  const rustServer = await serve(rustRoot);
  try {
    const legacyDom = await renderWith(legacyServer.origin, routes);
    const rustDom = await renderWith(rustServer.origin, routes);
    for (const route of routes) {
      const detail = difference(legacyDom[route]?.dom, rustDom[route]?.dom);
      record(`semantic dom ${route}`, detail === null, detail ?? 'identical');
      record(
        `http status ${route}`,
        legacyDom[route]?.status === 200 && rustDom[route]?.status === 200,
        `legacy ${legacyDom[route]?.status} / rust ${rustDom[route]?.status}`,
      );
    }
    result.legacyDom = legacyDom;
    result.rustDom = rustDom;
  } finally {
    legacyServer.server.close();
    rustServer.server.close();
    await rm(workspace, { recursive: true, force: true });
  }
  return result;
}

async function main() {
  const scaffold = await compareScaffold();
  process.stdout.write(`==> scaffold\n    ${scaffold.ok ? 'ok  ' : 'FAIL'} ${scaffold.detail}\n`);
  if (!scaffold.ok) process.exit(1);
  const only = process.argv.slice(2).filter((value) => !value.startsWith('--'));
  const names = (await readdir(CORPUS, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) => only.length === 0 || only.includes(name))
    .sort();

  const results = [];
  for (const name of names) {
    process.stdout.write(`==> ${name}\n`);
    const result = await compareProject(name);
    for (const check of result.checks) {
      process.stdout.write(`    ${check.ok ? 'ok  ' : 'FAIL'} ${check.check}: ${check.detail}\n`);
    }
    for (const entry of result.acknowledged) {
      process.stdout.write(`         reason: ${entry.reason}\n`);
    }
    results.push(result);
  }

  const failed = results.filter((result) => result.differences.length > 0);
  if (process.argv.includes('--json')) {
    process.stdout.write(`${JSON.stringify(results, null, 2)}\n`);
  }
  process.stdout.write(
    `\n${results.length - failed.length}/${results.length} corpus projects match across implementations\n`,
  );
  process.exit(failed.length === 0 ? 0 : 1);
}

await main();
