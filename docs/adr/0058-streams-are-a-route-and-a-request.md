# ADR 0058: Streams Are a Route and a Request

- Status: Accepted
- Date: 2026-09-26
- Amends: [ADR 0048](0048-yon-routes-come-from-annotations.md) (a streaming
  route has its own route annotation) and
  [ADR 0017](0017-mandatory-yon-layers-relay-and-streaming.md) (how a stream
  ends)
- Extends: [ADR 0013](0013-semantic-hot-updates.md) (server-sent events, not
  WebSockets, now also carry application streams to the browser)
- Preserves: [ADR 0055](0055-annotation-arguments-use-types.md) (typed events)
  and [ADR 0056](0056-every-annotation-checks-its-target.md) (the rule table)

## Context

A Yon stream was a `@GetRoute` method that was a generator, with an optional
`@Stream` marker. A generator under any verb streamed, so the method name
(`getTicks`) did not say that it streamed, and two spellings meant the same
thing. The browser side had no annotation: every page wrote about forty lines
of `EventSource` code for opening, closing, timeouts and checking messages.

Spring gives each outgoing call a declarative method (`@GetExchange`) and
each incoming verb one mapping. WebSockets stay out of scope: SSE is the one
push channel (ADR 0013), and a WebSocket decision gets its own ADR.

## Decision

### StreamRoute (Yon)

- `@StreamRoute(path?)` replaces `@GetRoute` with `@Stream`. It means the
  method answers GET requests with a stream of server-sent events. The method
  is a generator in the language's own syntax, as before.
- The verb-prefix rule applies: the name starts with `stream` in the
  language's casing (`streamTicks`, `StreamTicks`, `stream_ticks`).
- A generator under `@GetRoute` or another verb route is refused with a hint.
  `@Stream` is retired in the rule table and refused with its replacement.
  `ty migrate fix --write` turns `@GetRoute` with `@Stream` into
  `@StreamRoute` and renames a `get` method to `stream` when the old name is
  used nowhere else in the file; every other case is a manual step.
- The same eight languages stream (JavaScript, TypeScript, Python, PHP,
  Kotlin, C#, Dart and Ruby). Java, Rust and Go refuse `@StreamRoute`.
- When the method finishes normally, the server sends `event: end`. A failed
  stream still ends with `event: error`.
- `/api.json` marks the route with `"stream": true` and
  `"produces": "text/event-stream"`.

### StreamRequest (Tac)

- `@StreamRequest(path?)` goes on a method of an `@HTTPClient` class. The path
  is added to the class path. The method takes exactly one parameter, and the
  parameter's model type (JSDoc `@param {Tick} tick` or `tick: Tick`) says
  what each message holds.
- One rule decides where a message goes: Tac checks it against the model
  (the same description `@OnEvent` uses), passes it to the method, and then
  publishes it as that model's typed event. A message that does not fit is
  dropped and counts as a listener failure. Every `@OnEvent(Tick)` listener,
  in any Tac language, receives the message through the page's event bus.
- The browser event bus owns the `EventSource`. It opens the stream when the
  client is first used, which is when a component or service that needs the
  client starts. It closes the stream when that owner goes away or the server
  sends `end`. After an error it reconnects after 1, 2, 4, 8 and 16 seconds
  and then stops; a message resets the count. Sixty seconds without a message
  counts as an error.
- `@StreamRequest` is written in JavaScript or TypeScript, because the
  browser's JavaScript holds the connection. Rust, Go, C#, Dart, Swift,
  Kotlin and Ruby services receive the messages with `@OnEvent`; the rule
  table refuses `@StreamRequest` there with that advice. An `EventSource`
  sends no headers or body, so `@StreamRequest` takes no parameter markers.
- The rule table refuses `@StreamRequest` outside an `@HTTPClient` class and
  together with another request or route annotation. A method with more or
  fewer than one parameter, or without a model type, is refused when the
  page is built.

## Consequences

The realtime example has no hand-written `EventSource`: a two-line client and
an `@OnEvent` listener replace it. The browser event bus
(`template/application-events.js`) grew from 11,104 to about 12,300 bytes;
`scripts/application-events-test.mjs` checks the new budget and the stream
behaviour with a fake `EventSource`. `tac-client.js` did not change. Existing
controllers that stream must be renamed; the migration fix does this for the
common case.

## Amendment (2026-09-29)

The `@StreamRequest` method belongs to an `HTTPClient` contract (ADR 0048
amendment). In JavaScript and TypeScript the contract is a class whose
methods have empty bodies, so the stream method keeps its empty body and
Tachyon writes it.

## Amendment (2026-10-04): Go streams with iter.Seq

Go has a native generator since Go 1.23: a range-over-function iterator.
Tachyon needs Go 1.27, so the reason Go refused `@StreamRoute` is gone.

- A Go `// @StreamRoute` method returns `iter.Seq[Model]`. Its function calls
  `yield(item)` once for each value and returns when `yield` returns false.
  When the stream can fail, the method returns `iter.Seq2[Model, error]`; the
  first non-nil error ends the stream. Any other result is refused with TY2015,
  and the message names the two shapes.
- The Go prelude ranges over the iterator and writes each item as one
  length-prefixed Handler Protocol event frame, like Dart and Lua. When the
  iterator returns, the process exits and the server sends `event: end`. An
  error, a panic or an item that cannot be written as JSON sends the fixed
  TY2204 error frame, so the browser gets `event: error` and never the error
  text.
- Backpressure is the pipe: `yield` blocks while the server has not read the
  last frames, so a slow reader slows the producer.
- When the reader goes away, the server stops the process group at once, as in
  every language. Deferred Go code does not run then.
- The time limit does not change: a stream has the request time limit, and at
  the limit the server stops the process and sends `event: error`, after which
  the browser opens the stream again. There are no long-lived workers. A Go
  route that takes `*http.Request` now gets a request context that ends at the
  invocation deadline (a little before the server's limit), so a log follow can
  watch `request.Context().Done()` and end with `event: end` when it should
  finish for good.
- A channel (`<-chan Model`) was rejected. It needs a goroutine, a second
  channel for errors and a context to stop the sender, while an iterator has
  all three in one function and a test unit collects it with
  `slices.Collect`. Spring's `Flux` maps the same way: items, one error, the
  end, and cancellation.
- The cohesion rule (ADR 0063) does not count Go's `if !yield(item) { return }`
  stop check as a decision. It belongs to `yield`, which stops by itself in
  the other languages, so a private iterator method keeps its one loop.
- Java, Groovy and Rust still refuse `@StreamRoute`.

## Amendment (2026-10-11): Go stream clients

`// @StreamRequest` is also written on a Go HTTPClient interface method. The
page's JavaScript still holds the `EventSource`: the Go service that keeps
the client in a field declares the stream, the page's event bus opens it with
the same rules, and the Go service receives each message with `// @OnEvent`
on a method whose parameter is the message model. See ADR 0090, "Go streams
and sockets". The other compiled Tac languages still refuse `@StreamRequest`.

## Amendment (2026-10-17): Swift stream clients

`@StreamRequest` is also written on a Swift HTTPClient protocol method, held
by the page for the Swift web service that keeps the client in a property,
as for Go. See ADR 0090, "Swift streams and sockets".
