# Phase 0 Evidence

Status: **complete — local acceptance passed on 2026-07-26**

Native Linux and Windows results remain CI evidence and are not inferred from
the macOS run recorded here.

## Baseline

- Rewrite branch: `codex/rust-rewrite`
- Legacy baseline commit: `47775135e456cd8db3c80f4dfde0afaf1273296b`
- Product foundation version: `0.0.0-phase0`
- Required Rust toolchain: `1.97.1`

## Local Gate

- Host: macOS arm64.
- Installed and selected toolchain:
  - `rustc 1.97.1 (8bab26f4f 2026-07-14)`;
  - `cargo 1.97.1 (c980f4866 2026-06-30)`.
- `cargo fmt --check`: passed.
- `cargo check --workspace --all-targets --all-features --locked`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D
  warnings`: passed.
- `cargo test --workspace --all-targets --all-features --locked`: passed, 15
  Rust tests.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
  --locked`: passed.
- `cargo llvm-cov --workspace --all-features --locked --fail-under-lines 95
  --fail-under-functions 90 --fail-under-regions 90`: passed with 95.09% line,
  92.00% function, and 91.23% region coverage.
- `git diff --check`: passed.

The host also has a Homebrew Cargo earlier on `PATH`. Local evidence was
collected with the Rustup `1.97.1` toolchain directory prepended, not with the
older Homebrew binary.

## Contract Gate

- Seven canonical Draft 2020-12 schemas were meta-validated.
- Each schema accepted its valid example and rejected its invalid example.
- The embedded Rust registry exactly matched the on-disk schema corpus.
- Repository policy tests verified required foundation documents, five accepted
  ADRs, immutable GitHub Action pins, resolvable relative links, and one exact
  Rust version across the workspace, toolchain, and CI.

## Supply-Chain Gate

- `cargo deny check advisories bans licenses sources`: passed with
  `cargo-deny 0.19.7`.
- `cargo install --locked cargo-llvm-cov --version 0.8.6` supplied the pinned
  coverage runner used locally and in CI.
- All GitHub workflow and issue-form YAML parsed successfully, and
  `actionlint 1.7.7` accepted every workflow.
- Legacy compatibility guard: Bun `1.3.11` installed the existing lockfile,
  type checking passed, and the legacy suite passed 637 tests across 62 files
  with zero failures.

## Native Matrix

| Target | Evidence |
| --- | --- |
| macOS arm64 | buildable; complete local gate above |
| Linux x86_64 | pending GitHub Actions |
| Windows x86_64 | pending GitHub Actions |

Phase 0 completion means the foundation is complete and locally validated. It
does not claim that unexecuted remote CI jobs passed.
