# Application events, lifecycle and managed resources

The local event API replaces imperative Tac pub/sub (publish and subscribe).
It does not replace SSE (server-sent events), DOM events or native capability
permissions. A model marked `@Topic` also sends its events through the Yon
topic log to other processes and languages (see Topics below and
[ADR 0061](adr/0061-topic-events-cross-languages.md)).

The API has the same shape as Spring's (ADR 0031). The names are plain
English ([ADR 0047](adr/0047-annotation-names-are-plain-english.md)):

- A class injects an `ApplicationEventPublisher` and calls
  `publishEvent(model)`.
- The event is a model (ADR 0055). `@OnEvent` listens for the model type of
  its one parameter. The name of that type is the event name on the bus.
- `@OnEvent` carries a `condition`, an `id` and `replay`, and the method can
  have `@Order` and `@InBackground`.

`@EventPublisher` is removed. No realm has a method annotation that publishes
a successful return value anymore.

## WebAssembly services in the browser

Rust, C#, Go and Java WebAssembly services participate in the same document-local
event bus as JavaScript, TypeScript and Dart services. A listener can receive
another service's publication without an adapter or an authored bridge.

The generated service registers its listener metadata with the page runtime.
For incoming events, the page checks the payload against the listener's
model, evaluates `condition`, applies `Order` alongside browser listeners,
and delivers a cloned JSON payload to the selected WebAssembly owner, which
reads it into the model. Returned events are published once. Retained events reach
listeners with `replay: true`, including a service mounted later.

The page owns delivery for all WebAssembly publications, including delivery
back to the publishing service. `Order` therefore also applies between the
publisher's own listeners and other services' listeners. `InBackground` uses a
browser microtask and never republishes a return value. Removing a prototype
service unregisters its page listeners and disposes its WebAssembly activation.

An error returned or thrown by a listener sets the page's
`data-tachyon-signal-error="listener_failed"` marker. The publisher and
other listeners continue; the bridge does not expose the listener's error
message. An unrecoverable WebAssembly trap is not a recoverable listener error.

This is local event delivery, not network forwarding. Native host permissions,
Yon invocation boundaries and event payload limits are unchanged.

## Events are models

An event is a value of a model type, the same kind of model that a request
body uses (ADR 0041, ADR 0043). The name of the model type is the event name.
So a model named `OrderPlaced` in one language reaches a listener of a model
named `OrderPlaced` in any other language on the same bus: the browser page
connects JavaScript, TypeScript, Dart, Rust, C# and Go services, and each
language declares the model in its own `models` folder.

| Language | Listener parameter | Publishing | Where the model is |
| --- | --- | --- | --- |
| JavaScript | `/** @param {OrderPlaced} order */` above the method | `/** @type {OrderPlaced} */ const order = {...}` then `publishEvent(order)`, or `publishEvent(/** @type {OrderPlaced} */ (value))` | a JSDoc `@typedef` in the file |
| TypeScript | `placed(order: OrderPlaced)` | `const order: OrderPlaced = {...}` then `publishEvent(order)`, or `publishEvent(value satisfies OrderPlaced)` | an interface in `models/` |
| Python | `def placed(self, order: OrderPlaced)` | `publish_event(OrderPlaced(...))` | a dataclass in `server/models/` |
| PHP | `function placed(OrderPlaced $order)` | `publishEvent(new OrderPlaced(...))` | a class in `server/models/` |
| Java | `void placed(OrderPlaced order)` | `publishEvent(new OrderPlaced(...))` | a record in `server/models/`, or in `client/models/` for a Tac service (ADR 0083) |
| Kotlin | `fun placed(order: OrderPlaced)` | `publishEvent(OrderPlaced(...))` | a data class in `models/` |
| C# | `void Placed(OrderPlaced order)` | `PublishEvent(new OrderPlaced(...))` | a record in `models/` |
| Rust | `fn placed(&self, order: OrderPlaced)` | `publish_event(OrderPlaced { ... }, false)` | a struct in `models/` |
| Go | `func (s *S) Placed(order OrderPlaced)` | `PublishEvent(OrderPlaced{...}, false)` | a struct in `models/` |
| Dart | `void placed(OrderPlaced order)` | `publishEvent(OrderPlaced(...))` | a class in `models/` |
| Swift | `func placed(_ order: OrderPlaced)` | `publishEvent(OrderPlaced(...))` | a struct in `client/models/` |

- In JavaScript and TypeScript the compiler reads the event name from the
  declared type of the value you pass, and writes that name into the call.
  This works in minified browser code, because the name never comes from
  `constructor.name`. A value whose type the compiler cannot see is refused
  at build time with the forms above.
- The typed languages use the name of the value's type at run time. Dart in
  the browser uses a table that the compiler generates, because `dart2js`
  shortens type names.
- Swift models become `Codable` automatically in native builds. C# models
  are read and written by a generated `System.Text.Json` context, so the
  ahead-of-time and trimmed builds keep working.
- A string event name is refused (ADR 0055): `OnEvent` takes no `event` or
  `events`, and `publishEvent` takes no name. `ty migrate fix --write`
  converts a listener, and a JavaScript or TypeScript call, whose event name
  matches a model (the same name, or the name in PascalCase such as
  `order.placed` to `OrderPlaced`). It lists every other case as a manual
  step: declare a model named after the event, give the listener parameter
  that type and publish a value of it.
- A Ruby listener names its model in a YARD tag above the method,
  `# @param order [PlacedOrder]`, and a Ruby service publishes a
  `Data.define` or `Struct` value with `publish_event(order)`. The fix
  converts a Ruby listener that already has that tag.

## Publishing

Declare an `applicationEventPublisher` field without a value, and call it.
Application code never assigns the field and never uses it in a constructor.

```typescript
import type { CatalogCount } from '/models/CatalogCount.ts';

export default class CatalogService {
  applicationEventPublisher!: ApplicationEventPublisher;
  count = 0;

  @OnStart
  initialize(): void {
    const counted: CatalogCount = { count: this.count };
    this.applicationEventPublisher.publishEvent(counted, true);
  }
}
```

- `publishEvent(model)` publishes once.
- `publishEvent(model, retain)` also keeps the payload for later listeners
  that ask to replay it.
- A value that is not a model (a text, a number, a list or a plain map) is
  not published, and the failure is reported like any other publication
  failure.
- A publication failure never undoes the application operation that caused it.

| Realm and language | Field declaration | Bound by |
| --- | --- | --- |
| Tac JS | `applicationEventPublisher;` | Implicit resource, detected by name |
| Tac TS | `applicationEventPublisher!: ApplicationEventPublisher;` | Implicit resource, detected by name |
| Tac native (Swift/Kotlin/C#/Rust) | a member typed `ApplicationEventPublisher` | The native prelude constructs it |
| Yon JS | `applicationEventPublisher;` | `NeedsAll` by name |
| Yon TS/PHP/Java/Kotlin/C#/Rust | a field typed `ApplicationEventPublisher` | `NeedsAll` by type |
| Yon Python | `application_event_publisher: ApplicationEventPublisher` | `NeedsAll` by type |

Spelling in each language:

- Python spells the field in snake_case. It may call `publish_event` or
  `publishEvent`.
- Rust and Kotlin native members use `application_event_publisher` and
  `publish_event` where that is the language's spelling.
- In Tac JS/TS, the field is injected even without a marker. `@Needs` is
  accepted but not needed.
- In Yon, the publisher is a reserved provider token, like `DataSource` and
  `RedisTemplate`. A `@Needs` field with the same name receives it, in
  the same way that Spring's `@Autowired` receives one:

```javascript
@Service
@NeedsAll
class OrdersService {
  @Needs
  applicationEventPublisher;

  place(order) {
    this.applicationEventPublisher.publishEvent('order.placed', order);
  }
}
```

A `@Needs` field receives a resource by the field's name. Each resource has the name that
Spring gives to its closest equivalent (ADR 0038).

In every Yon language, the resources are `dataSource`, `redisTemplate` and
`applicationEventPublisher`.

The browser resources are:

| Field | What it is |
| --- | --- |
| `view` | The component's element. |
| `lifecycle` | An `AbortSignal` that aborts when the owner is disposed. |
| `resourceLoader` | The resource loader. |
| `taskScheduler` | The task scheduler. Its one task is a render. |
| `applicationEventPublisher` | The event publisher. |

`@Resource` is retired. Name the field instead. A class that only injects the
publisher still takes part in the event graph: its publications have an owner.

## Listening

```typescript
  @Order(-10)
  @OnEvent({"condition":"value.total >= 10","id":"orders.audit","replay":true})
  audit(order: OrderPaid): void { this.log.push(order.id); }
```

The method takes one parameter, and its declared type is a model. That model
is the event the listener hears. Write `@OnEvent` alone, or with options:

| Option | Meaning |
| --- | --- |
| `condition` | A bounded expression over `value` and `event`, compiled into a matcher. `event` is the model name. |
| `id` | A short literal identifier for the listener, as in Spring. |
| `replay` | Deliver the retained payload of the model on registration. |

Before a listener runs, Tachyon checks the payload against the listener's
model with the same rules as a request body (ADR 0048): every required field
is present, every field has the declared type (text, whole number, number,
true/false, list, nested model, or null for an optional field), and a closed
model has no extra fields. The compiler describes the model to every runtime,
and each runtime checks it before the condition. A payload that does not fit
is not delivered: the browser sets the listener failure marker, and the other
runtimes report a listener failure. The publisher and the other listeners
continue.

Rules and limits:

- A module listens for at most 128 distinct models.
- `id` is at most 64 bytes of `A-Za-z0-9._-`.
- `replay` is false by default, and so is retention.
- A listener takes exactly one payload parameter.

The compiler rejects these with `TY1308` before execution:

- unknown option keys, and the removed `event` and `events` keys;
- a parameter whose type is not a model;
- values that are not literals.

### Conditions

The compiler compiles `condition` once, at build time, into a JSON matcher.
Every runtime evaluates that matcher. No runtime includes an expression
evaluator.

The grammar contains:

- comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`);
- `&&`, `||` and `!`;
- literals;
- paths that start at `value`.

`event` is the event name. These are compile errors:

- method calls;
- arithmetic;
- any other root;
- a source longer than 512 bytes;
- a path deeper than 8 segments;
- an expression nested more than 16 levels.

```
"value.status == 'paid' && value.total >= 10"
"event == 'order.paid' || !value.draft"
"value.lines[0].sku != 'SKU-0'"
```

Evaluation is the same in every runtime:

- A path walks objects by key and arrays by integer index. A missing key, an
  index past the end or a step through a non-object gives absent
  (`undefined`/`None`/null). Absent equals a literal `null` and nothing else.
- `==` and `!=` are strict: the two operands must have the same JSON type and
  value. `1` does not equal `'1'`, and `0` does not equal `false`.
- `<`, `<=`, `>`, `>=` compare two numbers or two strings. Any other pair is
  false. This includes a number against a string and an absent operand.
- A bare operand is tested for truthiness. The runtime's own rule for that
  value applies.
- Any failure while evaluating the matcher counts as no match. A condition
  never raises an error into the publisher.

If a listener's condition does not match, the listener is not called at all.
This is not an error, and nothing is republished for it.

### Order and InBackground

`@Order(n)` orders the listeners of one event, lowest first.

- It takes one integer literal between -1,000,000 and 1,000,000.
- Listeners without `@Order` are at 0.
- Ties keep declaration order.
- A listener has at most one `@Order` and one `@InBackground`.
- `@InBackground` takes no arguments.

`@InBackground` runs the listener after the current delivery pass:

- The browser runs it in a microtask, outside the publisher's call.
- Yon JS/TS runs it after the current drain pass.
- Python defers it in the same way.
- Rust defers it in the same way: the listener method stays synchronous, and it
  runs after every other listener of the same event has finished.

An async listener never republishes its return value, as in Spring.

`@InBackground` is a compile error in Yon PHP, Java, Groovy, Kotlin and C#, because
their event methods are synchronous there. The error is "InBackground listeners
require a Yon language with deferred listeners (js, ts, py, rs)".

### Returned events

When a listener's declared return type is a model (or a list, optional value
or future of one), the value it returns is published again as that model. A
returned list publishes each of its first 32 values. JavaScript declares the
return type with a JSDoc `@returns {Model}` line. A returned value is never
retained.

Any other return value is ignored. Returning an ordinary value is not an
error. InBackground listeners never republish. Republication follows the same
delivery limits as any other publication. So a cycle stops at the recursion
and delivery limits and does not run forever.

## Lifecycle and resources

`@OnStart` and `@OnStop` are still instance hooks with zero
arguments.

- Each named type has at most one of each.
- Markers are compiler syntax on their own line, with strict JSON options.
- The compiler rejects static methods, generators, conflicting event markers
  on one method and annotated constructors.
- Methods, resources and listeners add no role suffix of their own. The class
  that holds them still follows its realm's shared role folder (ADR 0031).

Payloads must be finite JSON data, at most 64 KiB. Each listener receives its
own independent copy. A failed listener does not undo a successful application
operation. Do not use events as transaction-commit notifications.

### Replacing the old instance API

| Previous API | Replacement |
| --- | --- |
| `mount(root, signal)` | `@OnStart` plus `@Needs` `view` and `lifecycle` fields |
| Manual owner cleanup | `@OnStop` |
| `publish(name, value)` | An injected `applicationEventPublisher` and `publishEvent(model)` |
| An `@EventPublisher` method's return value | The same `publishEvent` call; the annotation is removed |
| `subscribe(name, callback)` | OnEvent on a one-parameter instance method |
| `retained(name)` | OnEvent with explicit replay |
| `invalidate(keys, prefixes)` | `@ClearCache` or `@UpdateCache` on the HTTPClient method that changes the data (ADR 0087) |
| `precache(urls)` / `clearAssetCache()` | A `@Needs` `resourceLoader` |
| `render()` | A `@Needs` `taskScheduler`; template handlers/listeners render automatically |
| `fetch(...)` | HTTPClient and request method annotations |

The `this.tac` object and the TacRuntime type are removed. They are not kept
as deprecated aliases.

The TypeScript interfaces are TacResourceLoader, TacTaskScheduler and
ApplicationEventPublisher. These are Tachyon contracts,
not Spring's Java interfaces.

NeedsAll leaves browser resource fields out of client
constructor inference. In Yon, NeedsAll is exactly what binds
the publisher. No constructor should access an injected resource.

## Ownership and initialization

### Tac

Tac registers listeners and injects resources before OnStart. The hook
runs for framework-owned services and their managed provider graph. It does
not run for instances that application code constructs by hand.

- A singleton service is constructed when the first component that names it
  renders. It is initialised once, when that component activates.
- A prototype-scoped service is initialised with its component instance.
- Persisted state is already restored at that point.
- Component mount scheduling (load/idle/visible/interaction) still applies to
  the component that names the service.

The component root stays the same for its owner's whole lifetime. Rendering
may replace descendant elements, so use authored HTML event directives for
them.

Removal, hot replacement and actual document disposal each unregister
listeners and run cleanup once. A bfcache (back/forward cache) suspension
keeps its owner. Async work must observe the injected signal, because
cancellation cannot interrupt arbitrary JavaScript. Do not depend on
page-unload hooks to finish network writes. Browser errors show generic
markers, without event payloads or raw exception text.

### Yon

Yon delivery belongs to the provider graph of one invocation. It is not any of
these: a server-wide bus, a cross-request singleton, a channel from a worker to
a request, or a background job API. The one exception is a `@Topic` model
(see Topics below).

- Its owned queue drains during initialization and after the endpoint
  finishes, before serialization.
- Listeners cannot change a primitive return value that is already computed.
  Application code may still change shared objects.
- When endpoint correctness depends on an operation's result, prefer explicit
  service return values.
- Cleanup is still subject to the existing supervisor deadline and forced
  termination.
- Yon JS/TS/Python support asynchronous hooks and event methods. PHP, Java,
  Kotlin, C# and Rust currently require synchronous methods. PHP, Java, Kotlin
  and C# also refuse `@InBackground`; Rust accepts it and runs the synchronous
  listener after the other listeners of the event.
- Streaming event containers and transient event providers are rejected. They
  are never partly initialized.

### Topics

A model marked `@Topic` works like a Spring Cloud Bus remote event
([ADR 0061](adr/0061-topic-events-cross-languages.md)):

- Write `@Topic` above the model in a models folder: `@topic` in Python,
  `#[Topic]` in Rust, PHP and Ruby, `[Topic]` in C#, `// @Topic` in Go and
  `/** @Topic */` in TypeScript. JavaScript writes ` * @Topic` in the model's
  `@typedef` block. The compiler refuses it anywhere else.
- The topic name is the kebab-case model name: `OrderPlaced` is
  `data/topics/order-placed.jsonl` and `/.tachyon/topics/order-placed`. The
  log folder is `topics.directory` in `server/application.json`
  (`data/topics` unless set); the URL is a server route, not a folder. The
  logs are saved data, so they stay out of the generated `.tachyon` folder.
- `publishEvent(model)` of a Topic model delivers to the request's own
  listeners first. The runtime then sends the value over the invocation's
  resource channel. The server accepts only a Topic model of the publishing
  file's language and a JSON object of at most 64 KiB, and appends one line
  (the log holds at most 16 MiB).
- The server calls each `@OnEvent` listener of the model in the other Yon
  files and languages once, with the operation `event.deliver`. A listener
  class that several controllers use runs in one of them. The listeners that
  ran in the publishing request are skipped. Delivery is at most once, with
  no stored position, and a republished Topic event travels at most 8 times.
- Each language marks its own model. A listener hears other processes only
  when its own model is a Topic, and only when its class is part of a
  controller, like every Yon provider.
- All eleven Yon languages send and receive Topic events.
- In Tac, an `@OnEvent` listener for a Topic model opens the topic stream by
  itself; all listeners of one topic share one stream, which replays the kept
  records, resumes after the last record it received and reconnects like
  `@StreamRequest`. A page's own `publishEvent` stays in the page.

### Native Tac

- Native Tac service activations use activation IDs. They reject stale calls
  and stale publications.
- They queue local events until the current callback returns.
- Event payloads carry the service's instance name.
- Native event annotations on helper owners are rejected.
- Native event and lifecycle methods stay synchronous. `async` declarations
  fail before the toolchain runs.
- Native client HTTP adapters are still configured by hand.
- Browser resources reach only Tac JS/TS services.

See [ADR 0031](adr/0031-realms-mirror-each-other.md) for the current model and
[ADR 0028](adr/0028-owned-application-events-and-lifecycle.md) for the original
reasons. See the handoff for executable qualification evidence and the limits
that are still open.
