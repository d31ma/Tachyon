// Entry module for a Dart companion. The Dart compiler emits a module that
// cannot be instantiated on its own and cannot export a function of its own,
// so this adapts it to the one thing the island runtime asks a glued companion
// for: tacInvoke(request) -> response, both JSON text. See ADR 0011.
//
// The compiler's own files sit beside this one under the component's name, so
// they are found relative to it rather than named at build time.
const glue = import.meta.url.replace(/\.mjs$/, '.dart.mjs')
const { compileStreaming, instantiate, invoke } = await import(glue)

const instance = await instantiate(
  compileStreaming(fetch(glue.replace(/\.mjs$/, '.wasm'))),
  {},
)
// main installs the entry point under this name and it is read back before any
// other module can run, so the global never outlives these three lines.
invoke(instance, '__tacEntry')
const entry = globalThis.__tacEntry
delete globalThis.__tacEntry
if (typeof entry !== 'function') {
  throw new TypeError('Tac Dart companion did not install its entry point.')
}

export const tacInvoke = (request) => entry(request)
