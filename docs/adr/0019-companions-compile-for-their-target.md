# ADR 0019: Companions Compile for Their Target, Not to WebAssembly

- Status: Accepted
- Date: 2026-08-30
- Reconciles: the unshipped companion decision dated 2026-08-23
- Supersedes: [ADR 0011](0011-wasm-companion-abi.md)

## Context

The WebAssembly companion ABI gave compiled languages a browser sandbox. The
newer local architecture instead uses compiled page companions to reach the
native platform. A browser sandbox cannot provide that access, and requiring
five WebAssembly toolchains does not solve the problem.

## Decision

Components use ordinary JavaScript/TypeScript modules. Compiled companions
beside a page use their language's native toolchain and run inside the host.

| Companion | Targets |
| --- | --- |
| `tac.js`, `tac.ts` | Web and all native web views |
| `tac.rs` | macOS, Windows, Linux |
| `tac.swift` | macOS, iOS |
| `tac.kt` | Android |
| `tac.cs` | Windows |

The table in `project.rs` is authoritative for compilation and diagnostics.
Where multiple native companions reach a target, its platform language wins
over Rust. Selection is per route, and only the selected companion is built.
A route with companions but none reaching the requested target fails with
TY1010 instead of publishing a page whose behavior was silently omitted.

The JSON operations remain `init`, `get`, `set`, and `call`. Initialization
describes fields and methods without invoking application methods. Rust exposes
`tac_native_invoke` and `tac_native_free` through a C ABI; native-language hosts
invoke their same-language companion directly. Generated per-route namespaces
and dispatch tables prevent collisions and preserve separate route state.

## Migration and consequences

- A browser component with `tac.rs`, `.swift`, `.kt`, `.cs`, or `.dart` must
  move platform work to a native page companion and keep its view-facing module
  in JavaScript/TypeScript. Old WebAssembly components receive an actionable
  diagnostic rather than being ignored.
- Dart has no native target under this contract and is not a companion language.
- Native companions have the application's operating-system privileges; they
  are not sandboxed plugins. Do not compile untrusted companion source.
- Browser modules retain normal module semantics, including imports, helpers,
  and constants. Native member discovery does not justify restricting them.
- The former WebAssembly gates are replaced by native compile/execute gates,
  plus browser-to-host protocol tests; removal alone is not qualification.
- Existing published versions and tags remain immutable. This behavior ships
  only in the new reconciliation release, with an explicit migration notice.
