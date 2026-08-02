# 0011 — One wasm ABI for companions in any language

## Status

Accepted, and amended below for toolchains that emit only WasmGC. Implements
the route recorded in `PARITY_LEDGER.md` for polyglot browser companions.

## Context

A browser companion in Rust, Kotlin, Swift, C#, or Dart cannot run in a browser
as written. The legacy implementation solves this by hand-writing a subset
transpiler per language: five partial language implementations, each diverging
from the language it claims to compile, all to be maintained forever.

This implementation has twice refused that trade. TypeScript is emitted by
`tsc` rather than reimplemented, so the reference implementation defines the
semantics. Polyglot handlers speak one direct protocol — read one JSON request
from standard input, write one JSON response to standard output — so there is
no adapter per language at all, and adding a language is a line in
`.tachyonrc`.

Every one of these languages already compiles to WebAssembly through its own
toolchain. The open question was never *whether* they can reach the browser; it
was what interface Tachyon asks them to present once they are there.

## Decision

A wasm companion exports **three** things, and nothing else:

| Export | Signature | Purpose |
| --- | --- | --- |
| `memory` | standard linear memory | where requests and responses live |
| `tac_alloc` | `(i32) -> i32` | reserve `n` bytes, return the offset |
| `tac_invoke` | `(i32, i32) -> i64` | one JSON request in, one JSON response out |

`tac_invoke` receives the offset and length of a UTF-8 JSON request and returns
the offset and length of a UTF-8 JSON response, packed as `offset << 32 | len`.

The request is one of four operations, mirroring exactly what an island already
does with a JavaScript companion:

```json
{"op":"init","props":{}}
{"op":"get","name":"label"}
{"op":"set","name":"count","value":7}
{"op":"call","name":"loadingState","args":[]}
```

The response is `{"value": …}` or `{"error": "…"}`.

`init` answers with the module's members —
`{"fields":["count"],"methods":["doubled"]}` — so the host never guesses
whether a name is a field or a method. Without that a read of an absent field
returns a callable rather than `undefined`, which is not how a plain object
behaves and hides a typo behind a call that fails later.

This is the direct protocol, moved from a process boundary to a memory
boundary. The insight is the same one that removed eleven handler adapters:
**one contract simple enough that any language satisfies it directly** beats a
per-language adapter, because the contract is the thing that does not have to
be written five times.

The host is a proxy. The island runtime wraps a wasm module in an object whose
property reads become `get`, whose assignments become `set`, and whose method
calls become `call`, so a wasm companion and a JavaScript companion are
indistinguishable to everything above them: deferred expressions, event
bindings, assignment, and refresh all work unchanged.

## Consequences

- **No bindgen, no component model, no glue crate.** A language needs only the
  ability to export two functions and read its own linear memory. That is the
  floor of what a wasm target provides, so support tracks the language's own
  toolchain rather than a binding generator's coverage.
- **JSON at the boundary.** Values cross as text, which costs a serialise and a
  parse per access. An island's expression list is small and evaluated at
  hydration and after events, not per frame, so the cost is bounded by
  interaction rather than by rendering.
- **A companion in wasm cannot touch the DOM.** It computes; the island renders.
  That is already true of a JavaScript companion under ADR 0010, where the
  template owns the DOM and the companion owns the values.
- **Toolchains are a build requirement**, reported by `ty doctor` per language
  before anything is compiled.
- **Module size is fine, once symbols are stripped.** The first measurement was
  819 KB for a trivial fixture, which would have killed the route. Almost all
  of it was symbol names and metadata rather than code: `-C strip=symbols`
  brings the same fixture to 21 KB, and a module that avoids `core::fmt` to
  4 KB. Emission therefore always strips, and the probe asserts a 64 KiB
  budget so a regression is caught rather than noticed.
- **A `tac.rs` cannot serve both implementations.** The legacy bundler refuses
  a Rust companion that declares imports, because it feeds a subset
  transpiler; a wasm companion is real Rust and needs them. Rewriting an
  existing companion to this ABI therefore breaks the legacy build, so it is a
  cutover-time migration rather than something to do ahead of it.
- `hydrate` is not part of the ABI. A wasm companion supplies values; a
  companion that needs to drive the DOM imperatively is JavaScript or
  TypeScript.

## Rejected alternatives

**The WebAssembly Component Model with WIT.** The standards-track answer, and
the right one eventually. Rejected for now because it requires a binding
generator per language and its coverage across these five is uneven, which
reintroduces exactly the per-language dependency this decision removes. The ABI
above is a subset a component-model host can also satisfy later.

**A subset transpiler per language**, as the legacy implementation does.
Rejected above.

**Numeric-only exports with per-type accessors.** Avoids JSON, but needs a
distinct export per field and per type, so the module's surface grows with the
companion instead of staying fixed at three.

## Amendment: a second module shape, for toolchains that emit only WasmGC

Extending this ABI past Rust established that the three exports above are not a
property of WebAssembly. They are a property of a toolchain that compiles to
**linear memory**. Measured, not assumed:

| Toolchain | Exports `memory` | Imports | Bare instantiation |
| --- | --- | --- | --- |
| `rustc` → wasm32-unknown-unknown | yes | none | works |
| `dart compile wasm` 3.6.0 | **no**, 46 exports and none of them memory | 56, from `dart2wasm` and `wasm:js-string` | impossible |
| `kotlinc-js -Xwasm` 2.1.10 | yes | `js_code`, from the module it emits beside the wasm | impossible |
| .NET 9 `browser-wasm` publish | not a module at all — a runtime, the companion's assemblies, and a loader | many | impossible |
| swift.org `swiftc` → wasm32-unknown-wasip1 | yes | 14 `wasi_snapshot_preview1` | works, given an environment |

Dart is emphatic about it: `@pragma('wasm:export')` is rejected in user code as
"for internal use only", so a Dart companion cannot declare a wasm export at
all. Kotlin exports its linear memory but still imports the JavaScript the
compiler emits alongside, so the module does not start on its own either.
Neither is a gap that will close; both compilers target WasmGC by design and
ship a loader with every module.

A companion in one of those languages therefore presents a **glued module**:

> a JavaScript module exporting `tacInvoke(request) -> response`, where both are
> the same JSON text the bare ABI passes through linear memory.

Everything else is unchanged — the same four operations, the same `init`
member list, the same proxy above it. The island runtime chooses by the
extension of the asset it was given: `.wasm` is instantiated with no imports
and driven through `memory`, `.mjs` is imported and driven through `tacInvoke`.
That is one branch, and the same JSON on both sides of it.

The glue is the language toolchain's own output, not something this
implementation writes: `dart compile wasm` emits its loader, `kotlinc-js` emits
its `.mjs`, the .NET publish emits a whole bundle, and the build ships them
beside a small entry module that adapts whichever of them was produced to
`tacInvoke`. What is emitted per language is a wrapper of about twenty lines,
not a language implementation.

Swift is the other lesson. It compiles to linear memory and satisfies the three
exports exactly, but its compiler targets WASI, so the module imports fourteen
functions it never calls and initialises through `_initialize` rather than at
instantiation. Neither is something a companion author asked for, so the island
runtime answers for both — an environment that does nothing, and a call before
first use — and a module with no imports ignores all of it. The alternative was
to demand that every WASI-targeting language stop being one.

### An authoring prelude, not a subset

Neither language has reflection in a wasm build, so a companion declares which
members the island may reach. The build appends a prelude that carries the ABI,
and the author writes plain code:

```dart
int count = 6;
int doubled() => count * 2;

final tac = {
  'count': TacField(() => count, (value) => count = value as int),
  'doubled': TacMethod((arguments) => doubled()),
};
```

The prelude is compiled by the real compiler along with the author's source: it
is a library, and the language stays whole. This is the opposite of the legacy
route, where the compiler understands a subset of each language.

### Consequences of the amendment

- **A glued companion cannot be verified by instantiating it in isolation**, so
  the browser gate is the check that matters. `scripts/wasm/companion-browser-test.mjs`
  drives Rust, Dart, Kotlin, Swift, and C# islands on one page and asserts the
  same five behaviours of each.
- **Size follows the language's runtime, not the companion.** The same trivial
  fixture — one integer, one string, one method — is 21 KB from Rust, 98 KB
  from Dart, 120 KB from Kotlin, 5.5 MB from Swift, and 3.5 MB across 21 files
  from C#. Kotlin is 584 KB without `-Xir-dce` and Swift would be 53 MB with
  Foundation linked, so emission passes the flags that avoid both. The 64 KiB
  budget in the ABI probe therefore applies to the bare Rust shape only: every
  other language brings its runtime, and a companion author should know that
  before choosing one.
- **A .NET companion boots rather than instantiates.** The bundle is a runtime,
  so the island's first value arrives after a runtime start rather than after a
  module instantiation. The browser gate allows for it; a page that wants its
  content fast should not put a C# companion on the critical path.
- **Kotlin needs a standard library the compiler does not ship.** `kotlinc-js`
  carries only `kotlin-stdlib-js.klib`, so a wasm build needs
  `kotlin-stdlib-wasm-js` from Maven Central, and the project points
  `KOTLIN_WASM_STDLIB` at it. `ty doctor` reports its absence rather than
  letting the build discover it.
- **Values cross as JavaScript values where the boundary is already
  JavaScript.** The Kotlin prelude exports typed accessors and the wrapper
  builds the JSON; only the host ever serialises. Dart cannot do this — it
  cannot export anything — so its prelude does the JSON itself.
