# Contributing to the Tachyon Rust Rewrite

## Before You Start

Read:

1. `CONTEXT.md`;
2. `docs/PROJECT_PLAN.md`;
3. `docs/architecture/OVERVIEW.md`;
4. relevant ADRs and schemas;
5. `docs/ENGINEERING_STANDARDS.md`;
6. `docs/THREAT_MODEL.md` for boundary-sensitive work.

The legacy JavaScript implementation is a compatibility oracle. Do not port
private implementation structure into Rust.

## Toolchain

Install `rustup`. The repository automatically selects the exact toolchain from
`rust-toolchain.toml`.

```text
rustup show
cargo --version
```

Both commands must report `1.97.1`. If another package manager shadows the
Rustup shims on `PATH`, run commands with `rustup run 1.97.1`, or prepend the
directory containing `rustup which --toolchain 1.97.1 cargo` to `PATH`.

The Phase 1 contributor path requires Rust only. Phase 2 through Phase 4
real-process tests also require Node.js and CPython available as `node` and
`python3` (`python` on Windows). Native CI fixes Node.js 24.18.0 and Python
3.14.6. The Phase 3 browser gate additionally uses Bun and Playwright Chromium;
Bun also runs the legacy compatibility suite. Phase 4 native evidence requires
macOS, Xcode/Swift, code signing, a GUI session with Accessibility permission,
and Playwright Chromium.

## Phase 1 CLI

The implemented slice accepts static `client/pages/**/tac.html` views and
discovers `server/routes/**/yon.*` REST handlers. `yon.html` is invalid:

```text
cargo run --locked --bin ty -- init hello --name "Hello"
cargo run --locked --bin ty -- build hello
cargo run --locked --bin ty -- dev hello
```

The behavior-level `phase1_cli` test starts this same compiled executable and a
real TCP server. It must remain platform-neutral.

## Phase 2 Handler CLI

Invoke a discovered JavaScript or Python Yon handler with the compiled binary:

```text
cargo run --locked --bin ty -- handler invoke \
  server/routes/products/yon.js \
  --project . \
  --route /products \
  --method GET
```

The `handler_process` corpus runs both embedded language adapters against real
children. The `phase2_cli` corpus runs the actual `ty` executable, including
paths with spaces and Unicode. Read `docs/PHASE_2_SPEC.md` before changing
source discovery, wire framing, error codes, process lifecycle, environment
inheritance, or adapter behavior.

## Phase 3 View Compiler

Read `docs/PHASE_3_SPEC.md` before changing expressions, controls, component
scope, client render-plan and component mount behavior, source maps, or
incremental state. Yon is REST-only under ADR 0016; Route Manifest v1 retains
only its required empty context shape for wire compatibility. The `phase3_cli`
corpus runs the compiled `ty` binary. Run
the real-browser contract after building it:

```text
cargo build --locked --bin ty
bun run test:rust-browser
```

## Phase 4 macOS Native Compiler

Read `docs/PHASE_4_SPEC.md` before changing Native UI adapters, state/actions,
accessibility, lifecycle, WebSurface containment, manifests, or packaging.

```text
cargo build --locked --bin ty
cargo run --locked --bin ty -- build <project> --target macos
bun run test:rust-macos
```

The final command builds and launches a fresh SwiftUI application, drives it
through macOS Accessibility, exercises its mobile-web reference in Chromium,
and writes ignored evidence under `target/phase4-evidence/`.

## Canonical Local Gate

Run these commands from the repository root:

```text
cargo fmt --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo deny check advisories bans licenses sources
```

Coverage uses a uniform 80% whole-workspace floor for lines, functions, and
regions:

```text
cargo llvm-cov --workspace --all-features --locked \
  --ignore-filename-regex 'crates/tachyon-core/src/native/(compiler|macos)\.rs' \
  --fail-under-lines 80 --fail-under-functions 80 --fail-under-regions 80
```

Only the external-tool native compiler and macOS host glue are excluded from
the LLVM percentage because their async/process closures distort function
coverage. They are covered by in-process packaging tests and mandatory native
application gates. CLI, other platform glue, compiler, server, and the
fuzz-only interface remain in the denominator. ADR 0012 records why this
honest whole-workspace baseline replaced the earlier target-neutral 90% phase
measurement. The `tachyon-contracts` tests meta-validate every canonical
schema, accept its positive fixture, and reject its negative fixture.

## Change Shape

- Work in a focused branch and submit small vertical slices.
- State observable acceptance criteria before implementation.
- Add the narrowest failing behavior test first.
- Include negative and boundary tests.
- Update contracts, threat model, support matrix, and documentation in the same
  change when behavior affects them.
- Do not introduce empty crates or placeholder modules.
- Do not mix generated output with unrelated handwritten refactors.

## Commit and Pull Request Expectations

Every material pull request describes:

- the invariant or user outcome;
- compatibility impact;
- failure and recovery behavior;
- security implications;
- local validation evidence;
- native evidence for any platform claim.

Generated artifacts and dependency lock changes must be intentional and
reviewable. CI is authoritative.

## Reporting Security Problems

Do not open a public issue for a suspected vulnerability. Follow
`SECURITY.md`.
