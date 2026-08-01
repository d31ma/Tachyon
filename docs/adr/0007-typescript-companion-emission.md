# ADR 0007: TypeScript Companion Emission

- Status: Accepted
- Date: 2026-07-27

## Context

`tac.ts` is a legacy source convention. Emitting it requires turning
TypeScript into JavaScript, because a browser cannot consume type
annotations. Type *checking* is a separate concern that `ty` does not claim to
perform and that this repository already covers with its own typecheck step.

Four options were measured rather than estimated. The workspace has 194
crates today, so either in-process transpiler roughly doubles the dependency
tree and its audit surface for one file type.

| Option | New Rust crates | Licensing change |
| --- | --- | --- |
| `OXC` | ~115 | add `ISC` and `Apache-2.0 WITH LLVM-exception` |
| `SWC`, which Deno uses | ~171 | similar |
| Node `stripTypeScriptTypes` | 0 | none |
| The TypeScript compiler | 0 | none |

Deno separates the two problems: it transpiles with `SWC` in Rust and type
checks by embedding the real `tsc` in a V8 snapshot. Only the transpilation
half applies here.

## Decision

Emit `tac.ts` by invoking the TypeScript compiler, version 6 or newer, as a
bounded external process.

The compiler is located project-first, so `node_modules/.bin/tsc` wins over
`PATH` and a pinned project dependency is authoritative. Emission is hermetic:
`--ignoreConfig` with an explicit `--target es2022 --module esnext` means a
project's `tsconfig.json` cannot alter the output. `--noCheck` is passed,
because emission must not depend on a project type checking cleanly. The
subprocess runs in the project root and never through a shell, and its major
version is verified before use.

## Consequences

No new Rust dependency and no licensing change. Zero semantic divergence: the
reference implementation defines the semantics, so enums, parameter
properties, and future syntax are correct by construction rather than by our
reimplementation of them. The decision is consistent with the existing
architecture, which already supervises `node`, `python3`, `swiftc`, `cc`,
`gradle`, and `mingw`. TypeScript 6 and 7 are the Go-native compiler, so one
file emits in roughly 70 ms.

Against that, building a project that uses `tac.ts` now requires TypeScript 6
or newer present — the version that first accepts `--ignoreConfig`, verified
against 5.6, 5.9, 6.0.3, and 7.0.2. That is a new build-time toolchain requirement, though only for
projects using the convention, and it fails closed with `TY1009` naming the
remedy. Emission costs one process per TypeScript companion.

## Rejected Alternatives

`SWC` or `OXC` in process would avoid the external toolchain and run faster
still, but each roughly doubles the dependency tree for one file type and each
is a reimplementation that can diverge from the reference compiler. Revisit if
the external requirement becomes a real obstacle.

Node's `stripTypeScriptTypes` costs nothing and Node is already required for
`yon.js`, but the API is explicitly experimental and its strip-only mode
rejects `enum` and parameter properties outright.

A hand-written type stripper was rejected outright. TypeScript syntax is large
enough that a bespoke stripper would be subtly wrong, and being subtly wrong
while claiming correctness is the failure mode this project can least afford.

## Acceptance Gate

- [x] `tac.ts` emits JavaScript with types erased and enums desugared.
- [x] The generated document references the emitted module.
- [x] A compiler older than version 7 reports a version requirement rather
      than an unknown-option error.
- [x] An absent compiler fails closed with `TY1009` and names the remedy.
- [x] Emission ignores a project's `tsconfig.json`, so output is deterministic
      across projects.
