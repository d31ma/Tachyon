import { pathToFileURL } from 'node:url'
import { readFile } from 'node:fs/promises'

const MAX_FRAME_BYTES = 16 * 1024 * 1024
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
  protocolOutput.write(Buffer.concat([prefix, payload]), () => process.exit(exitCode))
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

async function invoke(request) {
  try {
    const sourceText = await readFile(pathToFileURL(source), 'utf8')
    const sourceUrl = `data:text/javascript;base64,${Buffer.from(sourceText).toString('base64')}`
    const module = await import(sourceUrl)
    const Handler = module.Handler ?? module.default
    if (typeof Handler !== 'function') {
      failure(request.request_id, 'TY2201', 'Module must export a Handler class.')
      return
    }
    if (request.operation === 'view.context') {
      const staticValues = {}
      for (const [name, descriptor] of Object.entries(
        Object.getOwnPropertyDescriptors(Handler),
      )) {
        if (['length', 'name', 'prototype'].includes(name)) continue
        if (!Object.hasOwn(descriptor, 'value')) continue
        if (!descriptor.enumerable) continue
        if (typeof descriptor.value === 'function') continue
        staticValues[name] = descriptor.value
      }
      const method = Handler.GET
      let responseValues = {}
      if (method !== undefined) {
        if (typeof method !== 'function') {
          failure(request.request_id, 'TY1501', 'Handler.GET must be a static function.')
          return
        }
        const result = await method.call(Handler, request)
        if (
          result === null
          || typeof result !== 'object'
          || Array.isArray(result)
          || Object.getPrototypeOf(result) !== Object.prototype
        ) {
          failure(request.request_id, 'TY1501', 'Handler.GET must return a plain object for view context.')
          return
        }
        responseValues = result
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
          data: JSON.stringify({
            static_values: staticValues,
            response_values: responseValues,
          }),
        },
      })
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
