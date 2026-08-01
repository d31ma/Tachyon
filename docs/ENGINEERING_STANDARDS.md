# Tachyon Rust Engineering Standards

## Meaning of Enterprise-Grade

Enterprise-grade means repeatable evidence: explicit ownership, stable
contracts, bounded failure, deterministic builds, native validation, secure
defaults, recoverable releases, and documentation that agrees with behavior.
It does not mean maximizing crate count, abstractions, or CI jobs.

## Repository Rules

- Build vertical slices.
- Add a crate only with production behavior and tests.
- Keep public contracts under `api/`; generated output identifies its source
  and is never hand-edited.
- Record hard-to-reverse decisions in ADRs and proposed public behavior in
  RFCs.
- Do not create empty placeholder directories.
- Do not create dumping grounds named `utils`, `helpers`, `common`, `shared`,
  `misc`, `base`, or `manager`.
- Keep unit tests beside code and integration or system tests under the owning
  crate's `tests/` directory.
- Keep legacy compatibility evidence separate from new internal design.

## Rust Rules

- Stable Rust only. Pin an exact security-patched toolchain.
- The 2024 edition and workspace resolver 3 are mandatory.
- `unsafe` is forbidden by default. A future exception requires an accepted
  safety ADR, a small isolated crate, documented invariants, Miri or sanitizer
  coverage where applicable, and named reviewers.
- Public items have rustdoc.
- Errors are typed and actionable. Never parse human-readable error text.
- Do not panic on untrusted input, configuration, filesystem state, protocol
  messages, or expected runtime failure.
- Constructors validate required invariants.
- Prefer owned immutable values at module boundaries.
- Keep I/O at boundaries and transformations deterministic.
- Bound input size, recursion, queues, concurrency, retries, processes, and
  external reads.
- Do not use ambient mutable globals.
- Do not invoke application commands through a shell.
- Do not log secrets, full request bodies, capability tokens, or unredacted
  environment values.
- Avoid feature flags that create unsupported combinations. Every supported
  combination must compile and test in CI.

## Naming

- Crates and modules are named for one responsibility.
- Domain names come from `CONTEXT.md`.
- Avoid `Impl`, `Util`, `Helper`, `Manager`, and `Common`.
- Machine fields use `snake_case`.
- Diagnostic codes use `TY` plus four digits.
- Identifiers are opaque; consumers do not infer security or type from shape.

## Contract Rules

- Schemas, fixtures, error codes, and artifact manifests are canonical.
- Additive changes are preferred within a major version.
- Breaking changes require an ADR or RFC, migration, deprecation window,
  compatibility tests, and a new major version.
- Unknown-field and unknown-enum behavior is explicit.
- Mutating operations document idempotency and retry safety.
- Human-readable messages are not parsing contracts.

## Test Standards

- Unit tests prove deterministic transformations and invariants.
- Table, property, and model tests cover state spaces.
- Golden tests cover stable diagnostics and generated artifacts.
- Contract tests run against real compiled processes or artifacts.
- Fuzz tests cover every parser and external protocol boundary.
- Adversarial tests cover malformed, oversized, replayed, concurrent,
  path-escaping, resource-exhausting, and capability-escalating inputs.
- Native claims require execution on the native OS and architecture.
- Tests are deterministic unless explicitly fuzz, chaos, load, or soak tests.
- Whole-workspace coverage may not fall below 80% for lines, functions, or
  regions. The measurement includes CLI, operating-system, toolchain, and
  fuzz-only surfaces. Only the external native compiler and macOS host glue
  named by ADR 0012 are excluded. High-risk parser, path, compiler, and server
  boundaries require focused adversarial and end-to-end tests regardless of
  aggregate coverage.
- CRAP score 15 is the absolute per-function ceiling, not the target.

## Review Expectations

Every material change states:

- the invariant being protected;
- observable acceptance criteria;
- compatibility consequences;
- failure modes and recovery;
- threat-model changes;
- validation evidence.

Generated changes are not mixed with unrelated handwritten refactors. No
debugging output, sample secrets, stale fixtures, or unowned TODOs ship.
