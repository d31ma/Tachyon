# 0017 — Mandatory Yon layers, explicit relay, and declared streaming

Status: Accepted

## Context

Arbitrary `.tachyonrc` interpreters and executable/shebang handlers made every
file a potential runtime boundary. They also left routes, middleware, and
workers outside the five-layer architecture already expressed by the project
layout. A class-name fallback made an absent architecture declaration look
valid, while language-local relay code could expose child stderr or deadlock on
full pipes. Streaming additionally needs to be known before HTTP headers are
sent; discovering a generator only after invocation is too late.

## Decision

Yon runs exactly JavaScript, TypeScript, Python, Java, C#, Kotlin, PHP, and
Rust. Sources under `server/routes`, `server/services`,
`server/repositories`, `server/clients`, and `server/delegates` must attach
`@Controller`, `@Service`, `@Repository`, `@Client`, or `@Delegate`
respectively and use the matching class-name suffix. A layer may call only a
deeper layer; repository, client, and delegate are peers.

Programs in other languages remain usable only behind an explicit `@Relay`
method on a delegate. Relay commands never use a shell. Their request, stdout,
stderr, deadline, and cleanup are bounded; stdout and stderr drain concurrently;
process and stderr details never enter a client response. The outer handler
process group is the final descendant-reap boundary.

`.tachyonrc.interpreters`, shebang handlers, executable-handler discovery, and
the `Handler` class fallback are removed. `.tachyonrc.workers` remains, but the
scheduled source must use a Yon language. Middleware follows the same language
restriction.

A multi-response HTTP method declares `@Stream` and yields. JavaScript,
TypeScript, Python, PHP, Kotlin, and C# support this shape. Java and Rust remain
single-response. Each yielded value is a bounded Handler Protocol event carried
as SSE. Deadline, reader failure, subscriber disconnect, and normal exit all
settle or terminate the complete process group within a bound.

Runtime overrides are `YON_JAVASCRIPT_RUNTIME` and `YON_PYTHON_RUNTIME`; removed
`TACHYON_*` spellings are not aliases.

## Consequences

- `TY2015` identifies missing layer declarations and `TY1502` identifies
  removed interpreter registration.
- `ty migrate check` directs arbitrary handlers to `@Delegate` + `@Relay`;
  `ty doctor` no longer probes interpreter registrations.
- The server knows whether it must emit SSE headers before invoking a route.
- Compiled relay shims remain language-owned protocol preludes, while the
  Tachyon supervisor retains authoritative deadline and process-group cleanup.
