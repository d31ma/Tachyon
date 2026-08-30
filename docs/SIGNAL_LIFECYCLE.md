# CLI Signal Lifecycle

Tachyon's long-running `serve` (`dev`), `preview`, and `bundle --watch`
commands install their supported operating-system signal handlers before they
announce readiness. Lifecycle events are compact JSON lines on standard error;
normal readiness output remains on standard output.

On macOS and Linux, the installed signals are `SIGINT` and `SIGTERM`. On
Windows, they are `CTRL_C` and `CTRL_BREAK`. Windows `TerminateProcess`, Unix
`SIGKILL`, out-of-memory termination, and host shutdown cannot be observed by
this interface.

## Events

Every event has `event_version: 1`, the canonical `command`, and the host
`platform`. Startup reports the handlers that were installed:

```json
{"event":"runtime.signal_handlers_ready","event_version":1,"command":"serve","platform":"macos","installed":["SIGINT","SIGTERM"],"unavailable":[]}
```

If registration fails, Tachyon reports a privacy-safe warning containing only
the error kind and numeric operating-system error. A registration failure does
not stop the command and is never interpreted as a received signal.

```json
{"event":"runtime.signal_handler_unavailable","event_version":1,"command":"serve","platform":"macos","signal":"SIGTERM","error_kind":"other","raw_os_error":null}
```

The first observed signal identifies the reason and requests graceful shutdown:

```json
{"event":"runtime.shutdown_requested","event_version":1,"command":"serve","platform":"macos","reason":"signal","signal":"SIGTERM","signal_code":15}
```

That first request is not a common wall-clock bound across all three commands.
The development server owns and bounds its internal watcher, worker, response,
and handler settlement. Preview may still await Axum's cooperative connection
drain. `bundle --watch` may finish or observe its synchronous fingerprint pass
or the current bounded build before returning to the watch loop. Operators and
process supervisors must enforce their own grace period and send a second
supported signal when they require a hard escape.

A second observable signal received while graceful shutdown is still in
progress requests immediate termination. Tachyon flushes a
`runtime.shutdown_forced` event first, then exits with `130` for SIGINT or
CTRL-C, `143` for SIGTERM, and `131` for CTRL-BREAK. Operating systems may
coalesce identical signals delivered before the first notification is read.

Lifecycle events never contain project paths, process or parent identifiers,
arguments, environment values, requests, credentials, or authored output. A
supervisor should add its own timestamp and process identity when it captures
the standard-error stream.
