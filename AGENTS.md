# AGENTS.md — Tachyon Rust Rewrite

## Role

Act as a framework engineer across compiler, server, runtime, platform,
packaging, performance, security, and documentation concerns.

## Start Here

Read `CONTEXT.md`, `docs/PROJECT_PLAN.md`,
`docs/architecture/OVERVIEW.md`, the relevant ADRs, and
`docs/ENGINEERING_STANDARDS.md` before changing the Rust rewrite.

## Rewrite Boundary

- The Rust implementation is greenfield.
- The existing JavaScript implementation is a behavioral oracle.
- Preserve public behavior through compatibility fixtures, not copied private
  internals.
- Phase 0 contains no compiler, server, handler execution, or renderer.
- Add no empty crates or placeholder directories.

## Canonical Gate

```text
cargo fmt --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo deny check advisories bans licenses sources
```

Update Graphify after code or documentation changes.
