import { spawn } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

const MAX_FRAME_BYTES = 16 * 1024 * 1024
const MAX_RELAY_STDOUT_BYTES = 16 * 1024 * 1024
const MAX_RELAY_STDERR_BYTES = 64 * 1024
const protocolOutput = process.stdout
const source = process.argv[2]
let buffered = Buffer.alloc(0)
let activeRequest = null
let settled = false

function sideband(...values) {
  const line = values.map((value) => {
    if (typeof value === 'string') return value
    try {
      return JSON.stringify(value)
    } catch {
      return String(value)
    }
  }).join(' ')
  process.stderr.write(`${line}\n`)
}

console.log = sideband
console.info = sideband
console.warn = sideband
console.error = sideband
console.debug = sideband

function publicMessage(error) {
  const value = error && typeof error.message === 'string'
    ? error.message
    : String(error ?? 'Handler failed.')
  return value.slice(0, 2048) || 'Handler failed.'
}

/**
 * Writes protocol bytes without allowing a fast handler to outrun stdout.
 *
 * A false return from Writable.write means both the write callback and the
 * later drain signal belong to this chunk. Either may arrive first, so the
 * promise resolves only after both and rejects once on any stream error.
 */
function writeOutput(chunk) {
  return new Promise((resolve, reject) => {
    let callbackDone = false
    let drainDone = false
    let settledWrite = false
    const cleanup = () => {
      protocolOutput.off('error', onError)
      protocolOutput.off('drain', onDrain)
    }
    const finish = () => {
      if (settledWrite || !callbackDone || !drainDone) return
      settledWrite = true
      cleanup()
      resolve()
    }
    const failWrite = (error) => {
      if (settledWrite) return
      settledWrite = true
      cleanup()
      reject(error)
    }
    const onError = (error) => failWrite(error)
    const onDrain = () => {
      drainDone = true
      finish()
    }
    protocolOutput.once('error', onError)
    try {
      const writable = protocolOutput.write(chunk, (error) => {
        if (error) {
          failWrite(error)
          return
        }
        callbackDone = true
        finish()
      })
      drainDone = writable
      if (!writable) protocolOutput.once('drain', onDrain)
      finish()
    } catch (error) {
      failWrite(error)
    }
  })
}

function writeFrame(envelope, exitCode = 0) {
  if (settled) return
  settled = true
  let payload
  try {
    payload = Buffer.from(JSON.stringify(envelope), 'utf8')
  } catch {
    payload = Buffer.from(JSON.stringify({
      protocol_version: 1,
      kind: 'response',
      request_id: activeRequest?.request_id ?? 'adapter_error',
      status: 500,
      headers: {},
      error: {
        code: 'TY2203',
        message: 'Handler returned a value that is not JSON-serializable.',
        retryable: false,
      },
    }), 'utf8')
  }
  if (payload.length > MAX_FRAME_BYTES) {
    payload = Buffer.from(JSON.stringify({
      protocol_version: 1,
      kind: 'response',
      request_id: activeRequest?.request_id ?? 'adapter_error',
      status: 500,
      headers: {},
      error: {
        code: 'TY2203',
        message: 'Serialized handler result exceeds the protocol frame limit.',
        retryable: false,
      },
    }), 'utf8')
  }
  const prefix = Buffer.allocUnsafe(4)
  prefix.writeUInt32BE(payload.length)
  void writeOutput(Buffer.concat([prefix, payload]))
    .then(() => process.exit(exitCode), () => process.exit(74))
}

/**
 * Writes one streamed event frame and keeps the process alive.
 *
 * `writeFrame` settles the request and exits, which is right for a single
 * answer and wrong for a stream: a generator yields many times before it is
 * done. This writes the same length-prefixed shape without ending anything.
 */
async function writeEvent(requestId, value) {
  let data
  try {
    data = JSON.stringify(value === undefined ? null : value)
  } catch {
    failure(requestId, 'TY2203', 'Streamed event is not JSON-serializable.')
    return false
  }
  const payload = Buffer.from(JSON.stringify({
    protocol_version: 1,
    kind: 'event',
    request_id: requestId,
    body: { encoding: 'utf8', data },
  }), 'utf8')
  if (payload.length > MAX_FRAME_BYTES) {
    failure(requestId, 'TY2203', 'Streamed event exceeds the protocol frame limit.')
    return false
  }
  const prefix = Buffer.allocUnsafe(4)
  prefix.writeUInt32BE(payload.length)
  try {
    await writeOutput(Buffer.concat([prefix, payload]))
  } catch {
    process.exit(74)
    return false
  }
  return true
}

/** Whether a handler answered with a stream of events rather than one body. */
function isStream(value) {
  return Boolean(value)
    && (typeof value[Symbol.asyncIterator] === 'function'
      || typeof value[Symbol.iterator] === 'function')
    && typeof value !== 'string'
    && !Array.isArray(value)
}

/** Drains a generator into event frames, then ends the stream by exiting. */
async function streamEvents(requestId, iterable) {
  try {
    for await (const event of iterable) {
      if (!await writeEvent(requestId, event)) return
    }
  } catch (error) {
    failure(requestId, 'TY2204', publicMessage(error))
    return
  }
  // End of stream is end of process: the reader takes EOF as the close, so
  // there is no terminator frame to keep in step with.
  try {
    await writeOutput(Buffer.alloc(0))
    process.exit(0)
  } catch {
    process.exit(74)
  }
}

function failure(requestId, code, message, status = 500) {
  writeFrame({
    protocol_version: 1,
    kind: 'response',
    request_id: requestId,
    status,
    headers: {},
    error: {
      code,
      message: String(message).slice(0, 2048) || 'Handler failed.',
      retryable: false,
    },
  })
}

function responseDescriptor(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return null
  if (!Object.hasOwn(value, 'headers')) return null
  const allowed = new Set(['status', 'headers', 'body'])
  if (Object.keys(value).some((name) => !allowed.has(name))) return null
  const status = value.status ?? 200
  if (!Number.isInteger(status) || status < 100 || status > 599) {
    throw new TypeError('Handler response status must be an integer from 100 through 599.')
  }
  if (value.headers === null || typeof value.headers !== 'object' || Array.isArray(value.headers)) {
    throw new TypeError('Handler response headers must be an object.')
  }
  const headers = {}
  for (const [name, raw] of Object.entries(value.headers)) {
    const values = Array.isArray(raw) ? raw : [raw]
    if (!values.length || values.some((item) => typeof item !== 'string')) {
      throw new TypeError(`Handler response header '${name}' must contain strings.`)
    }
    headers[name.toLowerCase()] = values
  }
  let body
  if (Object.hasOwn(value, 'body') && value.body !== undefined) {
    const data = typeof value.body === 'string'
      ? value.body
      : JSON.stringify(value.body)
    body = { encoding: 'utf8', data }
  }
  return { status, headers, body }
}

/**
 * Whether a handler source declares a decorator.
 *
 * Read from the file rather than inferred from the error, because the message
 * a runtime gives for unexpected `@` is its own and not worth matching on.
 */
function declaresDecorator(path) {
  try {
    return /^\s*@[A-Z]/m.test(readFileSync(path, 'utf8'))
  } catch {
    return false
  }
}

let controller

/**
 * A delegate that could not be run answers 502, the same as any other upstream
 * that did not reply. The reason travels in the body: a delegate is a program
 * on the host, and "it did not run" is not otherwise visible.
 */
function relayFailed(reason = 'Delegate invocation failed.') {
  return {
    status: 502,
    headers: { 'content-type': ['application/json'] },
    body: JSON.stringify({ error: reason }),
  }
}

function relayFailure(request, category) {
  sideband(JSON.stringify({
    event: 'handler.relay_failed',
    request_id: request?.request_id ?? 'unknown',
    category,
  }))
  return relayFailed()
}

async function invoke(request) {
  try {
    // The layer stereotypes, before the module that uses them is imported.
    //
    // Global rather than imported, because an import is ceremony for something
    // that does nothing: a stereotype is read by Tachyon before the handler
    // ever starts, and the value here only has to exist so the decorator
    // syntax resolves. Making a developer write an import line to satisfy a
    // marker is the kind of tax that gets the marker dropped.
    //
    // Assigned rather than declared, and only when absent: a project that
    // defines its own `Controller` keeps it.
    // Work handed to a language Yon does not run.
    //
    // Yon runs the eight languages that can declare a layer. Go, Ruby, Elixir
    // and the rest cannot, so they are not routes — but they are still
    // programs, and a program that speaks Handler Protocol v1 on standard
    // input and output is exactly what Yon spawns anyway.
    //
    // The command is explicit rather than inferred from the file name: a
    // compiled language has no interpreter to infer. The working directory is
    // the project root, so a project-relative path reads as written.
    //
    // Global for the same reason the stereotypes are: an import line to reach
    // something the runtime already supplies is a tax that gets it dropped.
    globalThis.relay ??= async (command, request) => {
      const [program, ...rest] = command ?? []
      if (!program) return relayFailure(request, 'start')
      let child
      try {
        child = spawn(program, rest, { stdio: ['pipe', 'pipe', 'pipe'] })
      } catch {
        return relayFailure(request, 'start')
      }
      const drain = (stream, limit, capture) => new Promise((resolve) => {
        const chunks = []
        let captured = 0
        let oversized = false
        stream.on('data', (chunk) => {
          const remaining = limit - captured
          if (remaining > 0 && capture) chunks.push(chunk.subarray(0, remaining))
          captured += Math.min(chunk.length, Math.max(remaining, 0))
          oversized ||= chunk.length > remaining
        })
        stream.on('end', () => resolve({ data: Buffer.concat(chunks), oversized }))
        stream.on('error', () => resolve({ data: Buffer.concat(chunks), oversized: true }))
      })
      const stdout = drain(child.stdout, MAX_RELAY_STDOUT_BYTES, true)
      const stderr = drain(child.stderr, MAX_RELAY_STDERR_BYTES, false)
      const closed = new Promise((resolve) => {
        child.once('error', () => resolve(null))
        child.once('close', (code) => resolve(code))
      })
      child.stdin.end(JSON.stringify(request))
      let timedOut = false
      const timeout = setTimeout(
        () => { timedOut = true; child.kill('SIGKILL') },
        Math.max(1, Number(request.deadline_ms ?? 30_000)),
      )
      const [status, stdoutResult, stderrResult] = await Promise.all([closed, stdout, stderr])
      clearTimeout(timeout)
      if (timedOut) return relayFailure(request, 'timeout')
      if (stdoutResult.oversized || stderrResult.oversized) return relayFailure(request, 'overflow')
      if (status !== 0) return relayFailure(request, 'exit')
      let envelope
      try {
        envelope = JSON.parse(stdoutResult.data.toString('utf8'))
      } catch {
        return relayFailure(request, 'protocol')
      }
      // Returned in the shape a handler may return directly, so the runner's
      // own descriptor check writes it — the delegate's status and headers
      // travel without this shim re-implementing the envelope.
      return {
        status: envelope.status ?? 200,
        headers: envelope.headers ?? {},
        body: envelope.body ?? '',
      }
    }
    // `@Relay('ruby', 'server/delegates/report.rb')` on a method makes that
    // method a proxy. The command is metadata about the method, so it belongs
    // in the declaration rather than in a body the reader has to open.
    //
    // JavaScript is one of the four languages where the decorator can do the
    // work itself: a stage-3 method decorator returns the function that
    // replaces the one it was written on, so nothing intercepts the call later.
    globalThis.Relay ??= (...command) => (_method, _context) =>
      function (request) {
        return relay(command, request)
      }
    // `@Stream` marks a method that answers more than once. It does nothing at
    // run time — `yield` in the body is what streams — and exists so the
    // server can decide which path a route takes before it calls the handler,
    // and so the two can be checked against each other before it is built.
    globalThis.Stream ??= (method) => method
    for (const layer of ['Controller', 'Service', 'Repository', 'Client', 'Delegate']) {
      globalThis[layer] ??= (target) => {
        // `@Controller` is how the handler class says it is the handler class,
        // so the stub remembers what it was put on. That is cheaper than
        // hunting for an export named `Handler`, and it is what lets the class
        // be called `OrdersController` — which the suffix rule requires.
        if (layer === 'Controller') controller = target
      }
    }
    // Imported from its own path, not from a data: URL. A data: URL has no
    // base to resolve against, so `import '../services/health.js'` fails with
    // a specifier error — which made it impossible for a handler to be one
    // layer of anything.
    try {
      await import(pathToFileURL(source).href)
    } catch (error) {
      // A decorator is the one syntax error worth naming, because the file is
      // not wrong — the runtime is. Node rejects `@Controller` outright and
      // says "Invalid or unexpected token", which points at the handler and
      // sends the reader looking for a typo that is not there.
      if (error instanceof SyntaxError && declaresDecorator(source)) {
        failure(
          request.request_id,
          'TY2201',
          `${publicMessage(error)} — this handler declares a decorator, which `
            + `Node cannot parse. Run it on Bun or Deno: set `
            + `YON_JAVASCRIPT_RUNTIME for a server, or pass `
            + `--javascript-runtime to ty handler invoke.`,
        )
        return
      }
      throw error
    }
    // Discovery already required the annotation. Runtime dispatch uses the
    // exact class the decorator marked and never revives the removed Handler
    // export fallback.
    const Handler = controller
    if (typeof Handler !== 'function') {
      failure(request.request_id, 'TY2201', 'Module must export a class carrying @Controller.')
      return
    }
    const method = Handler[request.method]
    if (typeof method !== 'function') {
      failure(
        request.request_id,
        'TY2202',
        `Handler does not define static ${request.method}().`,
        405,
      )
      return
    }
    const result = await method.call(Handler, request)
    // A generator is a stream: `yield` is how a handler says it has more than
    // one thing to send, in the language's own words rather than a framework
    // call.
    if (isStream(result)) {
      await streamEvents(request.request_id, result)
      return
    }
    const explicit = responseDescriptor(result)
    if (explicit) {
      writeFrame({
        protocol_version: 1,
        kind: 'response',
        request_id: request.request_id,
        status: explicit.status,
        headers: explicit.headers,
        body: explicit.body,
      })
      return
    }
    let data
    try {
      data = JSON.stringify(result === undefined ? null : result)
    } catch {
      failure(
        request.request_id,
        'TY2203',
        'Handler returned a value that is not JSON-serializable.',
      )
      return
    }
    writeFrame({
      protocol_version: 1,
      kind: 'response',
      request_id: request.request_id,
      status: 200,
      headers: {
        'content-type': ['application/json; charset=utf-8'],
      },
      body: {
        encoding: 'utf8',
        data,
      },
    })
  } catch (error) {
    failure(activeRequest.request_id, 'TY2201', publicMessage(error))
  }
}

function accept(envelope) {
  if (!envelope || envelope.protocol_version !== 1) process.exit(70)
  if (envelope.kind === 'cancel') {
    if (activeRequest && envelope.request_id === activeRequest.request_id) {
      failure(envelope.request_id, 'TY2111', 'Handler invocation was cancelled.', 499)
    }
    return
  }
  if (envelope.kind !== 'request' || activeRequest) process.exit(70)
  activeRequest = envelope
  void invoke(envelope)
}

process.stdin.on('data', (chunk) => {
  buffered = Buffer.concat([buffered, chunk])
  if (buffered.length > MAX_FRAME_BYTES + 4) process.exit(70)
  while (buffered.length >= 4) {
    const length = buffered.readUInt32BE(0)
    if (length > MAX_FRAME_BYTES) process.exit(70)
    if (buffered.length < length + 4) return
    const payload = buffered.subarray(4, length + 4)
    buffered = buffered.subarray(length + 4)
    let envelope
    try {
      envelope = JSON.parse(payload.toString('utf8'))
    } catch {
      process.exit(70)
    }
    accept(envelope)
  }
})

process.stdin.on('error', () => process.exit(74))
process.stdin.resume()
