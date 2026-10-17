# ADR 0090: Compiled Dart HTTP Clients Await the Browser Host

- Status: Accepted
- Date: 2026-10-01
- Relates to ADR 0033 (Dart WebAssembly), ADR 0084 (page placement), ADR 0087 (injected HTTP and caching)

## Decision

A compiled browser Dart HTTPClient uses the default same-origin transport when
its Setup has no matching HTTP adapter. Its existing Future method awaits a
separate asynchronous browser host. Ordinary synchronous APIs keep their
current host ABI, and a matching custom Setup adapter takes precedence.
No worker placement or cross-origin-isolation headers are required.

The generated method converts the JSON answer with the existing typed model
reader. Invalid shape failures become BehaviorError with code HTTP_RESPONSE.
Request methods, path variables, query, headers and optional JSON bodies use
the existing HTTP interface annotations. A service receives the client through
Needs or NeedsAll; application services do not call fetch themselves.

The host accepts at most 65,536 UTF-8 request bytes, 64 concurrent requests,
a 60-second maximum deadline and 1 MiB of response data. It refuses another
origin, redirects, cookies supplied as headers and unsafe request headers.
Fetch uses credentials omit. Ordinary requests use no-store; Cached public
GETs use default cache and the first read after a successful clear/update
uses no-cache. A request with Authorization uses no-store. Failed mutations
do not invalidate a successful cached read.

AbortController implements deadlines and cancellation. Each compiled service
captures its own BehaviorSignal through the existing owner factory. A Dart
zone carries that owner across Future continuations and late initialization;
removing the owner disposes its contexts and aborts outstanding fetches.
Another service owner keeps its own signal. A failed request returns a bounded
error code without raw response text or host exception details.

Native Dart is not an available native companion target. This decision does
not install a browser transport in Yon or bypass native HTTP refusals. Existing
custom language adapters remain available in their supported environments.

## Scope and evidence

The executable website example is
`website/content/docs/calling-the-server/browser-http-dart/`.
`scripts/browser-http-browser-test.mjs` compiles it once and serves its actual
Yon controllers. It checks all three required browser engines.
`scripts/browser-http-runtime-test.mjs` directly tests the production async
host boundaries. Focused Rust tests also execute the actual Dart VM runtime.
These checks passed in Chromium, Firefox and WebKit when this ADR was accepted.

This decision covers Dart. The other compiled languages, TestSetup and Fake
replacement, migration help for clients a service builds itself, and the
remaining first-app tabs are not covered yet. The
next step is staged async interop that keeps each runtime on the page. C# and
F# Task interop come next; each other language needs its own decision.
The existing sync Rust/Swift ABI cannot serialize a Promise into a response,
and Binaryen 133 Asyncify rejects references live across the import. This ADR
does not choose a global worker architecture or change authored async contracts
for the other languages.


## Bounded .NET browser extension — 2026-10-03

C# Task ownership, default HTTP and captured registered scalar record results
are qualified in compiler27. Native result restrictions remain unchanged.
The cumulative C# first-app candidate now passes three actual authored SDK
cases and an ordinary post-Test .NET Wasm build with the existing Yon endpoint
in Chromium, Firefox and WebKit. Canonical tutorial files are identical to
those actual staged sources. A selected genuine TestSetup may construct its
stateless helper adapter; production intake validates that ownership, then
omits helpers reachable only from the test. This adds no managed helper
lifetime or injection contract.

F# uses the same .NET host for browser `Task` and `Task<BehaviorJSON>` results.
Its genuine ordinary Wasm/Yon proof passes all three browser engines: default
HTTP, explicit custom adapter precedence, unsafe-path refusal, disposal of a
pending request, captured stale-owner refusal, a working replacement and zero
late changes. Raw helper markers are validated before F# annotation lowering;
helpers remain explicitly constructed and caller-owned.

The actual debug compiler source is
`5d3d4a728f8cb361bc7e6e9fd486563608e38f05`; its production changes are rooted
in `2a4aeac3cb5dfe62eac3f3fc26bb3e55255ddd51`. The F# fixture constants are
saved separately in `8445c01943758b6475fbbb843f85f1d1706320b8`. External workspace
receipts are `FIRST-APP-CSHARP-QUALIFICATION.json`,
`FIRST-APP-CSHARP-CANONICAL-SOURCE.json`, `FSHARP-HTTP-QUALIFICATION.json` and
`QUEUE28-SOURCE-CONTROLS.json`. Canonical CLI coverage and coherent optimized
compiler28/final aggregate/website validation remain pending. Compiler27 is
still the latest complete qualified handoff.

This extension does not qualify F# DTO/list/ValueTask results, native async,
async lifecycle adapters, shared compiled page providers or F# cache behavior.
It adds no new browser worker placement or whole transport-matrix acceptance.

## Rust browser extension — 2026-10-03

Rust uses its own async pattern. An HTTPClient trait method is an
`async fn` that returns `BehaviorResult<T>`. `T` is `BehaviorValue`, `()`,
a model from `client/models`, a list of models, an `Option` or a plain value.
Tachyon writes the method. A Setup adapter of the same name answers first;
otherwise the request goes to the same browser host as Dart and C#, with the
same limits, omitted credentials and cache rules. A `Cached` miss sends a
public read; the first miss after a successful `ClearCache` or `UpdateCache`
revalidates. An answer of the wrong shape fails with `HTTP_RESPONSE`.
`Retry`, `MaxAtOnce` and `AllOrNothing` are refused on async client
methods for now.

A service gets the client with `#[Needs] name: std::sync::Arc<Client>`; its
other fields start at their `Default` value. A service method may be
`async fn` and return `()`, a crossing type or `BehaviorResult<T>`; an error
reaches the page as a rejected promise with the error code. The WebAssembly
module keeps the pending futures in a small executor. The page starts and
cancels fetches through `tac_host_http_start` and `tac_host_http_cancel`,
settles them through the `tac_http_settle` export, and learns that a task
ended through `tac_host_task_done`. A pending method holds its service
exclusively: until it ends, the page reads the fields as they were when the
method started, and other calls, field writes and events to that service run
after it, in order. Disposing the owner drops its tasks first, which aborts
their fetches and rejects their promises with `CANCELLED`, and then runs the
PreDestroy hooks.

In a Tac test unit, `self.construct()` builds a service through the
constructor that `Needs` produces. A Fake field
`std::cell::RefCell<Option<std::sync::Arc<Client>>>` supplies the client; while
it is empty, the service gets a client bound to the test's BehaviorContext, so
the TestSetup adapter answers. Test cases may be `async fn`. A test unit and a
native Rust companion have no browser transport: a request without an adapter
fails with `HTTP_MISSING`. `ty migrate check` reports a service that builds
its client itself (`service.builds_client`).

Evidence: `scripts/browser-http-browser-test.mjs --language=rust` builds
`website/content/docs/calling-the-server/browser-http-rust/` and checks it in
Chromium, Firefox and WebKit; `client_tests::rust_construct_tests` runs the
Fake and typed-answer cases natively.

## Go browser extension — 2026-10-03

Go keeps its own pattern: a call that waits blocks its goroutine. An
HTTPClient interface method returns `(Model, error)` or
`(BehaviorValue, error)`; the answer is read with `encoding/json` into the
model from `client/models`, and a model answer that is not an object fails
with `HTTP_RESPONSE`. A service gets the client with `// @Needs` above a field
such as `greetingClient *GreetingClient`.

The page calls every Go service member through a new goroutine. When the call
finishes without waiting, the answer is returned at once, as before. When the
call reaches the browser transport, the goroutine releases the service lock,
starts the fetch through the same host as the other languages and waits on a
channel; the page gets `{"task": id}` and a promise that settles when the
goroutine ends. While a Go call waits, other calls to the service run normally,
as in JavaScript. Disposing the owner aborts its pending fetches, and the
waiting call gets `CANCELLED`. A call that waits from a getter, a setter or
`OnStart` is refused with `HTTP_REQUEST`. The same browser host rules apply
(same origin, omitted credentials, `Cached` public reads, revalidation after
`ClearCache` or `UpdateCache`).

In a Go test unit, `test.construct(service)` fills the `Needs` fields from the
`// @Fake` fields of the test; an empty Fake client field gets a client bound
to the test's BehaviorContext (`NewGreetingClientWith`), so the TestSetup's
`Exchange` answers. Tests have no browser transport (`HTTP_MISSING`).
Coverage records no calls inside generated clients.

Evidence: `scripts/browser-http-browser-test.mjs --language=go` builds
`website/content/docs/calling-the-server/browser-http-go/` and checks it in
Chromium, Firefox and WebKit; `client_tests::go_construct_tests` runs the Fake
and typed-answer cases.

## Kotlin browser extension — 2026-10-04

A Kotlin HTTPClient is an interface whose methods are `suspend fun` without a
body. Tachyon writes each method: it goes through the behavior context's
suspend path (`invokeAwait` for reads, `mutateAwait` for `ClearCache` and
`UpdateCache`) and reads the answer with `Behaviors.answer` into the declared
type: a data class from `client/models`, a list, a plain value, `Any?` or
`Unit`. A model answer that is not an object, or a list answer that is not a
list, fails with `HTTP_RESPONSE`. Errors are `Behaviors.Failure` values whose
`code` names the problem. Retry, concurrency limits, transactions, `Check`
and `Pageable` are refused on suspend client methods. The Setup's adapter
answers first; without one, the request goes to the same browser host as the
other languages (same origin, omitted credentials, `Cached` public reads,
revalidation after a successful `ClearCache` or `UpdateCache`). The generated
client has two constructors: `Client()` builds its context from its Setup, and
`Client(context)` uses a given BehaviorContext.

A service gets the client with `@Needs private val name: Client`; on the page
Tachyon fills the field with `Client()`. A service method may be a
`suspend fun`. The Kotlin/Wasm module starts it with the standard library's
`startCoroutine`; when it finishes without waiting, the page gets the answer
at once. When it waits for the browser transport, the module keeps the
continuation, starts the fetch through `httpStart` on the Kotlin host, and the
page gets `{"task": id}` and a promise. The page settles the fetch through the
`tacHttpSettle` export, the module resumes the continuation with the service
as the executing owner, delivers its events and reports the end through
`taskDone`. A waiting method does not hold its service: other calls, field
reads and writes run while it waits, as in JavaScript and Go. Disposing the
owner drops its waiting continuations first, which aborts their fetches and
rejects their promises with `CANCELLED`, and then runs the PreDestroy hooks.
An Android companion has no browser transport: a suspend method answers
`HTTP_MISSING`.

In a Kotlin Tac test unit, `construct(Service::class)` calls the constructor
that `Needs` produces. A Fake field `var name: Client? = null` supplies the
client; while it is `null`, the service gets `Client(behaviors)`, bound to the
test's BehaviorContext, so the TestSetup adapter answers. Test cases may be
`suspend fun`; Tachyon runs them to the end and refuses a case that still
waits. Test units run on the JVM, so a request without an adapter fails with
`HTTP_MISSING`.

Evidence: `scripts/browser-http-browser-test.mjs --language=kotlin` builds
`website/content/docs/calling-the-server/browser-http-kotlin/` and checks it in
Chromium, Firefox and WebKit; `client_tests::kotlin_construct_tests` runs the
Fake and typed-answer cases.

## Swift browser extension — 2026-10-04

A Swift HTTPClient is a protocol whose methods are `async throws` and have no
body. Tachyon writes each method through the behavior context's async
`invoke`, so `Cached`, `ClearCache`, `UpdateCache`, retries, concurrency
limits and transactions keep working as before. A method may return
`BehaviorJSON`, nothing, or any `Decodable` type, such as a `Codable` struct
from `client/models`, a list of them, an optional or a plain value;
`Behaviors.answer` decodes the answer and an answer of the wrong shape fails
with `HTTP_RESPONSE`. Errors are `BehaviorError` values whose `code` names the
problem. The Setup's adapter answers first; without one, the request goes to
the same browser host as the other languages (same origin, omitted
credentials, `Cached` GET reads are public reads, and the first miss after a
successful `ClearCache` or `UpdateCache` revalidates). The generated client
has two initializers: `Client()` builds its context from its Setup, and
`Client(context)` uses a given BehaviorContext.

A service gets the client with `@Needs private let name: Client`; on the page
Tachyon fills the field with `Client()`. An `async` service method becomes a
task member. The Swift WebAssembly module starts it in a `Task` that carries
the running task in a task-local value and drives the cooperative executor
(`MainActor.executor.runUntil`) until the task ends or has nothing left to
run. The page always gets a promise for an `async` method; when the method
ends without waiting, the promise settles at once. When it
waits for the browser transport, a checked continuation is parked on a
ticket, the fetch starts through the `tac_host_http_start` import, and the
page gets `{"task": id}` and a promise. The page settles the fetch through
the `tac_http_settle` export; the module resumes the continuation with the
service as the executing owner, delivers its events and reports the end
through `tac_host_task_done`. Rust and Swift share this page glue
(`TacWasmTasks`). A waiting method does not hold its service: other calls,
field reads and writes run while it waits, as in JavaScript, Go and Kotlin.
A request from outside a task, such as from a getter, fails with
`HTTP_REQUEST`. Disposing the owner cancels its waiting tasks first
(`Task.cancel()`, their fetches are aborted, their continuations resume with
`CANCELLED` so the task unwinds) and rejects their promises with `CANCELLED`;
then the PreDestroy hooks run.

A native macOS or iOS companion keeps its behaviour: an `async` method still
runs to the end on the owner thread before the call answers, and a client
without an adapter fails with `HTTP_MISSING`. A Swift test unit runs natively
too, so a request without an adapter fails with `HTTP_MISSING`. In a Swift
test unit, `let service: Service = try construct()` calls the initializer that
`Needs` produces. A Fake field `var name: Client? = nil` supplies the client;
while it is `nil`, the service gets `Client(behaviors)`, bound to the test's
BehaviorContext, so the TestSetup adapter answers.

Evidence: `scripts/browser-http-browser-test.mjs --language=swift` builds
`website/content/docs/calling-the-server/browser-http-swift/` and checks it in
Chromium, Firefox and WebKit; `client_tests::swift_construct_tests` runs the
Fake and typed-answer cases.

## Java browser extension — 2026-10-11

A Java HTTPClient is an interface whose methods have no body. In Tac, a
method may return a model from `client/models`, a `List` of models, `String`,
a number, `boolean`, `Object` or `void`. Tachyon writes the method through the
behavior context's existing `invoke` and `mutate`, and reads the answer with
`Behaviors.answer` and the `TacModels` and `TacValues` readers. On the JVM,
the Tac test runner adds these two readers to the test program. A model answer
that is not an object, or a list answer that is not a list, fails with
`HTTP_RESPONSE`. Errors are `Behaviors.Failure` values whose `code` names the
problem. The page receives that code too.
The Setup's adapter answers first. Without one, the request goes to the same
browser host as the other languages. It uses the same origin and omits
credentials. A `Cached` GET is a public read, and the first miss after a
successful `ClearCache` or `UpdateCache` revalidates. A failed mutation keeps
the cached read. Yon Java clients still return `Object`. `restClient` keeps
its adapter-only path. The generated client has two constructors:
`Client()` builds its context from its Setup, and `Client(Behaviors.Context)`
uses a given context.

A service gets the client from a `private final Client name;` field with
`@Needs` on the line above. On the page, Tachyon fills the field with
`new Client()`. Java has no async keyword, so Java follows the Go model. The
`tacInvoke` export runs every page request (init, call, get, set, event and
dispose) in its own TeaVM fiber. It uses `org.teavm.runtime.Fiber.start`,
which runs the fiber at once on the caller's stack. When the fiber ends
without waiting, the page gets the answer at once, as before. When a method
call or an event listener reaches the browser transport, a TeaVM `@Async`
method parks the fiber on a ticket. The fiber releases the service, and the
page gets `{"task": id}` and a promise. The fetch starts through the
`httpStart` host call. The page settles it through the `tacHttpSettle` export,
which resumes the fiber with the service as the executing owner. When the
fiber ends, its events are delivered and `taskDone` reports the end. While a
call waits, other calls to the service run, as in Go and Kotlin. A getter, a
setter or `OnStart` cannot wait: a request there fails with `HTTP_REQUEST`.
Disposing the owner first drops its parked fibers. This removes their tickets,
aborts their fetches and rejects their promises with `CANCELLED`. Then the
OnStop hooks run. A disposed owner gets no further change notices, even from
a call that fails because the owner is gone. Kotlin and Java share the page
glue (`TacTicketHttp`).

Everything runs inside a fiber because TeaVM marks virtual call sites
conservatively as asynchronous. An asynchronous method reached outside a
fiber traps in WebAssembly, even when it would not wait. The design relies on
`org.teavm.runtime.Fiber`, which is an internal TeaVM API and not a public
one. This is acceptable only under the exact TeaVM 0.15.0 pin. Upgrading
TeaVM must re-run the Java contract test before the pin moves.

In a Java Tac test unit, `construct(Service.class)` calls the constructor
that `Needs` produces. A Fake field `Client name = null;` supplies the client.
While it is `null`, the service gets `new Client(this.behaviors)`, which is
bound to the test's context, so the TestSetup adapter answers. Test units run
on the JVM and have no browser transport. A request without an adapter keeps
the existing code `http-adapter`. A test unit loads only the TestSetup that
its `Test` names, as in every language, so a context for an assigned Fake
comes from a production Setup.

Evidence: `scripts/java-http-browser-contract-test.mjs --run` and
`scripts/browser-http-browser-test.mjs --language=java`. The first builds the
contract fixture, and the second builds
`website/content/docs/calling-the-server/browser-http-java/`. Both check
Chromium, Firefox and WebKit. `client_tests::java_http_construct_tests` runs
the Fake and typed-answer cases on the JVM.

## Ruby browser extension — 2026-10-11

A Ruby HTTPClient is a class whose methods have empty bodies. Tachyon writes
each method through the behavior context's existing `invoke`, so `Cached`,
`ClearCache`, `UpdateCache`, retries and the other method annotations keep
working. The YARD `@return` comment above the method names the answer: a
model from `client/models`, an `Array` of models, `String`, `Integer`,
`Float` or `Boolean`. `TacValues.answer` reads the answer into that type.
The program registers the shapes of every client model for this reader. A
model answer that is not an object, a list answer that is not a list or a
plain answer of another type fails with `HTTP_RESPONSE`. `Hash`, `Object` and
`void` keep the answer as it is, so existing clients are unchanged. Errors are
`BehaviorError` values whose `behavior_code` names the problem, and the page
receives that code too.
The Setup's adapter answers first. Without one, the request goes to the same
browser host as the other languages. It uses the same origin and omits
credentials. A `Cached` GET is a public read, and the first miss after a
successful `ClearCache` or `UpdateCache` revalidates. A failed mutation keeps
the cached read. `rest_client` keeps its adapter-only path. The generated
client takes an optional context: `Client.new` builds its context from its
Setup, and `Client.new(context)` uses a given BehaviorContext.

A service gets the client with `#[Needs]` above `attr_reader :name`, with the
`# @return [Client]` comment between them, as for stores and converters.
Tachyon turns the reader into a private method that builds the client once.
The page does not see the field. Ruby has no async keyword, so Ruby follows
the Go and Java model. Every page request (init, call, get, set, event and
dispose) runs in its own Ruby `Fiber`. When the fiber ends without waiting,
the page gets the answer at once, as before. When a method call or an event
listener reaches the browser transport, `Fiber.yield` parks the fiber on a
ticket. The service is released, and the page gets `{"task": id}` and a
promise. The fetch starts through the existing host channel of the runtime.
The page settles it through a `tachyon-http:settle` request to the existing
`tac_invoke` export. That request resumes the fiber with the service as the
executing owner. When the fiber ends, its events are delivered and the page
learns the answer. While a call waits, other calls to the service run, as in
Go, Kotlin and Java. `OnStart` cannot wait: a request there fails with
`HTTP_REQUEST`. At most 64 requests wait at once. Disposing the owner first
drops its parked fibers. This removes their tickets, aborts their fetches and
rejects their promises with `CANCELLED`. Then the OnStop hooks run. Kotlin,
Java and Ruby share the page glue (`TacTicketHttp`).

The design needs no change to the runtime in `runtimes/ruby-web`, so the
cached runtime and its fingerprint stay the same. It relies on the pinned
mruby commit `7b4e0f3b7b3db3595332c4e1f4c97afcea183e5e`: there a fiber that
`tac_invoke` starts can yield through `public_send`, `send`, `instance_exec`,
`Method#call`, blocks, `ensure` and `retry`, and a later `tac_invoke` can
resume it. A probe on this runtime showed that. Upgrading mruby must re-run
the Ruby transport test before the pin moves.

In a Ruby Tac test unit, `construct(Service)` gives the service its Fake
fields. A Fake field is `#[Fake]` above `attr_accessor :name`. While it is
`nil`, the service gets `Client.new(context)`, bound to the test's context,
so the TestSetup adapter answers. Test units run in CRuby and have no browser
transport. A request without an adapter keeps the existing failure
`Required behavior adapter is missing.` `ty migrate check` reports a service
that builds its client itself (`service.builds_client`). Method coverage
treats a Ruby method with an empty body as declared, so nobody has to test
the request methods that Tachyon writes.

Evidence: `scripts/browser-http-browser-test.mjs --language=ruby` builds
`website/content/docs/calling-the-server/browser-http-ruby/` and checks it in
Chromium, Firefox and WebKit.
`tac_ruby::tests::browser_http_tests` runs the page protocol on the pinned
runtime in Node, and `client_tests::ruby_http_construct_tests` runs the Fake
and typed-answer cases in CRuby.

## Amendment (2026-10-11): 1 MiB requests and the status of a failed answer

The host and every compiled client accept at most 1 MiB (1,048,576 UTF-8
bytes) of request instead of 65,536, so a page can send a file such as an
EML message. The server's own request body limit is also 1 MiB. Other
limits stay as above.

An `HTTP_STATUS` failure now carries the status number:
`{"error":{"code":"HTTP_STATUS","status":404}}`. The body of the answer
still stays out. Go reads it with `BehaviorErrorStatus(err)`, from the new
`Status` field of `BehaviorError`; the other languages keep the code only.

## Go streams and sockets — 2026-10-11

A Go HTTPClient interface method may carry `// @StreamRequest` or
`// @SocketRequest`, as in JavaScript and TypeScript (ADR 0058, ADR 0060).
The method takes one parameter whose model type says what each message
holds, for example `Receive(tick Tick)`, and has no result. It has no body in
the interface, and the generated client keeps an empty method. A Go client
with only stream or socket methods needs no Setup: it has no behavior context.

The page's JavaScript still holds the connection. A Go service that keeps the
client in a field (`// @Needs ticks *TicksClient`) declares the client's
streams in its `init` answer, next to its listeners. The page shell gives
them to the page's event bus, which opens each `EventSource` or `WebSocket`
when the service starts and closes it when the service's owner goes away.
The bus checks each message against the model, publishes it as that model's
event, and a Go `// @OnEvent` method whose parameter is the model receives it.
A Go service that does not hold the client opens nothing. The same limits
apply as in JavaScript: the page's own origin, JSON text, 64 KiB, at most 16
queued socket messages, and reconnects after 1, 2, 4, 8 and 16 seconds. A
service holds at most 16 such methods through its clients.

A socket client gets a generated `Send(message any) error`. It writes the
message as JSON with `encoding/json` and waits for the page like an HTTP
call: the goroutine releases the service, the bus sends or queues the text,
and `Send` returns `nil` or an error whose code is `HTTP_NETWORK` (for
example, 16 messages already wait for the socket to open), `HTTP_MISSING`
(the service does not hold the client, or there is no page) or `CANCELLED`
(the owner went away). Like an HTTP call, `Send` from a getter, a setter or
`OnStart` fails with `HTTP_REQUEST`; the bus opens the socket after `OnStart`.
A message larger than 64 KiB, or a value `encoding/json` cannot write, fails
with `HTTP_BODY`. Each socket client has its own sender on the service's page
shell, so a Go service may hold several socket clients. A Go test unit has no
page, so `Send` fails with `HTTP_MISSING`.

The bus changes in one place: a stream entry may name the property that
receives its `send`, so the Go shell's senders never replace a page method of
the service. Two one-statement functions of `application-events.js` now take
one line each, so the file stays inside its 14,338-byte budget.

Evidence: `tac_web::go_stream_tests` builds two real Go services to
WebAssembly and runs them under Node with the real event bus and fake
`EventSource` and `WebSocket` classes. Only the service that needs the
clients opens them; a valid tick reaches its `@OnEvent` method and an invalid
one is refused; a socket message reaches its listener; sends queued before
the socket opened are flushed in order, the seventeenth gets `HTTP_NETWORK`;
and disposing the owner closes both connections.

## Amendment (2026-10-11): Go services stop promptly when the page is left

The page stops every service on `pagehide` (when the page is not kept in the
back-forward cache), and that stop is a synchronous call into the Go
program. Before this amendment, stopping a service whose call waited for
HTTP sent `CANCELLED` straight to the waiting goroutine. Both Go runtimes
then run that goroutine before the synchronous call returns: the standard
runtime returns to JavaScript only when every goroutine is idle, and TinyGo
empties its run queue. The woken goroutine waited for the service lock in a
`runtime.Gosched()` loop, so it never became idle. As soon as the stop let
the scheduler run while it held the lock (an `OnStop` hook or the woken
call writing to the console, which calls back into Go in the standard
runtime), the standard runtime waited forever for the spinning goroutine
and the page froze until the browser abandoned the navigation. Under TinyGo
the woken call ran to the end inside the stop instead, and a call that
asked the server again after `CANCELLED` started a new fetch for a service
that was already gone; nothing ever aborted it.

Three changes fix the cause:

- A goroutine that returns from a wait and finds the service lock taken
  queues on a channel and is handed the lock when the holder releases it.
  It never spins, so the scheduler can go idle.
- Stopping a service aborts each of its pending fetches through the page's
  HTTP host and marks the ticket cancelled. The aborted fetch answers
  later, as any answer does, and the call then gets `CANCELLED` whatever the
  answer was. A ticket without a page request to abort (a socket send, or a
  page without the HTTP host) is still answered at once. The host-wide stop
  (`host-dispose`) now cancels requests too.
- A call whose service is already gone gets `CANCELLED` without starting a
  request or socket send.

Disposal on `pagehide` stays. Aborting fetches there is redundant, because
the browser drops them with the page, but `OnStop` hooks run only there, so
removing it would skip them. The stop is now bounded by the hooks
themselves.

Evidence: `tac_web::go_pagehide_tests` builds a prototype Go service with an
HTTP client and an `OnStop` hook that prints, plus a shared Go service with
one, with both the standard compiler and TinyGo, and drives them under Node
with the real `tac-http.js` and event bus and a fetch that waits until it is
aborted. For one waiting call, two waiting calls, a call that publishes and
asks again after `CANCELLED`, two such calls, and a call from an `@OnEvent`
listener, disposing both services as `pagehide` does returns within a
second, aborts every pending fetch, starts no new one, runs both `OnStop`
hooks before any woken call, and settles each call with `CANCELLED`. Before
the change every standard-Go case hung past 30 seconds, and under TinyGo the
"asks again" cases started fetches that were never aborted.

## Swift streams and sockets — 2026-10-17

A Swift HTTPClient protocol method may carry `@StreamRequest` or
`@SocketRequest`, as in Go. The method takes one parameter whose model type
says what each message holds, for example `func receive(_ tick: Tick)`, and
has no result. The generated client class keeps an empty method. A Swift
client with only stream or socket methods needs no Setup: it has no behavior
context.

The page's JavaScript still holds the connection, with the rules and limits
of "Go streams and sockets" above. A Swift web service that keeps the client
in a stored property (`@Needs private let ticks: TicksClient`) declares the
client's streams in its `init` answer; a Swift service that does not hold
the client opens nothing. A service holds at most 16 such methods through its
clients. A Swift service of a native app that holds such a client fails with
`TY1405`, because no page holds the connection there.

A socket client gets a generated
`func send<Message: Encodable>(_ message: Message) async throws`. It waits
for the page like an HTTP call, through a new `tac_host_socket_send` import
that uses the same tickets as `tac_host_http_start`. It throws
`TacSocketError`, whose description is `<code>: The socket message was not
sent.`, with the code `HTTP_NETWORK`, `HTTP_MISSING`, `HTTP_REQUEST` (outside
an async method), `CANCELLED` or `HTTP_BODY` (a message over 64 KiB).

Evidence: `tac_web::swift_stream_tests` builds two real Swift services to
WebAssembly and runs the same Node scenario as the Go test: only the service
that needs the clients opens them, a valid tick reaches `@OnEvent` and an
invalid one is refused, a socket message reaches its listener, 16 queued
sends are flushed in order and the seventeenth gets `HTTP_NETWORK`, and
disposing the owner closes both connections.
