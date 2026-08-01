// Entry module for a C# companion. The .NET wasm publish is a bundle rather
// than a module — a runtime, its assemblies, and a loader — so this boots it
// and adapts its one export to what the island runtime asks a glued companion
// for: tacInvoke(request) -> response, both JSON text. See ADR 0011.
const bundle = import.meta.url.replace(/\.mjs$/, '')
const { dotnet } = await import(`${bundle}/_framework/dotnet.js`)

const runtime = await dotnet.create()
const companion = await runtime.getAssemblyExports(runtime.getConfig().mainAssemblyName)
if (typeof companion?.Tac?.Invoke !== 'function') {
  throw new TypeError('Tac C# companion did not export its entry point.')
}

export const tacInvoke = (request) => companion.Tac.Invoke(request)
