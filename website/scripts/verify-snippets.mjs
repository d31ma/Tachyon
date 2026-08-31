#!/usr/bin/env node
// Check published examples against an explicitly selected or repo-built CLI.
// Syntax, bundle compilation, HTTP execution and omissions are reported apart.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { withServer } from '../../scripts/release/server-probe.mjs'

const repo = fileURLToPath(new URL('../..', import.meta.url))
const target = path.resolve(repo, process.env.CARGO_TARGET_DIR ?? 'target')
const executable = process.platform === 'win32' ? 'ty.exe' : 'ty'
const selected = process.env.TAC_BIN ?? ['debug', 'release'].map(mode => path.join(target, mode, executable)).find(existsSync)
assert.ok(selected, 'Build this checkout first or set TAC_BIN to its binary; installed/PATH ty binaries are never selected')
const binary = path.resolve(selected)
assert.ok(existsSync(binary), `TAC_BIN does not exist: ${binary}`)
const pin = readFileSync(path.join(repo, 'rust-toolchain.toml'), 'utf8').match(/channel\s*=\s*"([^"]+)"/)?.[1]
if (pin && !process.env.RUSTUP_TOOLCHAIN) process.env.RUSTUP_TOOLCHAIN = pin
process.env.PATH = [path.join(repo, 'node_modules/.bin'), path.join(process.env.HOME ?? '', '.cargo/bin'), process.env.PATH ?? ''].join(path.delimiter)
const bun = process.env.TAC_BUN ?? process.env.YON_JAVASCRIPT_RUNTIME ?? 'bun'
process.env.YON_JAVASCRIPT_RUNTIME = bun
const scope = process.argv.find(arg => arg.startsWith('--suite='))?.slice(8) ?? 'all'
assert.ok(['all', 'features', 'languages', 'guides'].includes(scope), 'Use --suite=all|features|languages|guides')
/** @typedef {{name:string,code:string}} Snippet */
/** @param {string} name */
const load = name => JSON.parse(readFileSync(path.join(repo, 'website/client/shared/data', name), 'utf8'))
/** @type {{id:string,files:Snippet[],verify?:string}[]} */
const features = load('features.json').features
/** @type {{tac:{files:Snippet[]},yon:{entries:{id:string,files:Snippet[]}[]}}} */
const languages = load('languages.json')
/** @type {{topics:Record<string,{sections?:{heading:string,code?:string,files?:Snippet[]}[]}>}} */
const docs = load('docs.json')
const counts = { bundle: 0, http: 0, syntax: 0, data: 0, skipped: 0, failed: 0 }
/** @param {string} label @param {'bundle'|'http'|'syntax'|'data'} kind */
const pass = (label, kind) => { counts[kind]++; console.log(`  ok  ${label} [${kind}]`) }
/** @param {string} label @param {string} why */
const skip = (label, why) => { counts.skipped++; console.log(`  --  ${label} [not executed: ${why}]`) }
/** @param {string} label @param {unknown} error */
const fail = (label, error) => { counts.failed++; console.error(`FAIL ${label}\n${String(error instanceof Error ? error.stack : error).slice(0, 8192).split('\n').slice(0, 12).join('\n')}`) }

// Every subprocess has bounded time/output; a private process group is stopped
// on timeout so compiler children cannot outlive this verifier on POSIX hosts.
/** @param {string} command @param {string[]} args @param {{cwd?:string,timeout?:number}} [options] @returns {Promise<string>} */
const run = (command, args, { cwd = repo, timeout = 120_000 } = {}) => new Promise((resolve, reject) => {
  const child = spawn(command, args, { cwd, detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] })
  let stdout = '', stderr = ''
  /** @type {Error|undefined} */
  let failure
  /** @type {ReturnType<typeof setTimeout>|undefined} */
  let killTimer
  /** @param {NodeJS.Signals} signal */
  const stop = signal => { try { if (process.platform === 'win32') child.kill(signal); else if (child.pid) process.kill(-child.pid, signal) } catch {} }
  /** @param {string} message */
  const terminate = message => { if (failure) return; failure = new Error(message); stop('SIGTERM'); killTimer = setTimeout(() => stop('SIGKILL'), 2000) }
  const timer = setTimeout(() => terminate(`${command}: exceeded ${timeout}ms`), timeout)
  child.stdout.on('data', chunk => { if (failure) return; stdout += chunk; if (Buffer.byteLength(stdout) > 1024 * 1024) terminate(`${command}: stdout limit exceeded`) })
  child.stderr.on('data', chunk => { if (failure) return; stderr += chunk; if (Buffer.byteLength(stderr) > 1024 * 1024) terminate(`${command}: stderr limit exceeded`) })
  child.once('error', error => { failure = error })
  child.once('close', code => {
    clearTimeout(timer); clearTimeout(killTimer)
    if (failure) reject(failure)
    else if (code !== 0) reject(new Error(`${command} exited ${code}:\n${stderr || stdout}`))
    else resolve(stdout)
  })
})
/** @type {Map<string,boolean>} */
const available = new Map()
/** @param {string} command */
const has = async command => {
  if (!available.has(command)) {
    try { await run(command, [['javac', 'kotlinc'].includes(command) ? '-version' : '--version'], { timeout: 15_000 }); available.set(command, true) }
    catch { available.set(command, false) }
  }
  return available.get(command)
}
/** @type {Record<string,string[]>} */
const requirements = { '.js': [bun], '.ts': [bun], '.py': ['python3'], '.php': ['php'], '.java': ['javac', 'java'], '.kt': ['kotlinc', 'java'], '.cs': ['dotnet'], '.rs': ['rustc'], '.rb': ['ruby'] }
/** @param {string} label @param {string} name @param {string[]} [extra] */
const supported = async (label, name, extra = []) => {
  const tools = requirements[path.extname(name)]
  if (!tools) { skip(label, `no execution checker for ${path.extname(name) || 'descriptive example'}`); return false }
  const missing = []
  for (const tool of [...tools, ...extra]) if (!await has(tool)) missing.push(tool)
  if (missing.length) {
    assert.notEqual(process.env.TAC_SNIPPETS_REQUIRE_TOOLCHAINS, '1', `${label}: required toolchain missing: ${missing.join(', ')}`)
    skip(label, `missing toolchain: ${missing.join(', ')}`)
    return false
  }
  return true
}
/** @param {string} root @param {string} name @param {string} code */
const write = (root, name, code) => {
  assert.ok(!path.isAbsolute(name) && !name.split(/[\\/]/).includes('..'), `Unsafe snippet filename: ${name}`)
  const file = path.join(root, name)
  mkdirSync(path.dirname(file), { recursive: true })
  writeFileSync(file, code)
}
/** @param {string} label @param {(root:string)=>Promise<void>} operation */
const temporary = async (label, operation) => {
  const root = mkdtempSync(path.join(tmpdir(), 'tachyon-snippet-'))
  try { await operation(root) } catch (error) { fail(label, error) }
  finally { rmSync(root, { recursive: true, force: true }) }
}
const placeholder = `export default class {
  title=''; message=''; state=''; status='ready'; count=0; label='';
  products=[]; pageCount=3; topic={slug:'a',title:'A'}; topics=[]; tabs=[];
  product={name:'n',price:1}; restocking=false; selected=null; $draft=''; $$theme='light';
  increment(){} select(){} refresh(){}
}`
/** @param {string} label @param {string} template @param {Snippet|null} [companion] @param {boolean} [components] */
const bundle = async (label, template, companion = null, components = false) => temporary(label, async root => {
  write(root, 'client/pages/tac.html', template)
  write(root, `client/pages/${companion?.name ?? 'tac.js'}`, companion?.code ?? placeholder)
  if (components) for (const name of ['product/card', 'sales/chart', 'heavy/editor', 'static/note']) {
    write(root, `client/components/${name}/tac.html`, '<article><h3>{name}</h3><slot /></article>')
    write(root, `client/components/${name}/tac.js`, 'export default class { name="" }')
  }
  await run(binary, ['bundle', root])
  pass(label, 'bundle')
})
/** @param {string} code */
const methods = code => [...new Set(code.match(/\b(?:GET|POST|PUT|PATCH|DELETE|OPTIONS|HEAD)(?=\s*\()/g) ?? ['GET'])]
/** @param {string} code */
const streaming = code => /(?:@Stream\b|#\[Stream\]|\[Stream\])/.test(code)
/** @param {string} name */
const routeOf = name => '/' + path.posix.dirname(name).replace(/^server\/routes\/?/, '').split('/').filter(Boolean).map(segment => segment.startsWith('_') ? 'snippet-42' : segment).join('/')
/** @param {Response} response */
const consume = async response => {
  const reader = response.body?.getReader()
  if (!reader) return ''
  let bytes = 0, text = ''
  const decoder = new TextDecoder()
  try {
    for (;;) {
      const { value, done } = await reader.read()
      if (done) break
      bytes += value.byteLength
      assert.ok(bytes <= 65536, 'Snippet HTTP response exceeds 64KiB')
      text += decoder.decode(value, { stream: true })
    }
    return text + decoder.decode()
  } finally { await reader.cancel().catch(() => {}) }
}
// Preview captures the whole server tree, stages the owned runtime and compiles
// reachable language layers. Direct single-source invocation cannot prove that.
/** @param {string} label @param {Snippet[]} files @param {Snippet} controller @param {(value:Record<string,unknown>)=>void} [expected] */
const execute = async (label, files, controller, expected) => {
  const extra = /@Relay|#\[Relay|\[Relay/.test(controller.code) ? ['ruby'] : []
  if (!await supported(label, controller.name, extra)) return
  await temporary(label, async root => {
    for (const file of files) write(root, file.name, file.code)
    await withServer(binary, ['preview', root, '--port', '0'], async request => {
      for (const method of methods(controller.code)) {
        const response = await request(routeOf(controller.name), { method,
          headers: { accept: streaming(controller.code) ? 'text/event-stream' : 'application/json', 'content-type': 'application/json' },
          ...(!['GET', 'HEAD', 'OPTIONS'].includes(method) ? { body: JSON.stringify({ name: 'snippet', sku: 'anvil', level: 4 }) } : {}),
        })
        const body = await consume(response)
        assert.ok(response.status >= 200 && response.status < 300, `${method} returned ${response.status}: ${body}`)
        if (streaming(controller.code)) {
          assert.match(response.headers.get('content-type') ?? '', /^text\/event-stream/)
          const events = body.split(/\r?\n\r?\n/).filter(event => /^data:/m.test(event))
          assert.ok(events.length >= 2, `Expected multiple SSE messages: ${body}`)
          for (const event of events) {
            assert.ok(!/^event:\s*error/m.test(event), event)
            const value = JSON.parse(event.split(/\r?\n/).filter(line => line.startsWith('data:')).map(line => line.slice(5).trim()).join('\n'))
            assert.equal(typeof value.sku, 'string'); assert.equal(typeof value.level, 'number')
          }
        } else if (method !== 'HEAD') {
          const value = JSON.parse(body)
          assert.ok(!Object.hasOwn(value, 'error'), body)
          if (expected && method === 'GET') expected(value)
        }
        pass(`${label} ${method}${streaming(controller.code) ? ' SSE' : ''}`, 'http')
      }
    })
  })
}
/** @param {string} label @param {string} name @param {string} code */
const syntax = async (label, name, code) => {
  if (!await supported(label, name)) return
  const extension = path.extname(name)
  if (['.java', '.kt', '.cs', '.rs'].includes(extension)) { skip(label, 'standalone layer fragment; full published language suites are compiled and executed separately'); return }
  await temporary(label, async root => {
    const file = `snippet${extension === '.js' ? '.mjs' : extension}`
    write(root, file, code)
    /** @type {[string,string[]]} */
    const args = extension === '.py' ? ['python3', ['-m', 'py_compile', file]]
      : extension === '.rb' ? ['ruby', ['-c', file]]
      : extension === '.php' ? ['php', ['-l', file]]
      : [bun, ['build', '--no-bundle', file, '--outfile', 'parsed.js']]
    await run(args[0], args[1], { cwd: root, timeout: 30_000 })
    pass(label, 'syntax')
  })
}
console.log(`Selected CLI: ${binary}`)
console.log((await run(binary, ['--version'], { timeout: 10_000 })).trim())
if (scope === 'all' || scope === 'features') {
  console.log('== feature snippets')
  for (const feature of features) for (const file of feature.files) {
    const label = `${feature.id} / ${file.name}`
    if (/^server\/routes\/.*\/yon\.(js|ts|py|php|java|kt|cs|rs)$/.test(file.name)) {
      const siblings = feature.files.filter(other => other.name.startsWith('server/') && !other.name.startsWith('server/routes/') && other !== file)
      await execute(label, [file, ...siblings], file)
    } else if (file.name.endsWith('.html')) {
      await bundle(label, file.code, null, feature.verify === 'tac-component' || /<product-card/.test(file.code))
    } else if (/^client\/.*\/tac\.(js|ts)$/.test(file.name)) {
      if (await supported(label, file.name)) await bundle(label, '<main>Snippet</main>', { name: path.basename(file.name), code: file.code })
    } else if (/\.json$/.test(file.name) || file.name === '.tachyonrc') {
      try { JSON.parse(file.code.replace(/^\s*\/\/.*$/gm, '')); pass(label, 'data') } catch (error) { fail(label, error) }
    } else if (/\.(js|ts|py|php|rb)$/.test(file.name) || file.name.startsWith('server/')) {
      await syntax(label, file.name, file.code)
    } else skip(label, /\.(rs|swift|kt|cs)$/.test(file.name) ? 'native companion; this website verifier does not compile native targets' : 'descriptive output, shell command, or stylesheet; not an executable snippet')
  }
}
if (scope === 'all' || scope === 'languages') {
  console.log('== language snippets')
  for (const file of languages.tac.files) {
    const label = `tac / ${file.name}`
    if (file.name.endsWith('.html')) await bundle(label, file.code)
    else if (/\/tac\.(js|ts)$/.test(file.name)) await bundle(label, '<main>Snippet</main>', { name: path.basename(file.name), code: file.code })
    else skip(label, 'non-browser companion or stylesheet; this verifier does not compile native targets')
  }
  for (const entry of languages.yon.entries) {
    const controller = entry.files.find(file => /^server\/routes\//.test(file.name))
    assert.ok(controller, `No controller in ${entry.id}`)
    await execute(`yon/${entry.id} layered suite (${entry.files.length} files)`, entry.files, controller, value => {
      assert.ok(Array.isArray(value.orders) && value.orders.length > 0, `Controller must reach its nonempty repository data: ${JSON.stringify(value)}`)
      assert.ok(JSON.stringify(value.orders).includes('anvil'), 'The published repository fixture must reach the HTTP response')
    })
  }
}
if (scope === 'all' || scope === 'guides') {
  console.log('== guide snippets')
  for (const [slug, topic] of Object.entries(docs.topics)) for (const section of topic.sections ?? []) {
    for (const file of section.files ?? []) {
      if (file.name.endsWith('.html')) await bundle(`${slug} / ${section.heading}`, file.code)
      else skip(`${slug} / ${section.heading} / ${file.name}`, 'guide excerpt without a complete execution fixture')
    }
    if (section.code?.trim().startsWith('export default class')) {
      await bundle(`${slug} / ${section.heading}`, '<main>Snippet</main>', { name: 'tac.js', code: section.code })
    } else if (section.code) skip(`${slug} / ${section.heading}`, 'guide command, layout, or excerpt; not a complete source file')
  }
}
console.log(`\n${counts.failed ? 'FAIL' : 'PASS'}: ${counts.bundle} bundles, ${counts.http} HTTP method probes, ${counts.syntax} syntax checks, ${counts.data} JSON checks; ${counts.skipped} explicitly unexecuted examples; ${counts.failed} failures.`)
process.exitCode = counts.failed ? 1 : 0
