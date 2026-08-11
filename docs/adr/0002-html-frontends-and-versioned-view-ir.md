# ADR 0002: HTML Frontends and Versioned View IR

- Status: Accepted
- Date: 2026-07-26
- Amended by: ADR 0016 removes `yon.html` as a frontend; this decision now
  applies only to Tac views.

## Context

Application developers must use HTML for views on web and non-web targets.
Tachyon also needs structural control flow, components, islands, source-aware
diagnostics, deterministic builds, and native code generation.

Passing raw HTML directly to every backend would duplicate parsing and make
control-tag, component, and fallback behavior platform-dependent.

## Decision

Treat `tac.html` as the entry context for the HTML frontend. Parse without
executing application code, preserve source spans,
resolve control tags and components, and lower validated source into View IR.

`if`, `else`, `for`, and `loop` are structural compiler syntax. They do not
survive as element nodes.

View IR v1 is a canonical, platform-neutral machine contract. Its JSON Schema
defines interchange and compatibility; optimized internal Rust
representations may differ as long as conformance tests prove equivalent
behavior.

Identical canonical source, configuration, compiler version, and target inputs
must produce byte-identical View IR.

## Consequences

- Web and native backends share parsing and semantic validation.
- Diagnostics can name stable source spans and codes.
- New backends consume one normalized representation.
- IR changes carry explicit compatibility and migration obligations.
- The parser, component resolver, and expression language remain separate
  future implementation decisions.

## Rejected Alternatives

- **Raw DOM as the compiler contract**: loses Tachyon structural meaning and is
  not stable across parser implementations.
- **Platform-specific parsing**: permits semantic drift and multiplies the
  security surface.
- **A new JSX-like view language**: violates the HTML-only developer contract.
- **Keeping control tags as unknown elements**: produces incorrect browser and
  native behavior.

## Acceptance Gate

- View IR v1 has a valid canonical schema and positive/negative fixtures;
- HTML-only and control-tag invariants appear in `CONTEXT.md`;
- web, server, and native backends are documented as consumers rather than
  alternate parsers;
- any future parser proves deterministic source-to-IR golden fixtures.
