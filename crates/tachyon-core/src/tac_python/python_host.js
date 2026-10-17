class TacPythonHost {
  static async start(program, hosts) {
    const root = new URL('./python/314.0.7/', import.meta.url)
    const loader = await import(new URL('pyodide.mjs', root).href)
    const runtime = await loader.loadPyodide({ indexURL: root.href, lockFileURL: new URL('pyodide-lock.json', root).href, stdLibURL: new URL('python_stdlib.zip', root).href })
    runtime.runPython(program)
    const type = runtime.globals.get('_TacPythonRuntime')
    return TacPythonHost.retain(type, hosts)
  }

  static retain(type, hosts) {
    try {
      type.connect(hosts?.emit, hosts?.done, hosts?.storage, hosts?.http)
      return TacPythonHost.copyMethods(type)
    } finally {
      type.destroy()
    }
  }

  static copyMethods(type) {
    let invoke
    let shutdown
    try {
      invoke = type.invoke.copy()
      shutdown = type.shutdown.copy()
      return new TacPythonHost(invoke, shutdown)
    } catch (error) {
      TacPythonHost.destroyMethods(invoke, shutdown)
      throw error
    }
  }

  static destroyMethods(invoke, shutdown) {
    try {
      invoke?.destroy()
    } finally {
      shutdown?.destroy()
    }
  }

  constructor(invoke, shutdown) {
    this.invoke = invoke
    this.shutdown = shutdown
    this.disposed = false
    this.pageHide = (event) => this.pageHidden(event)
    addEventListener('pagehide', this.pageHide)
  }

  exchange(text) {
    this.requireActive()
    return this.invoke(text)
  }

  pageHidden(event) {
    this.releaseForPage(event.persisted)
  }

  requireActive() {
    if (this.disposed) throw new Error('Python Tac host is disposed.')
  }

  releaseForPage(persisted) {
    if (!persisted && !this.disposed) this.dispose()
  }

  dispose() {
    this.releaseOnce()
  }

  releaseOnce() {
    if (!this.disposed) this.release()
  }

  release() {
    this.disposed = true
    removeEventListener('pagehide', this.pageHide)
    try {
      this.shutdown()
    } finally {
      TacPythonHost.destroyMethods(this.invoke, this.shutdown)
    }
  }
}

const taskAnswers = new TacTaskAnswers()
const pythonHttpMissing = () => ({ answer: Promise.resolve('{"error":{"code":"HTTP_MISSING"}}'), cancel() {} })
const pythonHost = await TacPythonHost.start(__TAC_PYTHON_PROGRAM__, {
  emit: (payload) => { published.push(payload); queueMicrotask(() => deliver()) },
  done: (task, text) => { taskAnswers.done(task, text) },
  storage: (text) => storageHost ? storageHost(text) : JSON.stringify({ error: 'Tachyon storage is not on this page.' }),
  http: (text) => (globalThis[Symbol.for('tachyon.tac.http')]?.start ?? pythonHttpMissing)(text),
})
const transport = (text) => pythonHost.exchange(text)
