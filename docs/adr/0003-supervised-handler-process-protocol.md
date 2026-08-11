# ADR 0003: Supervised Handler Process Protocol

- Status: Accepted
- Date: 2026-07-26

## Context

Yon handlers may be implemented in multiple languages. Tachyon needs one
server lifecycle, stable cancellation, bounded resources, actionable errors,
and cross-language behavior without embedding every language runtime in the
core.

Newline-delimited protocols are simple but make framing, oversized messages,
and protocol recovery more fragile. Shell invocation introduces injection and
quoting differences.

## Decision

Run language handlers as directly spawned, supervised child processes.

Handler Protocol v1 uses:

- a four-byte unsigned big-endian frame length;
- one UTF-8 JSON envelope per frame;
- a maximum frame of 16 MiB unless a future protocol version negotiates
  another limit;
- stable request IDs;
- explicit request, response, and cancellation envelope kinds;
- stdout for framed protocol only and stderr for bounded diagnostics;
- deadlines, cancellation, concurrency, and lifecycle owned by the supervisor.

Never invoke a handler through a shell. Pass executable and arguments as
separate operating-system values. Allowlist inherited environment variables.

A route may discover multiple same-level `yon.*` handlers in deterministic
source order. Each invocation selects one handler and one HTTP method; no
build-time route context is composed (ADR 0016).

## Consequences

- Every adapter can run the same real-process contract corpus.
- Handler crashes and protocol violations are isolated from the server.
- Large streaming bodies need a future streaming extension rather than
  unbounded frames.
- Runtime discovery and packaging are explicit per-language responsibilities.
- Process startup cost must be addressed by measured pooling, not hidden
  global processes.

## Rejected Alternatives

- **Embed every runtime**: expands attack surface and couples releases to
  multiple language engines.
- **Shell scripts as the common boundary**: unsafe and platform-dependent.
- **NDJSON without length framing**: weaker recovery and size enforcement.
- **Last handler wins during context merge**: nondeterministic and difficult to
  audit.

## Acceptance Gate

- Handler Protocol v1 meta-validates and rejects an invalid fixture;
- the threat model covers framing, resource exhaustion, environment leakage,
  cancellation, and process reaping;
- future adapters must pass the same corpus against real child processes.
