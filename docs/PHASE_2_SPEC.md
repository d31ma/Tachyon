# Phase 2 Yon Handler Boundary

## Outcome

A developer can invoke a JavaScript or Python Yon handler through the compiled
Rust `ty` executable. Tachyon directly spawns and supervises one isolated child
process per invocation, communicates only through Handler Protocol v1, and
returns a validated response or stable diagnostic.

Phase 2 establishes execution and isolation. It does not inject handler values
into HTML or merge route context; those semantics belong to Phase 3.

## Public Workflow

```text
ty handler invoke server/routes/products/yon.js \
  --project . \
  --route /products \
  --method GET
```

The command writes one Handler Protocol v1 response as JSON to stdout.
Diagnostics use the global human or JSON diagnostic format and stderr.

The invocation accepts:

- a project-relative `yon.js` or `yon.py` source;
- an explicit route and HTTP method;
- optional UTF-8 request body and repeated `name=value` headers;
- an optional request ID;
- a bounded timeout from 1 through 300,000 milliseconds;
- repeated `--allow-env NAME` entries;
- explicit JavaScript or Python runtime executable overrides.

Ctrl-C cancels the invocation through the same cancellation path used by the
library API.

## Handler Authoring Contract

JavaScript handlers use an ESM named or default `Handler` class:

```javascript
export class Handler {
  static async GET(request) {
    return { products: [] }
  }
}
```

Python handlers use a `Handler` class:

```python
class Handler:
    @staticmethod
    async def GET(request):
        return {"products": []}
```

The selected uppercase static method receives the validated request envelope as
a JavaScript object or Python dictionary. Synchronous and asynchronous methods
are accepted. A successful return value must be JSON-serializable and becomes a
UTF-8 JSON response body with status 200 and
`content-type: application/json; charset=utf-8`.

A missing class or method, import failure, thrown exception, or
non-serializable result becomes a bounded Handler Protocol error response.
Application console/print output is redirected to bounded stderr and never
shares protocol stdout.

## Source and Route Discovery

- `server/routes/**/yon.js` selects adapter `javascript.v1`.
- `server/routes/**/yon.py` selects adapter `python.v1`.
- Both may exist at the same route and are ordered by portable source path.
- Handler-only routes are API routes.
- A handler may accompany a Tac or Yon view without claiming a second view.
- Two view sources still conflict.
- Other `yon.*` companions remain rejected until their adapter phase.
- Handler files and their ancestors must be regular, non-symlinked,
  project-contained paths.
- Handler source is UTF-8, NUL-free, and at most 1 MiB.

Route Manifest v1 records discovered handlers. Handler-only routes advertise
the protocol's HTTP method set; view-only routes retain GET and HEAD.

## Process and Protocol Contract

- The executable and each argument are passed directly; no shell is used.
- Frames use a four-byte unsigned big-endian length and one UTF-8 JSON
  envelope.
- A complete frame, including JSON, cannot exceed 16 MiB.
- Exactly one request and at most one matching response are accepted.
- Trailing stdout, malformed JSON, invalid response fields, a mismatched
  request ID, oversized output, or premature exit is a protocol violation.
- stderr is continuously drained, retained up to 64 KiB, and reported only in
  bounded diagnostics.
- The default deadline is 30 seconds. Deadline time includes concurrency queue
  time, process startup, handler work, framing, and process exit.
- Cancellation writes a protocol cancellation frame, allows a 100 ms grace
  period, then kills and reaps the child.
- Timeout, cancellation, crash, and protocol failure never reuse the process.
  A later invocation starts a clean child.
- A supervisor admits at most 16 concurrent processes by default.

## Environment Policy

The child environment is cleared before launch. Tachyon inherits only the
minimum runtime variables required by the host platform and names explicitly
allowlisted by the developer. Invalid or non-Unicode allowlist names fail
closed. Tachyon never logs environment values.

Runtime lookup is explicit per adapter. A missing runtime is an actionable
diagnostic, not a fallback to another language or a shell command.

## Stable Failure Families

- `TY2001`–`TY2009`: invalid handler source, request, runtime, or environment.
- `TY2101`–`TY2109`: spawn, framing, protocol, exit, or stderr failures.
- `TY2110`: deadline exceeded.
- `TY2111`: invocation cancelled.
- `TY2201`–`TY2209`: adapter-reported handler authoring or execution failures.

Messages are bounded and may include a project-relative source path and
truncated stderr. They never include environment values or an unbounded stack
trace.

## Acceptance Criteria

- Typed Rust envelopes serialize to all Handler Protocol v1 wire shapes.
- JavaScript and Python pass one shared behavior corpus using real runtimes.
- The compiled `ty` binary invokes both languages successfully.
- Requests, response bodies, headers, request IDs, sync/async methods, and
  Unicode round-trip consistently.
- Missing runtimes/classes/methods, exceptions, malformed/oversized/trailing
  protocol output, mismatched IDs, source escapes, symlinks, and stderr floods
  fail safely.
- Deadline and explicit cancellation send a cancel frame, kill when necessary,
  and reap the process.
- A crash is isolated and the next invocation succeeds.
- A secret-canary variable is absent unless its name is explicitly allowed.
- Workspace quality, coverage, supply-chain, legacy compatibility, macOS,
  Linux-container, Windows-buildability, and native CI gates pass.

## Out of Scope

Handler pooling, streaming frames, TypeScript and additional languages, route
context extraction or merging, HTML injection, control tags, components,
bindings, islands, production HTTP routing, runtime installation, OS-level
CPU/memory sandboxing, JavaScript dependency imports, filesystem/network
capabilities, and native rendering are later phases.
