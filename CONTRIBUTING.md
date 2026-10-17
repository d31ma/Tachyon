# Contributing to the Tachyon Rust Rewrite

## Before You Start

Read these documents in this order:

1. `CONTEXT.md`;
2. `docs/PROJECT_PLAN.md`;
3. `docs/architecture/OVERVIEW.md`;
4. the architecture decision records (ADRs) and schemas that apply to your
   change;
5. `docs/ENGINEERING_STANDARDS.md`;
6. `docs/THREAT_MODEL.md`, if your work touches a security boundary.

The legacy JavaScript implementation is only a reference for compatible
behavior. Do not copy its private internal structure into Rust.

## Toolchain

Install `rustup`. The repository selects the exact toolchain from
`rust-toolchain.toml` automatically.

```text
rustup show
cargo --version
```

Both commands must report `1.97.1`. Another package manager can put its own
`cargo` earlier on `PATH` than the Rustup shims. If that happens, do one of
these:

- run commands with `rustup run 1.97.1`;
- add the directory of `rustup which --toolchain 1.97.1 cargo` to the start
  of `PATH`.

Each phase needs these tools:

| Work | Tools needed |
| --- | --- |
| Phase 1 | Rust only. |
| Phase 2 through Phase 4 real-process tests | Node.js 24.5 or newer as `node` (the outgoing request guard refuses older Node.js, ADR 0068), and CPython as `python3` (`python` on Windows). Native CI (continuous integration) uses Node.js 24.18.0 and Python 3.14.6. |
| Phase 3 browser gate | Bun and Playwright Chromium. Bun also runs the legacy compatibility suite. |
| Phase 4 native evidence | macOS, Xcode/Swift, code signing, a GUI session with Accessibility permission, and Playwright Chromium. |

## Writing Code

Write code that a new graduate who reads English as a second language can
understand on the first read.

- Write the simplest code that works.
- Name things with common, literal English words. Do not use idioms, slang,
  puns, rare words or abbreviations that the code does not explain.
- Keep functions short. Put one step on each line.
- Use early returns instead of deep nesting.
- Do not use clever tricks, dense one-line expressions or long chains that a
  reader must untangle.
- Choose the plain construct over the advanced one, unless the plain one is
  wrong. For example, use a loop instead of a fold, and an `if` instead of a
  table of closures.
- Write no comments. Names and structure must explain the code. If code needs
  a comment, rename or restructure it. Keep only the comments that a tool
  reads, such as JSDoc type tags, `@ts-` and lint directives, license headers
  and the clap `///` documentation of the CLI.
- Do not declare variables or constants at file (module) level. Put them in a
  class or type:
  - JavaScript and TypeScript: `static` fields of a class.
  - Rust: associated `const` items in an `impl` block. A lazily created
    `static` goes inside the function that returns it.
  - Python, Dart, Kotlin, Swift and PHP: class attributes or static fields.
- The only file-level code allowed besides imports, types, functions and
  classes is the code that defines the framework's decorators and
  annotations.

## Phase 1 CLI

The implemented slice accepts static `client/pages/**/tac.html` views. It
finds `server/routes/**/yon.*` REST handlers. A `yon.html` file is invalid.

```text
cargo run --locked --bin ty -- init hello --name "Hello"
cargo run --locked --bin ty -- build hello
cargo run --locked --bin ty -- dev hello
```

The `phase1_cli` behavior test starts this same compiled executable and a real
TCP server. The test must work the same on every platform.

## Phase 2 Handler CLI

Use the compiled binary to call a JavaScript or Python Yon handler that `ty`
has found:

```text
cargo run --locked --bin ty -- handler invoke \
  server/routes/products/yon.js \
  --project . \
  --route /products \
  --method GET
```

The `handler_process` corpus runs both embedded language adapters against real
child processes. The `phase2_cli` corpus runs the real `ty` executable. It
includes paths with spaces and Unicode. Read `docs/PHASE_2_SPEC.md` before you
change any of these:

- source discovery;
- wire framing;
- error codes;
- process lifecycle;
- environment inheritance;
- adapter behavior.

## Phase 3 View Compiler

Read `docs/PHASE_3_SPEC.md` before you change any of these:

- expressions;
- controls;
- component scope;
- the client render plan and how components mount;
- source maps;
- incremental state.

Yon supports REST only (ADR 0016). Route Manifest v1 keeps only its required
empty context shape, so that the wire format stays compatible. The
`phase3_cli` corpus runs the compiled `ty` binary. After you build it, run the
real-browser contract:

```text
cargo build --locked --bin ty
bun run test:phase3-browser
```

## Phase 4 macOS Native Compiler

Read `docs/PHASE_4_SPEC.md` before you change Native UI adapters,
state/actions, accessibility, lifecycle, WebSurface containment, manifests or
packaging.

```text
cargo build --locked --bin ty
cargo run --locked --bin ty -- build <project> --target macos
bun run test:phase4-macos
```

The last command does these steps:

1. It builds and starts a new SwiftUI application.
2. It controls the application through macOS Accessibility.
3. It tests the application's mobile-web reference in Chromium.
4. It writes evidence under `target/phase4-evidence/`. Git ignores this
   folder.

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

Coverage must be at least 80% for lines, functions and regions, measured over
the whole workspace:

```text
cargo llvm-cov --workspace --all-features --locked \
  --ignore-filename-regex 'crates/tachyon-core/src/native/(compiler|macos)\.rs' \
  --fail-under-lines 80 --fail-under-functions 80 --fail-under-regions 80
```

The LLVM percentage leaves out only two files: the native compiler that calls
external tools, and the macOS host code. Their async and process closures
make the function coverage number wrong. In-process packaging tests and the
required native application gates cover them instead. The CLI, the other
platform code, the compiler, the server and the fuzz-only interface stay in
the measurement. ADR 0012 explains why this honest whole-workspace baseline
replaced the earlier 90% measurement, which was per phase and did not depend
on the target. The `tachyon-contracts` tests check every canonical schema
against its meta-schema. They also check that each schema accepts its
positive fixture and rejects its negative fixture.

## Self-Hosted CI Runners

By default every job in `.github/workflows/rust-ci.yml` runs on a
GitHub-hosted runner. A maintainer can move the jobs of branch pushes to their
own macOS, Windows, and WSL machines with the repository variable
`TACHYON_SELF_HOSTED`. Pull requests always stay on GitHub-hosted runners.
[`docs/SELF_HOSTED_RUNNERS.md`](docs/SELF_HOSTED_RUNNERS.md) explains how to
set up each machine and the security rules for this public repository.

## Change Shape

- Work in a branch with one purpose. Submit small vertical slices.
- Write down acceptance criteria that can be observed before you implement.
- First add the smallest behavior test that fails.
- Include negative tests and boundary tests.
- If the behavior affects contracts, the threat model, the support matrix or
  the documentation, update them in the same change.
- Do not add empty crates or placeholder modules.
- Do not mix generated output with unrelated handwritten refactors.

## Commit and Pull Request Expectations

Every important pull request describes:

- the invariant or the result for the user;
- the effect on compatibility;
- what happens on failure and how the system recovers;
- the security effects;
- evidence from local validation;
- native evidence for any claim about a platform.

Changes to generated artifacts and dependency lock files must be on purpose
and easy to review. The result of CI is final.

## Reporting Security Problems

Do not open a public issue for a vulnerability you suspect. Follow
`SECURITY.md`.
