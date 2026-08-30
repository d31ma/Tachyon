# Tachyon Rust Engineering Standards

## Meaning of Enterprise-Grade

Enterprise-grade means repeatable evidence: explicit ownership, stable
contracts, bounded failure, deterministic builds, native validation, secure
defaults, recoverable releases, and documentation that agrees with behavior.
It does not mean maximizing crate count, abstractions, or CI jobs.

All external build, probe, and hook commands must use the shared supervised
runner. Direct `Command::output`, unbounded pipe capture, leader-only timeout,
and detached descendant cleanup are prohibited in production paths.

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
- Keep package-local tests beside code and system-wide tests under `tests/`.
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

## Yon Boundaries

- Server layer sources declare exactly one matching `@Controller`, `@Service`,
  `@Repository`, `@Client`, or `@Delegate`; class-name inference is forbidden.
- Routes, middleware, and workers use the eight framework-owned Yon languages.
  Other programs cross an explicit `@Relay` delegate boundary; project files
  cannot register an interpreter or select an executable handler.
- Relay and streaming implementations concurrently drain bounded pipes, use
  one absolute request deadline, redact child diagnostics from public
  responses, and prove descendant reaping on timeout and subscriber closure.
- Environment variables use `TAC_` or `YON_`; `TACHYON_*` names are forbidden.
- Topic SSE clients handle the named `topic-error` event, parse its canonical
  JSON payload, and close on `terminal: true`. Cursor-stale recovery recreates
  the subscription without an explicit cursor; HTTP 503 admission failures use
  bounded backoff. Limits are 128 global subscribers, 32 per topic, 32 active
  topics, 256 replay records, 64 KiB per record, and 16 MiB per log.

## Handler Cache Operations

Compiled and staged Yon artifacts live in the project-local
`.tachyon/handlers` directory. All lookups and mutations after opening that
directory are relative to its non-following directory handle. The cache is
content-addressed and pruned to at most 256 recursively accounted entries and
512 MiB. Tachyon normally recovers abandoned build locks automatically: a lock
whose process is gone is reclaimed immediately, while a hard ten-minute lease
bounds a lock whose process identifier was reused. Cache pruning then accounts
for and removes the abandoned digest and temporary publication files under the
same capability-confined prune transaction.

To inspect an incident, list `.tachyon/handlers` without following symlinks and
preserve the directory for diagnosis. Never delete an individual `*.lock` or
artifact while `ty serve`, `ty bundle`, or another Tachyon build may still be
running. For manual recovery:

1. Stop every Tachyon server and build process using that project.
2. Rename `.tachyon/handlers` to a diagnostic backup on the same filesystem.
3. Run the original Tachyon command; it creates a new confined cache and
   rebuilds content-addressed artifacts.
4. Remove the backup only after the rebuild succeeds and any required incident
   evidence has been collected.

`ty cache status` and `ty cache clean` operate on the installation/runtime
cache selected by `TAC_CACHE_DIR`. They do not inspect, repair, or remove the
project-local `.tachyon/handlers` cache.

## Naming

- Crates and modules are named for one responsibility.
- Domain names come from `CONTEXT.md`.
- Project-owned environment variables start with `TAC_` for compiler, client,
  packaging, and repository tooling, or `YON_` for server and handler runtime
  policy. Do not introduce a `TACHYON_` environment namespace.
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
