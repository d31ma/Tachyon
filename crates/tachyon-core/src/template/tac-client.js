const planElement = document.getElementById('tachyon-view')
if (!planElement) throw new Error('Tac client view plan is missing.')
const plan = JSON.parse(planElement.textContent || '{}')
const instances = new Map()
const componentRecords = new Map()
const lifecycleStarted = new Set()
const controllers = new Map()
let pageModule = {}
let pageOwner = Object.create(null)
let rendering = null
let renderedNodes = 0

// Interpret the compiler's bounded expression AST. Authored expression source
// never reaches the browser and is never passed to eval or Function.
const evaluate = async (node, environment) => {
  if (!node || typeof node !== 'object') return undefined
  switch (node.k) {
    case 'await': return await evaluate(node.e, environment)
    case 'lit': return node.v
    case 'id': {
      if (Object.prototype.hasOwnProperty.call(environment.locals, node.n)) {
        return environment.locals[node.n]
      }
      if (environment.owner && node.n in environment.owner) return environment.owner[node.n]
      return pageModule[node.n]
    }
    case 'get': {
      const target = await evaluate(node.o, environment)
      return target === null || target === undefined ? undefined : target[node.p]
    }
    case 'idx': {
      const target = await evaluate(node.o, environment)
      return Array.isArray(target) ? target[node.i] : undefined
    }
    case 'not': return !await evaluate(node.e, environment)
    case 'cmp': {
      const left = await evaluate(node.l, environment)
      const right = await evaluate(node.r, environment)
      if (node.op === 'eq') return left === right
      if (node.op === 'ne') return left !== right
      if (node.op === 'lt') return left < right
      if (node.op === 'le') return left <= right
      if (node.op === 'gt') return left > right
      return left >= right
    }
    case 'log': {
      const left = await evaluate(node.l, environment)
      if (node.op === 'and') return left ? await evaluate(node.r, environment) : left
      return left || await evaluate(node.r, environment)
    }
    case 'num': {
      const left = await evaluate(node.l, environment)
      const right = await evaluate(node.r, environment)
      if (node.op === 'add') return left + right
      if (node.op === 'sub') return left - right
      if (node.op === 'mul') return left * right
      return left / right
    }
    case 'if':
      return await evaluate(node.c, environment)
        ? await evaluate(node.t, environment)
        : await evaluate(node.f, environment)
    case 'call': {
      const callee = node.c
      const owner = callee.k === 'get' || callee.k === 'idx'
        ? await evaluate(callee.o, environment)
        : environment.owner
      const target = await evaluate(callee, environment)
      if (typeof target !== 'function') return undefined
      const arguments_ = await Promise.all((node.a || []).map((value) => evaluate(value, environment)))
      return target.apply(owner, arguments_)
    }
    default: return undefined
  }
}

const truthy = (value) => Boolean(value)
const display = (value) => {
  if (value === null || value === undefined || typeof value === 'object') return ''
  return String(value)
}
const childEnvironment = (environment, locals, owner = environment.owner) => ({
  owner,
  locals: Object.assign(Object.create(environment.locals), locals),
})
const ownedEnvironment = (owner, locals = {}) => ({ owner, locals: Object.assign(Object.create(null), locals) })

const resolveEventArgument = async (argument, event, environment) => {
  if ('value' in argument) return argument.value
  if ('expression' in argument) return evaluate(argument.expression, environment)
  if (!('event' in argument) || argument.event === '') return event
  let value = event
  for (const segment of String(argument.event).split('.')) {
    if (value === null || value === undefined) return undefined
    value = value[segment]
  }
  return value
}

const bindEvent = (element, type, binding, environment) => {
  element.addEventListener(type, async (event) => {
    try {
      const arguments_ = await Promise.all(
        (binding.arguments || []).map((argument) => resolveEventArgument(argument, event, environment)),
      )
      if (binding.assign) {
        const { target, operator } = binding.assign
        const current = environment.owner?.[target]
        const value = arguments_[0]
        environment.owner[target] =
          operator === '+=' ? current + value
          : operator === '-=' ? current - value
          : operator === '*=' ? current * value
          : operator === '/=' ? current / value
          : value
        await render()
      } else {
        const ownerHandler = environment.owner?.[binding.handler]
        const handler = ownerHandler ?? pageModule[binding.handler]
        if (typeof handler !== 'function') {
          throw new TypeError(`Tac handler '${binding.handler}' is not available.`)
        }
        await handler.call(environment.owner, event, ...arguments_)
        if (ownerHandler) await render()
      }
      document.documentElement.dataset.tachyonEvents = 'handled'
    } catch (error) {
      document.documentElement.dataset.tachyonEventError = 'handler_failed'
      console.error('[Tac event]', String(error?.message || error).slice(0, 512))
    }
  })
}

const applyAttributes = async (element, attributes, environment) => {
  for (const attribute of attributes || []) {
    if (attribute.event) {
      bindEvent(element, attribute.eventType, attribute.event, environment)
      continue
    }
    const value = 'expression' in attribute
      ? await evaluate(attribute.expression, environment)
      : attribute.value
    if (value === false || value === null || value === undefined) {
      element.removeAttribute(attribute.name)
      continue
    }
    const rendered = value === true ? '' : display(value)
    element.setAttribute(attribute.name, rendered)
    if (['checked', 'disabled', 'selected'].includes(attribute.name)) element[attribute.name] = Boolean(value)
    else if (attribute.name === 'value' && 'value' in element) element.value = rendered
  }
}

const importAsset = (raw) => {
  const url = new URL(raw, location.href)
  const web = url.origin === location.origin && url.pathname.startsWith('/.tachyon/components/')
  const native = url.origin === location.origin && url.pathname.includes('/WebBundle/tachyon-runtime/components/')
  if (!web && !native) throw new TypeError('Tac component asset is not a generated same-origin URL.')
  return import(url.href)
}

const wasi = new Proxy({}, {
  get: (_, name) => name === 'proc_exit'
    ? () => { throw new Error('Tac Wasm companion exited.') }
    : () => 0,
})
const wasmOwner = async (raw, properties) => {
  const url = new URL(raw, location.href)
  let invoke
  if (url.pathname.endsWith('.mjs')) {
    const module = await importAsset(url.href)
    if (typeof module.tacInvoke !== 'function') throw new TypeError('Tac Wasm glue must export tacInvoke.')
    invoke = (request) => JSON.parse(module.tacInvoke(JSON.stringify(request)))
  } else {
    const response = await fetch(url)
    const { instance } = await WebAssembly.instantiate(await response.arrayBuffer(), { wasi_snapshot_preview1: wasi })
    const { memory, tac_alloc: allocate, tac_invoke: call, _initialize: initialize } = instance.exports
    if (!memory || typeof allocate !== 'function' || typeof call !== 'function') {
      throw new TypeError('Tac Wasm companion has an invalid ABI.')
    }
    if (typeof initialize === 'function') initialize()
    const encoder = new TextEncoder()
    const decoder = new TextDecoder()
    invoke = (request) => {
      const input = encoder.encode(JSON.stringify(request))
      const pointer = allocate(input.length)
      new Uint8Array(memory.buffer, pointer, input.length).set(input)
      const packed = BigInt(call(pointer, input.length))
      const outputPointer = Number(packed >> 32n)
      const outputLength = Number(packed & 0xffffffffn)
      return JSON.parse(decoder.decode(new Uint8Array(memory.buffer, outputPointer, outputLength)))
    }
  }
  const request = (value) => {
    const answer = invoke(value)
    if (answer?.error) throw new Error(answer.error)
    return answer?.value
  }
  const members = request({ op: 'init', props: properties }) || {}
  const fields = new Set(members.fields || [])
  const methods = new Set(members.methods || [])
  return new Proxy({}, {
    has: (_, name) => fields.has(name) || methods.has(name),
    get: (_, name) => fields.has(name) ? request({ op: 'get', name })
      : methods.has(name) ? (...args) => request({ op: 'call', name, args })
      : undefined,
    set: (_, name, value) => { request({ op: 'set', name, value }); return true },
  })
}

const componentOwner = async (node, properties, path) => {
  if (instances.has(path)) return instances.get(path)
  let owner = properties
  if (node.wasm) owner = await wasmOwner(node.wasm, properties)
  else if (node.module) {
    const module = await importAsset(node.module)
    if (typeof module.default !== 'function') throw new TypeError(`Tac component '${node.name}' must export a default class.`)
    owner = new module.default(properties)
  }
  instances.set(path, owner)
  componentRecords.set(path, { node, properties, owner })
  return owner
}

const snapshotState = (owner) => {
  const state = {}
  for (const [name, value] of Object.entries(owner || {})) {
    if (typeof value !== 'function') state[name] = value
  }
  try { return structuredClone(state) } catch { return {} }
}

const snapshotMutableDom = () => ({
  active: document.activeElement?.id || null,
  elements: [...document.querySelectorAll('[id]')].slice(0, 2048).map((element) => ({
    id: element.id,
    open: element instanceof HTMLDetailsElement ? element.open : undefined,
    value: element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement ? element.value : undefined,
    checked: element instanceof HTMLInputElement ? element.checked : undefined,
    scroll: element.scrollLeft || element.scrollTop ? [element.scrollLeft, element.scrollTop] : undefined,
  })),
})
const restoreMutableDom = (state) => {
  for (const saved of state.elements) {
    const element = document.getElementById(saved.id)
    if (!element) continue
    if (saved.open !== undefined && element instanceof HTMLDetailsElement) element.open = saved.open
    if (saved.value !== undefined && (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement)) element.value = saved.value
    if (saved.checked !== undefined && element instanceof HTMLInputElement) element.checked = saved.checked
    if (saved.scroll) element.scrollTo(saved.scroll[0], saved.scroll[1])
  }
  if (state.active) document.getElementById(state.active)?.focus({ preventScroll: true })
}

const hotUpdate = async (boundaries, version) => {
  const selected = new Set(boundaries || [])
  const domState = snapshotMutableDom()
  for (const [path, record] of componentRecords) {
    if (!selected.has(record.node.name)) continue
    const state = typeof record.owner.hotState === 'function'
      ? structuredClone(await record.owner.hotState())
      : snapshotState(record.owner)
    const asset = record.node.module || record.node.wasm
    if (!asset) continue
    const url = new URL(asset, location.href)
    url.searchParams.set('tachyon_hot', version || String(Date.now()))
    let owner
    if (record.node.wasm) owner = await wasmOwner(url.href, record.properties)
    else {
      const module = await importAsset(url.href)
      if (typeof module.default !== 'function') throw new TypeError(`Tac component '${record.node.name}' must export a default class.`)
      owner = new module.default(record.properties)
    }
    if (typeof record.owner.hotDispose === 'function') await record.owner.hotDispose()
    if (typeof owner.restoreHotState === 'function') await owner.restoreHotState(state)
    else Object.assign(owner, state)
    controllers.get(path)?.abort()
    controllers.delete(path)
    lifecycleStarted.delete(path)
    record.owner = owner
    instances.set(path, owner)
  }
  await render()
  restoreMutableDom(domState)
}

const mountComponent = (host, owner, policy, key) => {
  if (!policy || policy === 'never') return
  const activate = async () => {
    try {
      controllers.get(key)?.abort()
      const controller = new AbortController()
      controllers.set(key, controller)
      host.tachyonComponent = { instance: owner, refresh: render, controller }
      if (typeof owner.mount === 'function') await owner.mount(host, controller.signal)
      // Compatibility only: this runs after browser rendering and adopts no SSR.
      else if (typeof owner.hydrate === 'function') await owner.hydrate(host, controller.signal)
      if (!lifecycleStarted.has(key)) {
        lifecycleStarted.add(key)
        for (const method of owner.constructor?.__tachyonOnMount || []) {
          if (typeof owner[method] === 'function') await owner[method]()
        }
      }
      host.dataset.tachyonActive = 'true'
      host.removeAttribute('data-tachyon-mount-error')
      return true
    } catch (error) {
      host.dataset.tachyonActive = 'false'
      host.dataset.tachyonMountError = 'activation_failed'
      console.error('[Tac component mount]', String(error?.message || error).slice(0, 512))
      return false
    }
  }
  if (policy === 'idle') (globalThis.requestIdleCallback || ((fn) => setTimeout(fn, 1)))(activate)
  else if (policy === 'visible' && globalThis.IntersectionObserver) {
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) { observer.disconnect(); void activate() }
    }, { rootMargin: '100px' })
    observer.observe(host)
  } else if (policy === 'interaction') {
    const listener = async (event) => {
      if (event.cancelable) event.preventDefault()
      event.stopImmediatePropagation()
      host.removeEventListener('pointerdown', listener, true)
      host.removeEventListener('keydown', listener, true)
      const target = event.target
      if (await activate()) {
        target.dispatchEvent(new Event(event.type, { bubbles: true, cancelable: true }))
      }
    }
    host.addEventListener('pointerdown', listener, true)
    host.addEventListener('keydown', listener, true)
  } else queueMicrotask(activate)
}

const renderText = async (parts, environment) => {
  let value = ''
  for (const part of parts || []) {
    value += 'expression' in part ? display(await evaluate(part.expression, environment)) : part.value
  }
  return document.createTextNode(value)
}

const renderComponent = async (node, environment, path) => {
  const properties = {}
  for (const property of node.properties || []) {
    if (property.event) continue
    properties[property.name] = 'expression' in property
      ? await evaluate(property.expression, environment)
      : property.value
  }
  const componentKey = `${path}:${node.name}`
  const owner = await componentOwner(node, properties, componentKey)
  for (const [name, value] of Object.entries(properties)) {
    if (!(name in owner)) owner[name] = value
  }
  const host = document.createElement('tachyon-component')
  host.dataset.tachyonComponent = node.name
  host.dataset.tachyonMount = node.mount || 'none'
  if (node.scope) host.setAttribute('data-tac-scope', node.name)
  const componentEnvironment = ownedEnvironment(owner)
  host.append(await renderNodes(node.template || [], componentEnvironment, `${path}.template`, {
    nodes: node.slot || [],
    environment,
  }))
  host.tachyonComponent = { instance: owner, refresh: render }
  mountComponent(host, owner, node.mount, componentKey)
  return host
}

const renderNode = async (node, environment, path, slot) => {
  renderedNodes += 1
  if (renderedNodes > 100000) throw new RangeError('Tac client view exceeds 100,000 nodes.')
  if (node.k === 'text') return renderText(node.parts, environment)
  if (node.k === 'comment') return document.createComment(node.value || '')
  if (node.k === 'slot') {
    return slot ? renderNodes(slot.nodes, slot.environment, `${path}.slot`, null) : document.createDocumentFragment()
  }
  if (node.k === 'iteration') {
    const fragment = document.createDocumentFragment()
    const values = await evaluate(node.iterable, environment)
    if (!Array.isArray(values)) return fragment
    if (values.length > 10000) throw new RangeError('Tac iteration exceeds 10,000 items.')
    for (let index = 0; index < values.length; index += 1) {
      const local = childEnvironment(environment, { [node.binding]: values[index], $index: index })
      fragment.append(await renderNodes(node.children || [], local, `${path}.${index}`, slot))
    }
    return fragment
  }
  if (node.k === 'switch') {
    const value = await evaluate(node.value, environment)
    let fallback
    for (const candidate of node.children || []) {
      if (candidate.k !== 'case') continue
      if (!candidate.when) fallback = candidate
      else if (value === await evaluate(candidate.when, environment)) {
        return renderNodes(candidate.children || [], environment, `${path}.case`, slot)
      }
    }
    return fallback
      ? renderNodes(fallback.children || [], environment, `${path}.default`, slot)
      : document.createDocumentFragment()
  }
  if (node.k === 'component') return renderComponent(node, environment, path)
  if (node.k !== 'element') return document.createDocumentFragment()
  const element = document.createElement(node.tag)
  await applyAttributes(element, node.attributes, environment)
  if (!node.void) element.append(await renderNodes(node.children || [], environment, `${path}.children`, slot))
  return element
}

const renderNodes = async (nodes, environment, path, slot) => {
  const fragment = document.createDocumentFragment()
  for (let index = 0; index < nodes.length; index += 1) {
    const node = nodes[index]
    if (node.k === 'conditional') {
      if (node.branch !== 'if') continue
      let cursor = index
      let selected
      while (cursor < nodes.length) {
        const candidate = nodes[cursor]
        if (cursor > index && (candidate.k === 'comment'
          || (candidate.k === 'text' && candidate.parts?.every((part) => 'value' in part && !part.value.trim())))) {
          cursor += 1
          continue
        }
        if (candidate.k !== 'conditional') break
        if (!candidate.condition || truthy(await evaluate(candidate.condition, environment))) {
          selected = candidate
          cursor += 1
          break
        }
        cursor += 1
      }
      if (selected) fragment.append(await renderNodes(selected.children || [], environment, `${path}.${index}.branch`, slot))
      index = cursor - 1
      continue
    }
    fragment.append(await renderNode(node, environment, `${path}.${index}`, slot))
  }
  return fragment
}

const planElementByTag = (nodes, tag) => nodes.find((node) => node.k === 'element' && node.tag === tag)
const renderDocument = async (environment) => {
  renderedNodes = 0
  const htmlPlan = planElementByTag(plan.nodes || [], 'html')
  const headPlan = htmlPlan && planElementByTag(htmlPlan.children || [], 'head')
  const bodyPlan = htmlPlan && planElementByTag(htmlPlan.children || [], 'body')
  const runtimeAssets = [...document.head.querySelectorAll('[data-tachyon-runtime]')]
  if (htmlPlan) await applyAttributes(document.documentElement, htmlPlan.attributes, environment)
  if (headPlan) {
    const head = await renderNodes(headPlan.children || [], environment, 'head', null)
    document.head.replaceChildren(head, ...runtimeAssets)
  }
  const bodyNodes = bodyPlan ? bodyPlan.children || [] : (htmlPlan ? [] : plan.nodes || [])
  document.body.replaceChildren(await renderNodes(bodyNodes, environment, 'body', null))
}

const render = async () => {
  if (rendering) return rendering
  rendering = (async () => {
    const focus = document.activeElement?.id
    const scroll = { x: scrollX, y: scrollY }
    await renderDocument(ownedEnvironment(pageOwner))
    if (focus) document.getElementById(focus)?.focus({ preventScroll: true })
    scrollTo(scroll.x, scroll.y)
    document.documentElement.dataset.tachyonRendered = 'client'
  })().finally(() => { rendering = null })
  return rendering
}

if (plan.module) {
  pageModule = await import(new URL(plan.module, location.href).href)
  if (typeof pageModule.default === 'function') pageOwner = new pageModule.default()
}
for (const [name, value] of Object.entries(plan.state || {})) {
  if (!(name in pageOwner)) pageOwner[name] = value
}
for (const method of pageOwner.constructor?.__tachyonOnMount || []) {
  if (typeof pageOwner[method] === 'function') await pageOwner[method]()
}
Object.defineProperty(globalThis, '__tachyonTac', {
  configurable: true,
  value: Object.freeze({ render, hotUpdate, instance: pageOwner, instances }),
})
globalThis.__tc_rerender = render
addEventListener('tachyon:rerender', render)
await render()
