#!/usr/bin/env node
// Compiles every snippet the site publishes.
//
// A documentation snippet that does not compile is worse than no snippet: it
// teaches an API that does not exist. Tac templates are put through the real
// compiler, and handler snippets are parsed by their own runtime.

import { spawnSync } from 'node:child_process'
import { cpSync, existsSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

// Falling back to whatever `ty` is installed is how this gate reported that
// every snippet compiled while the checkout's own compiler rejected two of
// them. The build is preferred; TAC_BIN overrides; PATH is the last resort.
const TY = process.env.TAC_BIN
  ?? [
    '/Volumes/ANNEX/build/cargo-target/debug/ty',
    fileURLToPath(new URL('../../target/debug/ty', import.meta.url)),
    fileURLToPath(new URL('../../target/release/ty', import.meta.url)),
  ].find((candidate) => existsSync(candidate))
  ?? 'ty'
const here = (relative) => new URL(relative, import.meta.url)
// Emitting a tac.ts needs TypeScript 6 or newer, and the machine's PATH may
// well have an older one on it. The repo pins its own, so that is the one the
// gate compiles against.
const TOOLCHAIN_BIN = path.resolve(fileURLToPath(here('../..')), 'node_modules/.bin')
// A `yon.rs` is compiled by rustc in a temporary directory, where the repo's
// rust-toolchain.toml does not apply. On a machine with no rustup default
// that fails for a reason that has nothing to do with the snippet, so the
// repo's own pin is passed through — the same answer TOOLCHAIN_BIN is.
const BUN = process.env.TAC_BUN ?? 'bun'
const RUST_TOOLCHAIN = (
  await readFile(here('../../rust-toolchain.toml'), 'utf8').catch(() => '')
).match(/channel\s*=\s*"([^"]+)"/)?.[1]
const features = JSON.parse(await readFile(here('../client/shared/data/features.json'), 'utf8')).features
const languages = JSON.parse(await readFile(here('../client/shared/data/languages.json'), 'utf8'))
const docs = JSON.parse(await readFile(here('../client/shared/data/docs.json'), 'utf8'))

let failures = 0
const fail = (label, detail) => { failures += 1; console.error(`FAIL ${label}\n  ${String(detail).split('\n')[0]}`) }
const pass = (label) => console.log(`  ok  ${label}`)

/** Compiles one Tac template as a real route. */
const compileTemplate = (label, template, component) => {
  const root = mkdtempSync(path.join(tmpdir(), 'ty-snippet-'))
  try {
    mkdirSync(path.join(root, 'client/pages'), { recursive: true })
    writeFileSync(path.join(root, 'client/pages/tac.html'), template)
    // Every snippet names fields it does not declare, so a permissive companion
    // stands in for the application's own.
    writeFileSync(path.join(root, 'client/pages/tac.js'),
      'export default class {\n' +
      "  title = ''\n  message = ''\n  state = ''\n  status = 'ready'\n  count = 0\n  label = ''\n" +
      '  products = []\n  pageCount = 3\n  topic = { slug: "a", title: "A" }\n  topics = []\n  tabs = []\n' +
      '  product = { name: "n", price: 1 }\n  restocking = false\n  selected = null\n  $draft = ""\n  $$theme = "light"\n' +
      '  increment() {}\n  select() {}\n  refresh() {}\n}\n')
    if (component) {
      const dir = path.join(root, 'client/components/product/card')
      mkdirSync(dir, { recursive: true })
      writeFileSync(path.join(dir, 'tac.html'), '<article><h3>{name}</h3><slot /></article>')
      writeFileSync(path.join(dir, 'tac.js'), 'export default class { name = "" }')
      for (const extra of ['sales/chart', 'heavy/editor', 'static/note']) {
        const d = path.join(root, 'client/components', extra)
        mkdirSync(d, { recursive: true })
        writeFileSync(path.join(d, 'tac.html'), '<p>x</p>')
        // A scheduled mount needs an instance to activate, so every stand-in
        // component gets a companion.
        writeFileSync(path.join(d, 'tac.js'), 'export default class {}')
      }
    }
    const built = spawnSync(TY, ['bundle', root], { encoding: 'utf8' })
    if (built.status === 0) pass(label)
    else fail(label, built.stdout || built.stderr)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

/**
 * Compiles one companion beside a template, through the real compiler.
 *
 * `node --check` cannot stand in for this. `@onMount` is a line the Tachyon
 * compiler strips, not a JavaScript decorator, so a companion using it is not
 * parseable JavaScript until the compiler has been over it — which is exactly
 * why every companion snippet on this page used to be skipped silently. A
 * TypeScript companion is emitted by the project's own tsc, so it needs the
 * repo's TypeScript ahead of whatever is on the machine's PATH.
 */
const compileCompanion = (label, name, code) => {
  const root = mkdtempSync(path.join(tmpdir(), 'ty-companion-'))
  try {
    const pages = path.join(root, 'client/pages')
    mkdirSync(pages, { recursive: true })
    writeFileSync(path.join(pages, 'tac.html'), '<main aria-label="Snippet"><p>ok</p></main>\n')
    writeFileSync(path.join(pages, path.basename(name)), code)
    const built = spawnSync(TY, ['bundle', root], {
      encoding: 'utf8',
      env: {
        ...process.env,
        PATH: `${TOOLCHAIN_BIN}${path.delimiter}${process.env.PATH ?? ''}`,
        ...(RUST_TOOLCHAIN ? { RUSTUP_TOOLCHAIN: RUST_TOOLCHAIN } : {}),
      },
    })
    if (built.status === 0) pass(label)
    else fail(label, built.stdout || built.stderr)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

/**
 * The HTTP method a snippet actually declares.
 *
 * Invoking GET at every handler passes a POST-only one on a 405, which is a
 * pass that proves nothing — and did: the PHP example answered a lowercase
 * `post` with the old envelope shape and was green throughout.
 */
const declaredMethod = (code) => {
  const found = code.match(/\b(GET|POST|PUT|PATCH|DELETE|OPTIONS|HEAD)\s*\(/)
  return found ? found[1] : 'GET'
}

/**
 * Resolves and runs one handler through the real toolchain.
 *
 * An ahead-of-time handler is compiled during discovery, so invoking it is the
 * only way to know the snippet builds — `node --check` has nothing to say
 * about a `yon.rs`, and neither does the bundler.
 */
const invokeHandler = (label, name, code) => {
  // A shared root rather than a fresh temporary directory: `ty` keys a
  // compiled handler on its source digest, so an unchanged snippet is built
  // once ever instead of once per run. Java, Kotlin, C# and Rust are slow
  // enough that the difference is the gate being run or skipped.
  const root = path.join(tmpdir(), 'ty-snippet-cache')
  try {
    const route = path.join(root, path.dirname(name))
    mkdirSync(route, { recursive: true })
    writeFileSync(path.join(root, name), code)
    // Nothing to copy: Tachyon stages the PHP protocol runtime itself now, so
    // a project no longer ships the half of the protocol Tachyon owns.
    const method = declaredMethod(code)
    const result = spawnSync(TY, ['handler', 'invoke', name, '--method', method], {
      cwd: root,
      encoding: 'utf8',
      env: {
        ...process.env,
        PATH: `${TOOLCHAIN_BIN}${path.delimiter}${process.env.PATH ?? ''}`,
        ...(RUST_TOOLCHAIN ? { RUSTUP_TOOLCHAIN: RUST_TOOLCHAIN } : {}),
      },
    })
    const answer = result.stdout ?? ''
    // A 405 means the method the snippet declares was not the one dispatched,
    // which is the snippet being wrong rather than the handler answering.
    if (result.status === 0 && !answer.includes('"error"') && !answer.includes('"status": 405'))
      pass(label)
    else fail(label, answer || result.stderr)
  } finally {
    // The route is removed, the cache under it is not.
    rmSync(path.join(root, path.dirname(name)), { recursive: true, force: true })
  }
}

/**
 * Stages every file a feature declares and invokes its controller.
 *
 * A delegate calls a second program, so invoking the handler alone would prove
 * only that it fails to find one. The whole feature goes into the project.
 */
const invokeWithSiblings = (label, feature, name, code) => {
  const root = mkdtempSync(path.join(tmpdir(), 'ty-relay-'))
  try {
    for (const file of feature.files) {
      if (!file.name.includes('/')) continue
      mkdirSync(path.join(root, path.dirname(file.name)), { recursive: true })
      writeFileSync(path.join(root, file.name), file.code)
    }
    const result = spawnSync(TY, ['handler', 'invoke', name, '--method', 'GET'], {
      cwd: root,
      encoding: 'utf8',
      env: {
        ...process.env,
        PATH: `${TOOLCHAIN_BIN}${path.delimiter}${process.env.PATH ?? ''}`,
        ...(RUST_TOOLCHAIN ? { RUSTUP_TOOLCHAIN: RUST_TOOLCHAIN } : {}),
      },
    })
    const answer = result.stdout ?? ''
    if (result.status === 0 && !answer.includes('"error"')) pass(label)
    else fail(label, answer || result.stderr)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

/** Stages a whole layered suite and invokes its controller. */
const invokeSuite = (label, entry, controller) => {
  const root = mkdtempSync(path.join(tmpdir(), 'ty-suite-'))
  try {
    for (const file of entry.files) {
      mkdirSync(path.join(root, path.dirname(file.name)), { recursive: true })
      writeFileSync(path.join(root, file.name), file.code)
    }
    const runtime = entry.id === 'javascript' || entry.id === 'typescript'
      ? ['--javascript-runtime', BUN]
      : []
    const result = spawnSync(TY, ['handler', 'invoke', controller.name, '--method', 'GET', ...runtime], {
      cwd: root,
      encoding: 'utf8',
      env: { ...process.env, PATH: `${TOOLCHAIN_BIN}${path.delimiter}${process.env.PATH ?? ''}` },
    })
    const answer = result.stdout ?? ''
    if (result.status === 0 && !answer.includes('"error"')) pass(label)
    else fail(label, answer || result.stderr)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

/** Parses a handler with its own runtime. */
const parseHandler = (label, name, code) => {
  // Java requires the file to be named after its public class, so a fixed
  // `snippet.java` fails on the name rather than on the code.
  const javaClass = name.endsWith('.java') && code.match(/public\s+class\s+(\w+)/)?.[1]
  const file = path.join(
    mkdtempSync(path.join(tmpdir(), 'ty-handler-')),
    javaClass ? `${javaClass}.java` : name,
  )
  writeFileSync(file, code)
  const runner =
    name.endsWith('.py') ? ['python3', ['-m', 'py_compile', file]]
    : name.endsWith('.rb') ? ['ruby', ['-c', file]]
    : name.endsWith('.sh') ? ['sh', ['-n', file]]
    : name.endsWith('.php') ? ['php', ['-l', file]]
    : name.endsWith('.java') ? ['javac', ['-d', path.dirname(file), file]]
    // Kotlin, C# and Rust have no parse-only mode: kotlinc and dotnet build a
    // whole artefact, and `rustc --emit=metadata` would demand a `main` a
    // fragment does not have. A layer under `server/` outside a route is a
    // fragment, so it is shown and said to be shown.
    : name.endsWith('.kt') ? null
    : name.endsWith('.cs') ? null
    : name.endsWith('.rs') ? null
    : name.endsWith('.ts') || /^\s*@[A-Z]/m.test(code)
      // Node rejects decorator syntax outright, so a decorated module is
      // parsed by a runtime that transpiles it. `bun build` parses without
      // running, which is what a syntax check should do.
      ? [BUN, ['build', '--no-bundle', file, '--outfile', `${file}.out`]]
    : ['node', ['--check', file]]
  if (!runner) {
    console.log(`  --  ${label} (shown, not parsed: ${path.extname(name)} has no parse-only mode)`)
    return
  }
  const result = spawnSync(runner[0], runner[1], { encoding: 'utf8' })
  if (result.error) { console.log(`  --  ${label} (${runner[0]} unavailable)`); return }
  if (result.status === 0) pass(label)
  else fail(label, result.stderr || result.stdout)
}

console.log('== feature snippets')
for (const feature of features) {
  for (const file of feature.files) {
    const label = `${feature.id} / ${file.name}`
    if (file.name.endsWith('.html') && feature.verify?.startsWith('tac')) {
      compileTemplate(label, file.code, feature.verify === 'tac-component')
    } else if (/^client\/.*\/tac\.(js|ts)$/.test(file.name)) {
      compileCompanion(label, file.name, file.code)
    } else if (/\.(js|mjs)$/.test(file.name) && !file.name.includes('config')) {
      parseHandler(label, 'snippet.mjs', file.code)
    // A `yon.ts` matched no branch at all and was skipped in silence, which is
    // the one outcome a gate must never have.
    } else if (/^server\/.*\.ts$/.test(file.name)) {
      parseHandler(label, 'snippet.ts', file.code)
    // `php` is in this list because it was not, and the PHP example went
    // stale unnoticed: it still described a shebang naming a shared runtime,
    // and returned the old envelope shape from a lowercase method. Parsing
    // would not have caught it either — only running it does.
    } else if (/^server\/routes\/.*\/yon\.(rs|java|cpp|cc|cxx|c|cs|kt|go|php)$/.test(file.name)) {
      // A relay is only proven by running what it relays to, so its
      // whole feature is staged rather than the handler alone. Without the
      // callee on this machine the check is reported as skipped, because a
      // silent pass reads as coverage it does not have.
      // A streaming handler writes length-prefixed frames, and `ty handler
      // invoke` reads one response — it refuses with TY2015 rather than
      // pretending. Parsing is what is left, and it is said out loud.
      if (/^\s*(@|#\[|\[)Stream\b/m.test(file.code)) {
        parseHandler(label, `snippet${path.extname(file.name)}`, file.code)
        console.log(`      (parsed, not invoked: @Stream answers with a stream)`)
      } else if (feature.verify === 'yon-relay') {
        if (spawnSync('ruby', ['--version']).status === 0) {
          invokeWithSiblings(label, feature, file.name, file.code)
        } else {
          console.log(`  --  ${label} (shown, not run: no ruby on this machine)`)
        }
      } else {
        invokeHandler(label, file.name, file.code)
      }
    } else if (/\.(py|rb|sh)$/.test(file.name)) {
      parseHandler(label, `snippet${path.extname(file.name)}`, file.code)
    // A service or a repository is not under `server/routes`, so it reached no
    // branch at all and was skipped without a word. PHP can still be parsed:
    // it resolves an attribute lazily, so `#[Service]` needs nothing declared.
    } else if (/^server\/.*\.php$/.test(file.name)) {
      parseHandler(label, 'snippet.php', file.code)
    } else if (/^server\/.*\.(java|kt|cs|rs)$/.test(file.name)) {
      // The compiled four declare their annotations in the protocol runtime,
      // and only a route handler is given one. A layer beneath a route is a
      // fragment on its own, so `@Service` resolves to nothing — which is the
      // same reason a compiled handler cannot import the layers below it.
      console.log(`  --  ${label} (shown, not compiled: the annotations are declared in the `
        + `protocol runtime, which only a route handler is given)`)
    } else if (file.name.endsWith('.json') || file.name === '.tachyonrc') {
      try { JSON.parse(file.code.replace(/^[^{]*\n/, '')); pass(label) }
      catch (error) { fail(label, error) }
    }
  }
}

console.log('== language snippets')
// The Tac layer is one example across every target — a view, a stylesheet and
// one companion per language — where the Yon layer is one handler written in a
// choice of languages. The two are walked differently because they are
// different shapes, not because one is special.
for (const file of languages.tac.files) {
  if (/\.(js|mjs)$/.test(file.name)) parseHandler(`tac / ${file.name}`, 'snippet.mjs', file.code)
  else if (file.name.endsWith('.html')) compileTemplate(`tac / ${file.name}`, file.code, false)
}
// Every Yon language is compiled and run for real through `ty handler invoke`,
// which is the only check that proves the example answers rather than merely
// parses. A language whose toolchain is absent on this machine is reported as
// skipped rather than passed — a silent skip reads as coverage it does not
// have.
// Each Yon language is now a layered suite: a controller, a service and a
// repository. Only the controller is invoked, and only where the language can
// actually reach the layers below it — a compiled handler is one source file,
// so a `yon.java` has no classpath for `server/services` and could not call it
// even though the annotation on it is real and checked. The rest are parsed,
// and the difference is reported rather than passed over.
// Where a controller can actually reach the layers below it, and so where
// invoking one proves the example rather than only the file.
//
// TypeScript is absent for a reason worth naming: a shebang'd handler is staged
// to `.tachyon/handlers/<digest>-yon.ts` with its shebang stripped, so its
// relative imports no longer resolve from where it now sits. PHP escapes that
// only because its runtime resolves `require_once` from the project root.
const LAYERED = new Set(['javascript', 'python', 'php'])

// Languages whose suite is shown but not compiled, each for a stated reason —
// a silent skip reads as coverage this does not have. Java, Kotlin and C# are
// compiled one source file at a time, so a handler has no classpath for
// `server/services` and could not call it even though the annotation on it is
// real and checked. Compiling the suite together would prove something the
// framework cannot yet do.
const SHOWN_ONLY = new Map([
  ['java', 'a compiled handler is one source file, so the layers are not on its classpath'],
  ['kotlin', 'kotlinc builds one file, so the layers are not reachable'],
  ['csharp', 'the generated project holds one file, so the layers are not reachable'],
  ['typescript', 'a shebang handler is staged elsewhere, so its relative imports do not resolve'],
  ['rust', 'rustc builds one file, so the layers are not reachable'],
])
for (const entry of languages.yon.entries) {
  for (const file of entry.files) {
    const label = `yon/${entry.id} / ${file.name.split('/').pop()}`
    if (SHOWN_ONLY.has(entry.id) && !/\.(ts|rs)$/.test(file.name)) {
      console.log(`  --  ${label} (shown, not compiled: ${SHOWN_ONLY.get(entry.id)})`)
    } else if (file.name.startsWith('server/routes/') && LAYERED.has(entry.id)) {
      // Staged whole, so the controller's imports resolve the way they will.
      invokeSuite(label, entry, file)
    } else if (/\.(py|rb|sh)$/.test(file.name)) {
      parseHandler(label, `snippet${path.extname(file.name)}`, file.code)
    } else if (/\.(js|mjs)$/.test(file.name)) {
      parseHandler(label, 'snippet.mjs', file.code)
    } else if (file.name.endsWith('.php')) {
      parseHandler(label, 'snippet.php', file.code)
    } else if (/\.(ts|java|kt|cs|rs)$/.test(file.name)) {
      parseHandler(label, `snippet${path.extname(file.name)}`, file.code)
    } else if (file.code.trim() === '') {
      // A package marker has nothing to check and is not a silent skip.
      pass(`${label} (empty package marker)`)
    } else {
      console.log(`  --  ${label} (no checker for ${path.extname(file.name)})`)
    }
  }
}

console.log('== guide snippets')
for (const [slug, topic] of Object.entries(docs.topics)) {
  for (const section of topic.sections ?? []) {
    for (const file of section.files ?? []) {
      if (file.name.endsWith('.html')) compileTemplate(`${slug} / ${section.heading}`, file.code, false)
    }
  }
}

console.log(failures === 0 ? '\nPASS: every snippet compiles' : `\nFAIL: ${failures} snippet(s)`)
process.exitCode = failures === 0 ? 0 : 1
