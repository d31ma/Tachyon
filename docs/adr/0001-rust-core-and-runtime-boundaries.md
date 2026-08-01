# ADR 0001: Rust Core and Runtime Boundaries

- Status: Accepted
- Date: 2026-07-26

## Context

Tachyon currently combines compilation, server lifecycle, runtime behavior,
native generation, and packaging in a Bun-based implementation. The greenfield
exercise selected a language without assuming an existing runtime.

The core needs predictable performance, strong types, memory safety, native
artifacts, controlled concurrency, and direct access to platform toolchains.
Tachyon must remain polyglot for application handlers without making its core
dependent on every supported language runtime.

## Decision

Use Rust for the compiler, route graph, server, process supervision, packaging,
cache, diagnostics, observability, and native render planning.

Use a Cargo workspace organized as a modular monolith. Modules communicate
through typed in-process contracts. Do not introduce internal microservices
without a measured isolation or independent-scaling requirement.

Application language runtimes remain supervised adapters behind Handler
Protocol v1. The Rust core does not embed Bun or assume Node, Python, Ruby, PHP,
or another runtime is installed unless a project selects that adapter.

Pin exact stable Rust security releases. Phase 0 uses Rust 1.97.1. Forbid
`unsafe` workspace-wide until a reviewed safety requirement proves that a
small isolated exception is necessary.

## Consequences

- The core build and test path requires Rust only.
- Language adapters can evolve without changing compiler internals.
- Runtime compatibility is an explicit support matrix.
- Native platform bindings may eventually require isolated FFI crates and
  safety ADRs.
- The current JavaScript implementation remains a behavior oracle, not a core
  runtime dependency.

## Rejected Alternatives

- **TypeScript/Bun core**: fastest source port, but retains the runtime and
  deployment assumptions the greenfield exercise removed.
- **Go core**: operationally simple, but less suitable for compiler IR,
  deterministic low-level platform integration, and tightly controlled FFI.
- **C++ core**: mature platform access, but materially weaker default memory
  safety and dependency hygiene.
- **Internal services per subsystem**: adds deployment and failure boundaries
  without a measured need.

## Acceptance Gate

- the workspace builds on exact stable Rust;
- core crates have no application runtime dependency;
- dependency direction is documented and acyclic;
- all public boundaries can be tested without loading legacy implementation
  code.
