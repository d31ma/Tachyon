// Entry module for a Kotlin companion. Kotlin/Wasm exports its members through
// the JavaScript module the compiler emits beside the wasm, so this adapts them
// to the one thing the island runtime asks a glued companion for:
// tacInvoke(request) -> response, both JSON text. See ADR 0011.
//
// The values themselves never become JSON on the Kotlin side: they cross as
// JavaScript values, and the text ends here.
const glue = import.meta.url.replace(/\.mjs$/, '.kotlin.mjs')
const companion = await import(glue)

const names = (list) => (list ? String(list).split(',').filter(Boolean) : [])
const answer = (value) => JSON.stringify({ value: value ?? null })

export const tacInvoke = (raw) => {
  const request = JSON.parse(raw)
  try {
    switch (request.op) {
      case 'init':
        return answer({ fields: names(companion.tacFields()), methods: names(companion.tacMethods()) })
      case 'get':
        return answer(companion.tacGet(request.name))
      case 'set':
        companion.tacSet(request.name, request.value)
        return answer(null)
      case 'call':
        return answer(companion.tacCall(request.name, request.args ?? []))
      default:
        return JSON.stringify({ error: `Unknown companion operation: ${request.op}` })
    }
  } catch (error) {
    return JSON.stringify({ error: String(error?.message ?? error) })
  }
}
