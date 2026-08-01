# Phase 1 Web Vertical Slice

## Outcome

A new developer can use one compiled Rust `ty` executable to create, build,
and serve a static Tachyon application without Bun, Node.js, a handler runtime,
or application dependencies.

## Public Workflow

```text
ty init hello --name "Hello"
ty build hello
ty dev hello
```

`ty bundle` is an alias for `ty build`, and `ty serve` is an alias for
`ty dev`, preserving the current workflow vocabulary while the rewrite is
developed.

## Source Model

- `client/pages/tac.html` is the root client page.
- `client/pages/<static-segments>/tac.html` is a nested static client page.
- `server/routes/yon.html` is the root static Yon view.
- `server/routes/<static-segments>/yon.html` is a nested static Yon view.
- Route segments are lowercase ASCII letters, digits, and single internal
  hyphens.
- Tac and Yon views cannot claim the same route.
- Symlinks below either source root are rejected.
- Every source is UTF-8, at most 1 MiB, and contains no NUL byte.

Phase 1 tokenizes HTML with a WHATWG-compatible tokenizer. HTML fragments are
placed into a deterministic accessible document shell; complete documents are
preserved after line-ending normalization. The compiler rejects tokenizer
errors and any feature whose semantics belong to a later phase:

- `if`, `else`, `for`, and `loop`;
- Tac components without a standards-based custom-element name;
- companion `tac.*` or `yon.*` source;
- inline scripts and event-handler attributes;
- dynamic route segments.

## Build Contract

`ty build`:

1. canonicalizes the project root;
2. discovers sources without executing application code;
3. constructs a sorted, immutable route graph;
4. tokenizes and validates every HTML source;
5. writes each static route as `<route>/index.html`;
6. writes `route-manifest.json` using Route Manifest v1;
7. publishes the complete output directory only after every route succeeds.

Identical source and options produce byte-identical output. A failed build
leaves the previously published output untouched.

## Development Server

`ty dev` builds before binding and serves only generated files. It binds to
`127.0.0.1` by default, supports GET and HEAD, returns a bounded 404 response,
and emits defensive browser headers. A non-loopback bind requires
`--allow-non-loopback`.

## Diagnostics

Failures use stable `TY####` codes, project-relative source spans, actionable
help, and non-zero exit status. `--diagnostic-format json` emits Diagnostics
v1; the default human format is suitable for a terminal.

## Acceptance Criteria

- A generated project builds twice with byte-identical output.
- Its root page and route manifest are served by the compiled Rust binary.
- The behavior-level suite invokes the real `ty` process.
- Missing sources, route collisions, unsafe source shapes, unsupported syntax,
  unsafe bind configuration, and non-empty initialization targets fail safely.
- The workspace passes formatting, check, Clippy, tests, rustdoc, coverage,
  dependency policy, and the legacy compatibility suite.
- Linux, macOS, and Windows CI build and exercise the same generated project.

## Out of Scope

Handler execution, context injection, control tags, components, bindings,
islands, incremental rebuilds, source maps, native rendering, remote binding
without explicit opt-in, TLS, and production HTTP serving are later phases.
