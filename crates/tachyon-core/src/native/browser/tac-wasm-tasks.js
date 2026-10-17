/** @typedef {{answer: Promise<string>, cancel: () => void}} WasmHttpTicket */
/** @typedef {{start: (text: string) => WasmHttpTicket}} WasmHttpEndpoint */
/** @typedef {{memory: WebAssembly.Memory, tac_wasm_alloc: (length: number) => number, tac_wasm_dealloc: (pointer: number, length: number) => void, tac_http_settle: (ticket: number, pointer: number, length: number) => void}} WasmTaskExports */
class TacWasmTasks {
  /** @type {symbol} */
  static symbol = Symbol.for('tachyon.tac.http')
  /** @type {string} */
  static missing = '{"error":{"code":"HTTP_MISSING"}}'
  /** @type {TextEncoder} */
  #encoder = new TextEncoder()
  /** @type {WasmTaskExports | undefined} */
  #exports = undefined
  /** @type {Map<number, WasmHttpTicket>} */
  #tickets = new Map()
  /** @type {TacTaskAnswers} */
  answers = new TacTaskAnswers()
  /** @type {(owner: string, text: string) => Promise<string>} */
  sockets = async () => TacWasmTasks.missing

  /** @param {(pointer: number, length: number) => string} read @returns {Record<string, Function>} */
  imports(read) {
    return {
      tac_host_http_start: (ticket, pointer, length) => { this.#start(ticket, read(pointer, length)) },
      tac_host_http_cancel: (ticket) => { this.#cancel(ticket) },
      tac_host_socket_send: (ticket, pointer, length) => { this.#send(ticket, read(pointer, length)) },
      tac_host_task_done: (task, pointer, length) => { this.answers.done(task, read(pointer, length)) },
    }
  }

  /** @param {WasmTaskExports} exports @returns {void} */
  attach(exports) { this.#exports = exports }

  /** @param {number} ticket @param {string} text @returns {void} */
  #start(ticket, text) {
    /** @type {WasmHttpEndpoint | undefined} */
    const endpoint = globalThis[TacWasmTasks.symbol]
    const started = typeof endpoint?.start === 'function' ? endpoint.start(text) : { answer: Promise.resolve(TacWasmTasks.missing), cancel() {} }
    this.#hold(ticket, started)
  }

  /** @param {number} ticket @param {string} text @returns {void} */
  #send(ticket, text) {
    let owner = ''
    try { owner = String(JSON.parse(text).owner) } catch {}
    this.#hold(ticket, { answer: this.sockets(owner, text), cancel() {} })
  }

  /** @param {number} ticket @param {WasmHttpTicket} started @returns {void} */
  #hold(ticket, started) {
    this.#tickets.set(ticket, started)
    started.answer.then((answer) => this.#settle(ticket, answer), () => this.#settle(ticket, TacWasmTasks.missing))
  }

  /** @param {number} ticket @returns {void} */
  #cancel(ticket) {
    const started = this.#tickets.get(ticket)
    this.#tickets.delete(ticket)
    started?.cancel()
  }

  /** @param {number} ticket @param {string} answer @returns {void} */
  #settle(ticket, answer) {
    if (!this.#tickets.delete(ticket) || !this.#exports) return
    const bytes = this.#encoder.encode(answer)
    const pointer = this.#exports.tac_wasm_alloc(bytes.length)
    try {
      new Uint8Array(this.#exports.memory.buffer, pointer, bytes.length).set(bytes)
      this.#exports.tac_http_settle(ticket, pointer, bytes.length)
    } finally {
      this.#exports.tac_wasm_dealloc(pointer, bytes.length)
    }
  }
}
