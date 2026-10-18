# Shared Tac and Yon Behavior Annotations

The shared behavior annotations work (they have executable integrations) in
every supported Yon language and every supported Tac service language. Plain
classes without annotations stay valid.

The annotations follow the existing boundary between languages and targets.
They do not make Swift a Yon handler language. They do not make Java, Python or
PHP Tac service languages.

Apart from that, the two realms mirror each other (ADR 0031). These annotations
exist in both Yon and Tac:

- `Service`, `Lifetime`, `HTTPClient`, `Helper`, `Converter`;
- `Store`;
- `Needs`, `Named`;
- `OnStart`, `OnStop`, `OnEvent`, `Order` and `InBackground`.

Only `Controller`, `StreamRoute`, `SocketRoute` (with `OnOpen`, `OnMessage`, `OnClose` and `OnError`) and `Gateway` are Yon-only. Only Yon has an HTTP
surface, and only the Yon supervisor can start another language for a call.
[Application events](APPLICATION_EVENTS.md) documents the lifecycle and event
markers.

| Language | Yon REST endpoints | Tac browser/client bundle | Tac target-native service |
| --- | --- | --- | --- |
| JavaScript / TypeScript | Yes | Yes | Same JS/TS bundle in web-view hosts |
| Python | Yes | No | No |
| PHP | Yes | No | No |
| Java | Yes | Yes, compiled to WebAssembly ([ADR 0083](adr/0083-java-services-reach-the-web-as-webassembly.md)) | No (runs as WebAssembly in the web view) |
| Kotlin | Yes | No | Android |
| C# | Yes | Yes, compiled to WebAssembly ([ADR 0037](adr/0037-csharp-services-reach-the-web-as-webassembly.md)) | Windows |
| Rust | Yes | Yes, compiled to WebAssembly ([ADR 0035](adr/0035-rust-services-reach-the-web-as-webassembly.md)) | macOS / Windows / Linux |
| Swift | No | No | macOS / iOS |
| Dart | Yes | Yes, compiled to JavaScript ([ADR 0033](adr/0033-dart-browser-services.md)) | No |

The annotations describe shared policies. Each language keeps its own types,
error handling and execution model. Tac still has no SSR (server-side
rendering). Yon still calls methods named after uppercase HTTP verbs and
returns REST responses.

## Global Annotations and Types

Tachyon annotations, lifecycle names, `Behaviors` and the framework types are
global in every language (ADR 0046). Write them without an import. There is no
package to install:

```typescript
@Setup
class AppSetup {
  static configure(execution: BehaviorExecution): BehaviorConfiguration {
    return {};
  }
}
```

Tachyon finds the markers by text, validates them, removes them, and emits a
runtime inside the same source file.

Rules and results:

- An import from `tachyon:behaviors` is refused with `TY1308`. Delete the
  line; the names are already global.
- Tachyon installs no global runtime decorators and no reflection package. The
  markers exist only at compile time.
- The framework types are ambient declarations only. New projects get
  `tachyon-behaviors.d.ts` for semantic type checking.
- Composed classes need no import either. `ty` writes
  `.tachyon/types/<realm>/composed.d.ts`, which declares every exported,
  annotated JavaScript or TypeScript class that the realm composes as a
  global with its real type; each realm's `jsconfig.json` includes its own
  file (ADR 0053). The server realm also declares the route and socket
  controllers, because a Yon test unit reaches them. Each `@Test` class gets
  `BehaviorTestUnit` (`behaviors` and `construct(Type)`, which `ty test`
  adds). Run `ty types` to write it without building.
- Each realm's `jsconfig.json` turns on `experimentalDecorators`, because
  route parameters use parameter annotations such as `@FromBody`, and maps
  `/models/Hello.ts` to the realm folder.
- A request method such as `@GetRequest` keeps an empty body, because
  Tachyon writes it. TypeScript accepts annotations only on a method with a
  body, so a TypeScript `HTTPClient` is a class whose methods have empty
  bodies. An empty body returns nothing to TypeScript, so declare
  the answer as `Promise<Hello | void>` and check the value before use.

A type that has a role needs three things: a matching name suffix, the exact
file name, and a plural parent folder. Each realm shares these folders:
`client/<role>` and `server/<role>`. The only exception is controllers, which
stay beside their route (ADR 0031).

The shared role folders supply captured declarations in the same language for
one compilation unit. Each compilation unit belongs to one entry. Tachyon
rejects legacy module manifests.

An included JavaScript or TypeScript file may import by absolute specifier, for
example `/shared/scripts/navigation.js`. That specifier resolves to the same
file wherever the unit is assembled. Relative imports and bare package imports
must stay in the entry file. The reason: they would resolve against a file
path that no longer exists in the assembled unit.

Tachyon does not scan the project for annotations. Contexts that were
generated separately cannot replace each other.

[Project structure](BEHAVIOR_PROJECT_STRUCTURE.md) describes the required
layout and has complete examples in several languages. The declaration
excerpts below show policy syntax only. Role declarations belong in their
matching files, not inline in entry files.

## Ownership and Lifecycle

In JavaScript/TypeScript, `@Setup` marks a named class that has
`static configure(execution)`. This method returns:

- adapter collections that the application selects, named `caches`, `http` and
  `entities`;
- a cache function `authorize(call, execution)`;
- a query function `authorizeEntity(command, execution)`.

The two functions are required when the matching cache or query operations are
used. They must return literal `true`. You may leave out collections that you
do not use.

The configuration factory runs only when application code calls it
explicitly:

```javascript
async function createCatalogContext(signal) {
  const context = await Behaviors.create(ApplicationSetup, {
    partition: 'public-catalog',
    signal,
    timeoutMs: 10000,
  });
  return context;
}
```

Every service instance that uses adapters receives this context in its
`behaviors` field. Construct or assign it explicitly. There is no global
application container.

Rules for each place:

- **JS/TS Tac.** Create the context inside OnStart. Use the owner's
  `lifecycle` signal, which a `@Needs lifecycle` field receives.
- **No context needed.** Methods that use only `Check`, and the functions
  `Behaviors.validate`/`Behaviors.properties`, need no context. They do not
  create instances of their schema classes.
- **Component removal and hot replacement** abort the signal and all pending
  operations.
- **Pages.** Page resources include `document.body` and a signal. The signal is
  aborted when a page is left and does not enter the back/forward cache. When a
  document is kept in that cache, it keeps its owner and signal while it is
  suspended and when it is restored.
- **Hooks.** OnStart and OnStop are instance hooks with no
  arguments. Native event and lifecycle hooks are synchronous.
- **Manual lifetimes.** Call `context.dispose()` when you are finished.
- **Yon.** Construct and dispose the context inside the invocation. The
  supervisor's deadline and process cleanup are still the final limit on
  execution. Processes and contexts are not reused between requests.

**Partition.** The partition is required. It must not be empty and must be at
most 256 UTF-8 bytes. Applications derive it from their authenticated user or
tenant, not from request input that nobody validated. Data that is explicitly
public may use a fixed public partition.

**Concurrency and deadlines.** A context allows at most 64 operations at the
same time. All steps of one operation share one absolute deadline. The default
is 10 seconds. You can set it from 1 ms to 60 seconds.

**Cancellation.** For application adapters, cancellation is cooperative: the
adapter must check for it. Yon also keeps its supervised process termination
boundary. Tac does not get process isolation.

**Native services:**

- Native services create a `BehaviorSignal`/`Behaviors.Signal` explicitly, and
  dispose their context together with their owner. They do not receive the
  browser's `AbortSignal` automatically.
- Native lifecycle contexts have explicit activation and disposal by their
  owner.
- The synchronous Java/Kotlin/PHP/Rust integrations check deadlines before and
  after calls to application code and adapters. They do not interrupt blocking
  database or network operations.
- C# and Swift also check around awaited operations.
- Select adapters that respect the supplied signal and deadline themselves.
- Tachyon adds no hidden retry, background thread or detached timeout worker.

**Native service class.** Name the native service class after its file, for
example `BasketService` in `client/services/BasketService.swift`. Keep its
contract of construction without arguments. Add explicitly selected native
role modules after that service type. Create their behavior objects from the
lifetime that the service owns. Annotations do not change which type the
native bridge selects as the page, and they do not add Tac DI (dependency
injection).

## Annotation Reference

Rules for every declaration:

- Each marker is on its own complete line.
- Classes and structs must have explicit names.
- Annotated inheritance, computed members, accessors and generator methods are
  not supported.
- Object arguments are strict JSON literals with quoted keys. They are not
  evaluated JavaScript expressions.
- Invalid declarations fail before execution with `TY1308`.
- Every annotation checks what it is written on (ADR 0056). One rule table,
  `crates/tachyon-core/src/annotation_rules.rs`, says for each annotation
  whether it goes on a class, a method, a field or a parameter, whether it
  takes arguments, which Tac and Yon languages it reaches, which annotation it
  needs beside it or on its class, and which ones it cannot share a member
  with. The same checks and messages apply in every language.

The table below uses JavaScript spelling. Other languages spell the same
annotation like this:

| Language | Spelling |
| --- | --- |
| Python | also accepts snake_case (`@needs_all`, `@web_call`, `@gateway`) on classes and methods; a field or parameter marker goes inside the type: `count: Annotated[int, min(1), max(100)] = 1`, `query: Annotated[OrderQuery, check]` |
| C# | `[CacheSettings(...)]` |
| PHP | `#[CacheSettings(...)]` |
| Rust | `#[CacheSettings(...)]`, or the matching namespaces `#[yon::cache_settings(...)]`/`#[tac::cache_settings(...)]` |
| Go | `// @CacheSettings(...)`, or `// @yon::cache_settings(...)`/`// @tac::cache_settings(...)` |
| Swift, Java, Groovy, Kotlin | `@CacheSettings(...)` |

In every language, the options stay strict JSON that the compiler owns. They
are not evaluated as native expressions. Run these files through Tachyon before
native compilation.

Rules for native annotations:

- They target named top-level classes (structs in Rust and Go) and direct,
  concrete instance methods.
- Use explicit fields, positional parameters without default values or rest
  arguments, and method bodies in blocks.
- Kotlin primary-constructor and data-class shapes are not part of this
  annotation grammar.
- PHP parameter names that begin with `__behavior_` and Rust parameter names
  that begin with `__tachyon_` are reserved for generated bindings.

These restrictions fail before any adapter runs.

| Annotation | Executable behavior |
| --- | --- |
| `@Setup` | Makes `static configure(execution)` available to an explicitly selected `Behaviors.create`. No calls at build time. No hidden global registration. |
| `@Needs` | Field injection, like Spring's `@Autowired`. The constructor that Tachyon generates takes the fields that carry this marker, in declaration order. The field must be a direct instance field, without an initial value, and not static. Tachyon resolves each field to a provider by its own type or, in JavaScript, by its lower-camel name. |
| `@Named('ProductStore')` | Names the provider for the field next to it, when the field's own name or type does not select one. |
| `@CacheSettings({"name":"products","ttlMs":30000,"maxBytes":65536,"maxEntries":256})` | Declares the policy for the Cached methods of that class. Selects `configuration.caches.products`. |
| `@Cached` | Wraps a service method with these steps: authorization, bounded key construction, lookup, running the method on a miss, and storing a successful JSON result. The key is the parameters marked `@CacheKey`, or every parameter when none is marked. Whether it is sync or async follows the language table below. |
| `@CacheKey` (on a parameter) | Makes that parameter part of the cache key of its `@Cached` method. |
| `@UpdateCache({"method":"find"})` | Always runs the mutation. Then it stores the successful result under the key of the Cached target method in the same class. Its parameters are matched to the key parameters of `find` by name; a missing name fails the build. |
| `@ClearCache({"method":"find"})` | Deletes the key of the Cached target method in the same class, but only after a successful mutation. Parameters are matched by name, as for UpdateCache. |
| `@PageDefaults({"defaultSize":20,"maxSize":100,"sort":["id","name"]})` (on a parameter) | Before the method runs and before cache lookup, turns that parameter into `{page,size,offset,sort}`. It never fetches a collection and cuts it into pages. On a Store query, `@PageDefaults` without sizes marks the page parameter; the Store gives the sizes. |
| `@Entity({"adapter":"data"})` with `@Id` on one field | Declares the model identity that `context.repository(Model)` uses through `configuration.entities.data`. The field marked `@Id` is the identifier; an Entity without exactly one `@Id` field fails the build. |
| `@Table('products')` | Gives the required table mapping for an entity. The mapping cannot change. Tachyon runs no DDL, creates no schema and runs no migration. |
| `@Store(ProductEntity, {"fields":["id","status"],"defaultSize":20,"maxSize":100})` | Declares a Store: a contract with no implementation, like a Spring Data repository interface, in the bodiless form of its language (see [Declared exchanges and queries](#declared-exchanges-and-queries)). The first argument is the Entity class: `ProductEntity.class` in Java, `ProductEntity::class` in Kotlin and PHP, `typeof(ProductEntity)` in C#, `typeof<ProductEntity>` in F#, the bare name elsewhere. Each method is a derived query (`findBy`, `countBy`, `existsBy`, `deleteBy`) or a `@Query`, and Tachyon writes it. A Store that extends `Repository<ProductEntity, Id>` also gets `findById`, `findAll`, `save` and `deleteById` (see [The Repository base](#the-repository-base)). `{"setup":"CatalogSetup"}` in the options names the Setup that builds its behavior context; the default is the store name with `Setup` instead of `Store`. `@Needs` injects the store in Yon and in Tac. A Store without an Entity, a class form where the language has a contract form, a method body, a field, a constructor or a method that is not a query fails with `TY1308`, and so does an `@AllOrNothing` method on a Store: the transaction belongs to the service that calls the store. |
| `@Query({"operation":"find","where":[{"field":"status","operator":"eq","argument":0}],"orderBy":[]})` | Turns an empty method into an authorized, structured query to the entity adapter. Never SQL text. A `find` reads its page request from the parameter marked `@PageDefaults`. |
| `@HTTPClient({"adapter":"catalog","path":"/products"})` with `@GetRequest` | Declares an outgoing call through the selected HTTP adapter. It does not register a Yon route. `GetRequest`, `PostRequest`, `PutRequest`, `PatchRequest` and `DeleteRequest` select the verb and optional method path. `PathVariable`, `RequestParam`, `RequestHeader` and `RequestBody` select argument values. An `HTTPClient` is a contract with no implementation, like a Spring `@HttpExchange` interface: declare it in the bodiless form of its language (see [Declared exchanges and queries](#declared-exchanges-and-queries)). A class form where the language has a contract form, a method body, a field, a constructor, a method without a request annotation, or another role on the same class fails with `TY1308`. `ty migrate fix --write` rewrites a class client. `{"setup":"CatalogSetup"}` names the Setup whose adapters send the requests; the default is the client name with `Setup` instead of `Client`. GET requests cannot have a body. See ADR 0048. |
| `@StreamRequest("/ticks")` on an `@HTTPClient` method in JavaScript, TypeScript or Go | Tac keeps a stream of server-sent events open at the path: it opens it when a component or service first uses the client, reconnects with a growing wait (1, 2, 4, 8 and 16 seconds, then it stops), reconnects after 60 seconds without a message, and closes it with its owner or when the server sends `end`. The method takes one parameter whose model type describes each message. A message that does not fit the model is dropped. Tac passes each message to the method and then publishes it as that model's event, so an `@OnEvent` listener in any Tac language receives it. It pairs with `@StreamRoute` on the server. In Go the method is part of the client interface (`Receive(tick Tick)`), the page holds the stream for the Go service that keeps the client in a `// @Needs` field, and the service receives each message with `// @OnEvent`. See ADR 0058 and ADR 0090. |
| `@Topic` on a model in a models folder | Makes the model a topic (ADR 0061). `publishEvent(model)` of a Topic model also appends the value to `data/topics/<kebab-name>.jsonl` (the folder is `topics.directory` in `server/application.json`), and the server calls each `@OnEvent` listener of that model in the other Yon files and languages once; the publishing request's own listeners are not called again. A Tac `@OnEvent` listener for a Topic model opens `/.tachyon/topics/<kebab-name>` by itself. Spelled `@topic` in Python, `#[Topic]` in Rust, PHP and Ruby, `[Topic]` in C#, `// @Topic` in Go, `/** @Topic */` in TypeScript and ` * @Topic` in a JavaScript `@typedef` block. Refused outside a models folder and when no model follows it. |
| `@SocketRequest("/chat")` on an `@HTTPClient` method in JavaScript, TypeScript or Go | Tac keeps a WebSocket open at the path (`ws:` or `wss:` on the page's origin): it opens it when a component or service first uses the client, reconnects with a growing wait (1, 2, 4, 8 and 16 seconds, then it stops) unless the server closed it normally or refused a message, and closes it with its owner. The method takes one parameter whose model type describes each incoming message; Tac checks it, passes it to the method and publishes it as that model's event for `@OnEvent`. The client also gets `send(message)`, which sends JSON text (TypeScript declares it with `declare send: SocketSend<ChatMessage>`). A class has at most one `@SocketRequest`. It pairs with `@SocketRoute` on the server. A Go socket client gets `Send(message any) error`, which waits for the page like an HTTP call and returns an error code (`HTTP_NETWORK`, `HTTP_MISSING`, `HTTP_REQUEST` outside a method call, `CANCELLED`) instead of a rejected promise; the Go service receives the messages with `// @OnEvent`. See ADR 0060 and ADR 0090. |
| `@Gateway({"class":"server/services/ReportService.java","method":"summarize"})` or `@Gateway({"command":["ruby","server/helpers/report.rb"]})` | Yon only. Declares a method that another language answers: a class in another Yon language, or a program. Like a declared HTTP request, it is **declared as a signature**: a method of the client contract. The positional arguments (at most 32) travel as bounded JSON, and the other side's return value is the result. A failure is a BehaviorError with the code `invalid`, `timeout`, `capacity` or `adapter` and the fixed message `Gateway call failed.`. It cannot be combined with another method annotation. See [Gateways](#gateways). |
| `@Settings({"prefix":"YON_APP_"})` | Defines explicit binding of a string map into typed, bounded JSON. The class's instance fields are the settings: each field's declared type and initial value give its type and default, and `@Min`/`@Max` on a field give its range. The environment name is the field name in capitals with underscores (`pageSize` is `YON_APP_PAGE_SIZE`); `"names":{"pageSize":"SIZE"}` renames one. Rust and Go fields have no initial values, so their defaults go in `"defaults":{...}`. Use `TAC_` prefixes in Tac. |
| `@WhenProfile('development', 'test')` | Selects a Setup when any listed profile is active (an explicit OR allowlist of active profiles). |
| `@WhenSetting({"name":"YON_CACHE_ENABLED","havingValue":"true","matchIfMissing":false})` | Requires an exact, explicitly supplied string value before the Setup runs. |
| `@Setting("${yon.server.port:8080}")` | Declares one instance field that `Behaviors.values` binds from the resolved application configuration. The object form `{"path","type","default","min","max"}` adds a type and bounds. Tac may name only `tac.*`. Yon may name only `yon.*`. |
| `@Checked` | Declares a DTO (data transfer object) validation schema over direct fields. The DTO is a behavior-only class in its own file in a role folder, usually `helpers/`. Discovery composes it into the same unit as the method that names it ([ADR 0044](adr/0044-an-annotated-file-holds-one-class.md)). |
| `@Check` (on a parameter) | Validates that parameter against a `@Checked` class before pagination, caching, HTTP or method code. In JavaScript, TypeScript and Python the parameter's type names the class (JavaScript uses a JSDoc `@param` type). The other languages keep the JSON parameter type and name the class: `@Check(ProductInput.class) Object input` in Java. |
| `@Required` | Rejects a missing or null field. |
| `@NotEmpty` | Requires a string that is not empty and contains at least one Unicode scalar that is not whitespace. |
| `@Length({"min":1,"max":100})` | Limits the number of UTF-8 bytes in a string, or the number of items in an array or object. Missing or null values are ignored. |
| `@Min(0)` | Sets a finite numeric lower bound. Strings are not converted to numbers. Missing or null values are ignored. |
| `@Max(100)` | Sets a finite numeric upper bound. Strings are not converted to numbers. Missing or null values are ignored. |
| `@Retry({"maxRetries":3,"delayMs":10,"multiplier":2,"maxDelayMs":200,"jitterMs":5})` | Runs one async method again after a bounded delay, which can grow exponentially. `maxRetries` is 1 through 8. `delayMs`, `maxDelayMs` and `jitterMs` are millisecond literals. `multiplier` is 1 through 10. Framework refusals are never retried: invalid policy, denied authorization, disposed context, expired deadline. An adapter fault or an error that the method raised is retried. |
| `@MaxAtOnce({"limit":4,"policy":"block"})` | Limits how many calls to one method run at the same time. `limit` is 1 through 64. `block` waits for a free slot, but only until the operation deadline. `reject` refuses at once. A call that retries keeps its slot across every attempt. PHP runs one synchronous call at a time and accepts only `reject`. |
| `@AllOrNothing({"adapter":"data"})` | Runs one async service method inside a transaction on the named `configuration.entities` adapter. It calls `begin` before the body, `commit` after the body returns, and `rollback` when the body raises an error. Every repository and `@Query` command issued while the transaction is open carries the transaction when its entity uses the same adapter, also from a Store that was injected with its own behavior context: the open transaction follows the call, like Spring's propagation REQUIRED. A nested AllOrNothing joins the open transaction; it does not nest. There are no savepoints. A method that retries opens a new transaction for each attempt. The adapter decides: an adapter that ignores the transaction key is not transactional. The managed Yon datasource answers `{"operation":"transaction","action":"begin"\|"commit"\|"rollback"}` with a bounded count and lifetime. FYLO refuses, because documents have no transaction to open. |
| `@Converter` | The role that holds `@Convert` methods, like MapStruct's `@Mapper`. It takes no arguments and follows the role layout: a `<Name>Converter` type in `converters/`. A Converter is a contract with no implementation, in the bodiless form of its language (an abstract interface class in Dart). A method body, a field, a constructor, a class form where the language has a contract form fails with `TY1308`: "A Converter has no implementation: Tachyon generates each mapping." A Converter needs no behavior context and has no constructor argument: a controller or service asks for it with `@Needs` in Yon and in Tac (an F# constructor parameter, `#[Needs]` above a Ruby `attr_reader`). `ty migrate fix --write` rewrites a class converter, and `ty migrate` reports code that still builds one with a context. |
| `@Convert({"fields":[{"source":"detail.id","target":"productId"},{"target":"kind","constant":"product"}]})` | Declares the whole method as a field plan over its one bounded JSON parameter. Each entry names its `target`. It also takes either a dotted `source` path or a literal `constant`. A `source` path has at most 8 names, and an absent path maps to `null`, as in MapStruct. The method takes exactly one parameter. JSON-value signatures require the field plan; model signatures infer same-named fields and may omit `@Convert`. At most 32 entries. A target written twice is refused. Like a query, it is **declared as a signature with an empty body**. Nothing else is copied: an input field without a mapping is dropped. |
| `@Description('Charges the card and records the order')` | One bounded literal line: at most 256 characters, with no quotes, escapes or control characters. It is carried into the registration for tools. It changes no behavior. It annotates a class that already declares one. |
| `RestClient` (not an annotation) | `behaviors.restClient("catalog").get("/products").query({...}).header(name, value).body(value).retrieve()` calls a configured `http` adapter directly in code. It has the same rules as the declarative form: a bounded path relative to the origin, headers owned by the transport are refused, no body on GET or HEAD, `redirect: error`, `credentials: omit`, a 1 MiB response limit and the operation's own deadline. Python spells it `rest_client`. |
| `@Schedule({"fixedDelaySeconds":300,"initialDelaySeconds":30})` | On a worker under `server/workers`, declares the schedule that the supervisor already follows. `fixedDelaySeconds` is 1 through 86,400. `initialDelaySeconds` is 0 through 86,400 and defaults to one full interval. A worker declares its schedule here or in `.tachyonrc.workers`, never in both (`TY1502`). There is no cron field and no Tac equivalent, because a browser page has no supervisor to run a detached timer. |

Names are plain English words
([ADR 0047](adr/0047-annotation-names-are-plain-english.md)). They no longer
copy Spring, Lombok, MapStruct or JUnit. The old spellings are refused with
`TY1308`, and the error names the replacement.
`ty migrate fix --write` renames them for you:

| Old spelling | Current name |
| --- | --- |
| `Autowired` on a field | `Needs` |
| `Inject` on a class | `Needs` |
| `RequiredArgsConstructor` | `NeedsAll` |
| `Qualifier` | `Named` |
| `Scope` | `Lifetime` |
| `Repository` | `Store` |
| `Mapper` | `Converter` |
| `Mapping` | `Convert` |
| `Configuration` | `Setup` |
| `ConfigurationProperties` | `Settings` |
| `Value` | `Setting` |
| `Profile` | `WhenProfile` |
| `ConditionalOnProperty` | `WhenSetting` |
| `PostConstruct` | `OnStart` |
| `PreDestroy` | `OnStop` |
| `EventListener` | `OnEvent` |
| `Async` | `InBackground` |
| `Scheduled` | `Schedule` |
| `Cacheable` | `Cached` |
| `CachePut` | `UpdateCache` |
| `CacheEvict` | `ClearCache` |
| `CacheConfig`, `Cache` | `CacheSettings` |
| `Retryable` | `Retry` |
| `ConcurrencyLimit` | `MaxAtOnce` |
| `Transactional` | `AllOrNothing` |
| `PageableDefault`, `Pageable` | `PageDefaults` |
| `Client`, `WebCall`, `HttpExchange`, `HTTPExchange` | `HTTPClient` with request method annotations |
| `GetExchange`, `PostExchange`, `PutExchange`, `PatchExchange`, `DeleteExchange` | `GetRequest`, `PostRequest`, `PutRequest`, `PatchRequest`, `DeleteRequest` |
| `Controller`, `RequestMapping` | `RequestRoute` |
| `GetMapping`, `PostMapping`, `PutMapping`, `PatchMapping`, `DeleteMapping` | `GetRoute`, `PostRoute`, `PutRoute`, `PatchRoute`, `DeleteRoute` |
| `Validated` | `Checked` |
| `Valid` | `Check` |
| `NotNull` | `Required` |
| `NotBlank` | `NotEmpty` |
| `Size` | `Length` |
| `TachyonTest`, `SpringBootTest` | `Test` |
| `TestConfiguration` | `TestSetup` |
| `MockBean`, `MockitoBean` | `Fake` |
| `ActiveProfiles` | `UseProfiles` |
| `TestPropertySource` | `TestSettings` |
| `DirtiesContext` | `FreshStart` |
| `Delegate` role, `Component` | `Helper` |

The folder of the old `Delegate` role, `delegates/`, is now `helpers/`, and a
helper class name ends in `Helper`. So a Tac project's `client/components/`
still holds only views, and no role shares its name with a view component.

## Test units

A test unit is an entry, like a route. `server/tests/<Name>Test.<ext>`
assembles the role files of its realm, so every service, repository, client,
component and converter is in scope. `@Test` names the Setup that
the unit's behavior context comes from. The compiler writes the members that
the unit never writes: the lifecycle, the construction that knows about mocks,
and the driver that `ty test` runs.

For example, `server/tests/CatalogServiceTest.js`:

```javascript
@Test(CatalogSetup, {"partition":"tests"})
@UseProfiles("test")
@TestSettings({"yon.catalog.size":5})
export default class CatalogServiceTest {
  @Fake
  catalogCounterHelper;

  async testDoublesTheCount() {
    this.catalogCounterHelper = { count: () => 4 };
    const service = this.construct(CatalogService);
    if (service.total() !== 8) throw Error('expected 8');
  }
}
```

```bash
ty test
```

**Tac units.** A Tac unit is `client/tests/<Name>Test.<ext>`. It is assembled
from the client roles and runs in its own realm. A browser unit cannot use the
Yon route plan, because its roles are Tac roles and its context is a Tac
context.

- **JavaScript and TypeScript** units run under Node, Bun or Deno.
  `TAC_JAVASCRIPT_RUNTIME` picks one. Without it, the first of the three that is
  installed runs the unit. If a runtime is missing, the next one is tried. If a
  runtime ran and failed, its result is the answer.
- **Swift, Kotlin, C# and Rust** each compile to one artifact that prints its
  report: a binary, a jar, and a project built the same way as a C# handler.
- **Go** units build and run with `go run`, outside the browser. A client unit
  uses the same service runtime as the WebAssembly module.
- The Tac driver uses no Yon transport type. Every behavior runtime already
  has a bounded JSON serializer (`BehaviorJSON.encoded()`,
  `BehaviorJSON.ToString()`, `BehaviorValue::to_json`, `Behaviors.json`). So
  the driver answers with its own encoded report, and the runner reads it.
- `ty test` runs both realms. It exits with a non-zero code when a case fails.

**What a browser unit sees.** A browser unit sees what a browser service sees:

- An absolute specifier such as `/shared/scripts/navigation.js` resolves
  against the project's `client/` tree, the same way the browser resolves it
  against the site. This works under all three runtimes.
- The unit gets the project's DOM when the project installs one, for example
  `@happy-dom/global-registrator`, which Bun recommends. Without one, the unit
  runs without a DOM. Tachyon ships no DOM, so the project makes the choice. A
  case that needs only one or two globals can define them itself.
- `history.pushState` sets `location` for a case that reads the current page.

**Imports in role files.** A role file may import by absolute specifier. What
assembly forbids is moving a relative path. A relative specifier resolves
against the file where it was written, but the assembled unit is in another
place. `/shared/...` means the same thing wherever the unit is.

- When two role files import the same helper, the assembled unit contains one
  shared import statement.
- One local name bound from two different specifiers is refused, because no
  merge can make both correct.
- Bare package names stay refused until assembly has an import map to resolve
  them.

**`@TestSetup`.** A unit's own Setup is `@TestSetup`,
declared in its own same-language sibling file named after the class, such as
`client/tests/ShellTestSetup.js`. `@Test(ShellTestSetup)` selects
that class explicitly. The compiler validates both authored files before
composing them. Missing files, mismatched names and duplicate definitions fail
before execution. Test setup files are not production role files and
never enter browser bundles. A unit may still select an existing production
`Setup` from its realm's `setups/` folder.

**`@Fake`.** `@Fake` declares a field that the driver clears before
every case. In JavaScript and TypeScript, `this.construct(Role)` then builds the
role under test with those test doubles in place of its `@Needs`
collaborators. The other six Yon languages get the declaration and the
clearing, but not the construction. Rust cannot inject a collaborator again
through reflection at all. So a unit in those languages tests the logic
directly instead of going through the container.

**Every public method needs a test.** Every public method of an annotated class
must have a test, and the development server enforces this:

- A marker on the class says that the type has behavior the framework owns. So
  its public methods are the application's contract.
- `ty preview` refuses to start while a `test*` case has not called any of
  those methods (`TY2023`). It prints the missing ones.
- `ty start` does not check this, because production is not the place to learn
  it.
- `TAC_ALLOW_UNTESTED=1` allows a run that really cannot wait.
- `ty test` prints the same report. With `--coverage`, it fails on it.

"Covered" means that a test case really called the method while `ty test` ran
(ADR 0070). A test build records each call; a production build does not. A
call on a Fake does not count, because a Fake is not the real class. A
JavaScript or TypeScript utility in `client/shared/utils` that a file imports
by its path runs as a recorded copy in each unit, so its calls count too.
`ty test` writes the calls to `.tachyon/coverage/calls.json`, and `ty preview` reads them
or runs the tests again when a source is newer. Only when the units of a
language cannot run on the machine does `ty` fall back to checking names, and
the report labels those classes `[name check]`.

Methods that the compiler writes are exempt, because they contain no authored
logic to test. A derived repository query, a declared HTTP request and a
`@Convert` all arrive as a bare signature.

`ty init` creates the unit that covers the service it creates. So a new project
starts with the convention and does not need to add it later.

**A case is a convention**, like a controller method named after a verb. A case
is an instance method whose name begins with `test`. Tachyon's `@Test` marks
the test unit class, not a case. JUnit also has a `@Test`, but JUnit puts it
on each case method. JUnit's other markers, such as `@BeforeEach` and
`@Disabled`, are not Tachyon markers. The compiler ignores them, because they
belong to your own runner. So the compiler lists the cases itself. This also
makes Rust work, since Rust has no reflection to find cases at run time.

| Marker | What it does |
| --- | --- |
| `@Test(CatalogTestSetup, {"partition":"tests"})` | Selects a sibling TestSetup class or an existing production Setup class. Java writes `CatalogTestSetup.class`, Kotlin and PHP `CatalogTestSetup::class`, C# `typeof(CatalogTestSetup)`. `partition` defaults to `test`. |
| `@TestSetup` | A test-only Setup in its own same-language sibling file named after the class. |
| `@UseProfiles("test")` | Profiles for that context, with the same limits as `@WhenProfile`. |
| `@TestSettings({"yon.catalog.size":5})` | At most 64 bounded literal properties. Each must be inside the realm's own namespace: a Tac unit cannot reach `yon.*`. |
| `@Fake` | A field that the test assigns. In JavaScript and TypeScript, `construct(Role)` passes it to any role with a `@Needs` field of that name. At most 32 per unit. Each must be a field without an initial value and not static. |
| `@FreshStart` | Disposes the context between cases. Accepted, and the driver already does this. |

**Bare markers.** A marker without arguments is written bare, as Spring writes
it: `@Service`, `@Helper`, `@Store`, `@Setup`, `@Needs`,
`@OnStart`, `#[Service]` in Rust and `[Service]` in C#. This is true in
every language (ADR 0038). The form with an empty argument list, `@Service()`,
is the same marker and is still accepted. The file location decides the realm
of a bare `@Service`: under `client/` it is the Tac service, under `server/` it
is Yon's.

**Generated helpers.** Tachyon generates `startBehaviors()` and
`stopBehaviors()`, so a runner or the driver can open and dispose the context
around each case. In JavaScript and TypeScript, `construct(Type)` also passes a
role its `@Fake` fields. It refuses a mock that names no `@Needs` field
on that type.

**Refused Spring test markers.** Tachyon refuses Spring's slicing and mocking
markers; it does not silently ignore them. `@WebMvcTest`, `@WebFluxTest`,
`@DataJpaTest`, `@JsonTest`, `@RestClientTest`, `@AutoConfigureMockMvc`,
`@SpyBean` and `@MockitoSpyBean` each fail with an error that names the
supported marker. Reasons:

- There is nothing to slice. An invocation owns its graph of instances, and a
  Setup is always selected explicitly. So `@Test` replaces the
  whole slicing family.
- There is no bean to wrap. So `@Fake` replaces the spies.

`@SpringBootTest` is retired. Use `@Test`.

**Injection.** A field carries `@Needs`, like a field with Spring's `@Autowired`.
`@Named` names the provider when the field's own name or type does not.

`@NeedsAll` takes no arguments, like Lombok's `@RequiredArgsConstructor`
(ADR 0056). A field list such as `@NeedsAll("a", "b")` is refused, and
`ty migrate fix --write` removes it. A class with `@NeedsAll` declares no
constructor (`constructor`, `__init__`, `__construct`, `initialize`, `init`,
`fn new` or a Java, Kotlin, C# or Dart constructor): `@NeedsAll` builds the
class. `@Needs(A, B)` on a class is an older form that still parses while
published sources migrate.

On a Tac service, a bare `@NeedsAll` has the meaning of Lombok's
`@RequiredArgsConstructor` (ADR 0039):

- It takes every field without an initial value, in declaration order. The
  component properties with the same names fill them.
- In Rust, C# and Go, fields always have an initial value. There, it lets
  every field take the property with its name.
- A service that takes its component's properties exists once per component,
  because a page singleton is constructed with no properties. So it needs no
  `@Lifetime`.
- `@Lifetime({"value":"prototype"})` is still available for a service that keeps
  state per component without taking properties.
- A field that Tachyon fills, a required `@NeedsAll` field or a `@Needs`
  field, is never null. The build refuses one that is declared nullable with
  `TY1331` in both realms (ADR 0057 amendment) and shows the language's
  non-null form: `open!: boolean` (TypeScript), `late final String path;`
  (Dart), `val path: String` or `lateinit var store: Store` (Kotlin),
  `let path: String` or `var store: Store!` for a `@Needs` field (Swift),
  `public required string Path;` (C#), `path: String` instead of
  `Option<String>` (Rust), `T $store` (PHP) and `store: T` (Python). A field
  that may stay empty is plain state with a starting value, such as
  `note: string | null = null`. `ty migrate fix --write` rewrites the simple
  cases.
- A field that holds an injected class, a `@Needs` field or a Tac `@NeedsAll`
  field whose type is a class, uses the language's variable case, never the
  class name: `cartStore` in JavaScript, TypeScript and Dart (`_cartStore`
  is allowed in Dart), and the Yon case of the language for a `@Needs` field
  in a native service. The
  build refuses `CartStore` with `TY1332` (ADR 0057 amendment). Only the case
  is checked, not the words; `ty migrate fix --write` renames the field, its
  uses in the class and the view bindings such as `{cart.CartStore.size}`.

**Yon endpoints.** Yon endpoints still use uppercase `GET`, `POST` and so on.
Tachyon accepts no HTTP-verb decorators. Cached is not allowed on these
endpoints, on purpose. Authenticate and validate the request before you call a
service with a Cached method. The cache authorization hook runs on every access,
including hits and eviction. Only literal `true` allows access.

Controller, StreamRoute, SocketRoute and Schedule are the only Yon-only APIs. These are
mirrored in both realms (ADR 0031): Service, Lifetime, HTTPClient, Helper, Converter,
Store, Needs, Retry, MaxAtOnce, AllOrNothing,
Description, and the lifecycle and event markers.

**Transaction limits.** A transaction is bounded on both sides:

- The behavior runtime holds at most one open transaction per context. It
  always finishes the transaction before the method returns.
- The managed Yon datasource holds at most four transactions at once. Each one
  keeps one connection for at most ten seconds of idle time. After that, the
  datasource rolls it back, and its identifier is no longer accepted. So a
  handler that dies during a transaction cannot keep a connection open.
- Authored SQL still cannot contain `BEGIN`, `COMMIT` or `ROLLBACK`. Only the
  transaction operations emit them.

### Declared exchanges and queries

Two kinds of methods are declared, not written:

- a repository's derived methods and `@Query` methods;
- an HTTP request method whose parameter annotations map its arguments.

Both are a signature that the compiler fills in.

Spring writes these as an interface, and so does Tachyon. Declare the role with
the abstract construct that its language already has. The compiler writes the
members: the behavior context, the constructor and every body.

| Language | Declaration | Method |
| --- | --- | --- |
| Java, Groovy, Kotlin, PHP | `interface CatalogClient` | no body |
| C# | `public interface CatalogClient` | no body |
| Go | `type CatalogClient interface { … }` | a method of the interface |
| Dart | `abstract interface class CatalogClient` | no body |
| Swift | `protocol CatalogClient` | no body |
| Rust | `trait CatalogClient` | no body |
| F# | `type CatalogClient =` with only `abstract` members (an F# interface) | an `abstract` member |
| Python | `class CatalogClient(Protocol)` | `...` |
| JavaScript, TypeScript, Ruby | `class CatalogClient` | an empty body |

JavaScript and Ruby have no method without a body, and TypeScript accepts
annotations only on a method with a body (an interface or an `abstract` member
cannot carry `@GetRequest`). So those languages write a class whose methods
have empty bodies. Everywhere else, the declaration has the same shape as
Spring's.

An `HTTPClient` must use this form (ADR 0048). A client written as a class in a
language that has a contract form, any method body, a field, a constructor, a
method without a request or gateway annotation, or a client that also carries
another role fails with `TY1308`: "An HTTPClient has no implementation:
Tachyon generates each request. Move the logic into a service."
A C# client, store or converter without `public` also fails with `TY1308`:
C# makes it internal, and a public class that needs it would fail
`dotnet build` with CS0051.
`ty migrate fix --write` rewrites a class client into the contract form when
its methods only send requests.

Tachyon builds the client and its behavior context. The context comes from a
Setup: `{"setup":"CatalogSetup"}` on `@HTTPClient` names it, and the default is
the client name with `Setup` instead of `Client`. The partition is the adapter
name. A client whose methods are all `@Gateway` methods needs no context. In
Yon, `@Needs` injects the client. A Tac service in Rust, Go, Kotlin, Swift,
Java, Ruby, C# or Dart gets it with `@Needs` too (in Kotlin `@Needs private val
catalog: CatalogClient`, in Swift `@Needs private let catalog: CatalogClient`
and in Java `@Needs private final CatalogClient catalog;`, which Tachyon fills
with `CatalogClient()` or `new CatalogClient()`; in Ruby `#[Needs]` stands
above `attr_reader :catalog` with a `# @return [CatalogClient]` comment, and
Tachyon builds it with `CatalogClient.new`). A Tac service in F# builds it:
`CatalogClient()`. Tac JavaScript and TypeScript clients stay managed by the
page.

In the example below, `findByStatus` and `countByStatus` are derived from their
names and checked against the field allowlist. The client methods are declared
the same way: `page` and `tenant` become the request.

```java
@Store(ProductEntity.class, {"fields":["id","status"],"defaultSize":20,"maxSize":100})
interface ProductStore {
  Object findByStatus(Object status);
  Object countByStatus(Object status);
}

@HTTPClient
interface CatalogClient {
  @GetRequest("/products")
  Object search(@RequestParam("page") Object page, @RequestHeader("X-Tenant") Object tenant);

  @PostRequest("/products")
  Object save(@RequestBody Object product);
}
```

```javascript
@HTTPClient
class CatalogClient {
  @GetRequest("/products")
  async search(@RequestParam("page") page, @RequestHeader("X-Tenant") tenant) {}

  @PostRequest("/products")
  async save(@RequestBody product) {}
}
```

A `Store` uses the same form. A Store written as a class in a language that
has a contract form, any method body, a field, a constructor, a method that is
neither a derived query nor a `@Query`, or a Store without an Entity fails with
`TY1308`: "A Store has no implementation: Tachyon generates each query. Move
the logic into a service." `ty migrate fix --write` rewrites a class store
into the contract form when its methods are only queries (ADR 0059
amendment).

The request is built only from the arguments. Tachyon resolves parameter
annotations at compile time and evaluates no configuration expressions.
A missing argument is refused before the call reaches the adapter.

### Managed constructor inference

On RequestRoute controllers, Service, Helper or Converter, each
field that carries `@Needs` becomes a parameter of the generated
constructor, in declaration order. The field must be direct, without an initial
value, and not static. You do not need to write a constructor.

How a field finds its provider:

- Typed dependencies resolve to exactly matching concrete provider types. Rust
  uses `Arc<Provider>` or `std::sync::Arc<Provider>`.
- JavaScript fields must exactly match lowerCamel provider names. For example,
  `MoneyService` becomes `moneyService`, and `HTTPClient` becomes
  `httpClient`.
- `@Named('MoneyService')` next to the marker names the provider when the
  field itself does not.

A controller declares its route in `server/controllers/MoneyController.ts`:

```typescript
@RequestRoute('/money')
@Public
class MoneyController {
  @Needs
  money: MoneyService;
  @GetRoute
  getMoney() { return { amount: this.money.amount() }; }
}
```

`MoneyService` must be a managed provider found in the same entry scope. Here
it is `server/services/MoneyService.ts`, which every route shares (ADR 0031).

Rules:

- Fields with an initial value and static fields are not injected.
- These selected dependencies fail: missing, ambiguous, optional, generic,
  interface or function dependencies.
- Tachyon uses no reflection and scans no modules.
- Generated constructors reject an authored constructor.
- A Rust provider without dependencies marks no field, so it writes its own
  inherent `fn new()`.

This inference does not cover:

- primitive settings;
- Setting bindings;
- behavior contexts;
- arbitrary datasource adapters;
- a global Tac DI container.

ADR 0027 and ADR 0031 manage these separately: the reserved Yon provider tokens
(DataSource, Redis and ApplicationEventPublisher), and the browser provider
graph of Clients and Helpers. Use explicit configuration
factories, `Behaviors.values`/`Behaviors.properties` and context ownership as
before. See [Yon annotations](YON_ANNOTATIONS.md),
[ADR 0025](adr/0025-required-constructor-inference.md) and
[ADR 0031](adr/0031-realms-mirror-each-other.md).

### Explicit construction outside managed roles

TypeScript decorators cannot add constructor signatures for the type checker.
For Tac types and unmanaged behavior types, use declared fields and the typed
factory, which validates its input at run time. Here, `context` is the value
that `Behaviors.create(...)` returned:

```typescript
class Catalog {
  @Needs
  behaviors!: BehaviorContext;
}

function createCatalog(context: BehaviorContext): Catalog {
  return Behaviors.construct(Catalog, { behaviors: context });
}
```

The type checker checks field names and value types. The runtime checks the
exact set of required fields. Missing, extra, accessor and `undefined`
arguments fail. JavaScript may also call the generated positional constructor
directly.

## Typed-model converter signatures

A Converter may declare `map(source: Product): ProductSummary` without an
`@Convert` marker or a field plan. Both types must be models in the `models/`
folder of the realm. JavaScript gives the types with JSDoc `@param` and
`@returns`, and its model files may declare JSDoc `@typedef` models. JavaScript
roles may also use TypeScript model files.

The compiler copies each target field from the source field with the same name.
It turns integer values into numbers, and it maps nested models and lists the
same way. Source fields that the target does not have are dropped. A source
field that may be null cannot fill a required target field. A target field that
is missing or has the wrong type fails the build, and the error names the field.
Models may nest at most 8 levels deep; map deeper levels with their own method. An explicit
`@Convert({"fields":[...]})` sets the named target fields, and the compiler
fills in the rest. A method that takes and returns JSON values still needs a
complete field plan. One method cannot mix model types and JSON values.

Server tests run these mappings in JavaScript, TypeScript, Python, Ruby, PHP,
Dart, Go, Java, Groovy, Kotlin, C#, F#, Rust and Lua, with nested models and
lists. Browser tests run them in JavaScript and TypeScript in Chromium, Firefox
and WebKit. Dart, Go, Java, Kotlin, C#, F#, Rust, Ruby, Swift and Lua are
compiled to WebAssembly and run. Ruby rebuilds typed results from the model
information that Tac records, and F# uses generated record readers and writers.
These tests cover web builds only, not native apps.

In Lua, a Converter is a class table marked `-- @Converter`. Its methods are
empty and use LuaCATS type comments for the models. A service or controller
receives it through a typed `-- @Needs` field. Plain mappings run in Yon and in
the Tac Lua WebAssembly runtime. Other Tac behavior annotations and Lua
resources are not available yet; see
[ADR 0085](adr/0085-lua-is-a-yon-language.md).
Swift is a Tac language only, so it has no server test.

## Adapter Contracts

The JS/TS interfaces are in `types/tachyon-behaviors.d.ts`. This section
describes the native contracts and tested examples.

Execution objects carry:

- `environment` (`tac` or `yon`);
- `partition`;
- `signal`;
- an absolute `deadline`.

Adapter failures are reported with sanitized `BehaviorError` codes. Exceptions
from application methods keep their normal application meaning.

### Cache

- `get(key, execution)` returns `{hit:false}` or
  `{hit:true,value,expiresAt}`.
- `set(key, value, execution)` receives the policy and the absolute
  `expiresAt`.
- `delete(key, execution)` removes the selected entry.
- `context.evict(instance, 'method', arguments)` builds the same key again and
  calls the authorization hook before it deletes the entry.

**Keys.** A key contains the partition, the cache name, a stable hashed
identity of the source module, the class, the method and the explicitly
selected arguments. So services with the same name in different modules cannot
share entries by accident. Applications that share storage with other
applications must still use separate cache namespaces or partitions. Include
every input that changes the result.

**Limits:**

- Keys are at most 4 KiB.
- Cached values are finite JSON. Tachyon copies them to protect them from
  changes.
- Tachyon never stores an error, generator, accessor, cyclic object or any
  executable key expression.
- TTL (time to live) is 1 ms to 24 hours.
- A value is at most 1 MiB.
- The entry quota is at most 10,000.

The storage adapter that the application selects must enforce the supplied
entry quota and replace entries atomically. Tachyon validates values and
expiry, but it cannot enforce the physical capacity of an external database.
When the cache is down, the operation fails. It does not quietly run side
effects again.

An in-memory adapter lives only as long as its owner. A Yon application that
needs reuse across requests must select a shared external cache. Tac
applications own their browser and API storage policy, and must not bundle
server credentials.

### Cache mutations

`UpdateCache` and `ClearCache` select a Cached method in the same class. Their
`arguments` array maps parameter positions of the mutation method to the raw
argument list of the target. The target's PageDefaults normalization runs before
key selection. So reading, putting and evicting all use the same normalized
key. The target's CacheSettings namespace, TTL, byte limit, module identity and
authenticated partition apply.

Order of checks:

1. Authorization uses `cache:put` or `cache:evict`. Reads use `cache:read`.
2. Validation of the target's arguments and key, and lookup of the required
   adapter, happen before the application's mutation runs.
3. Set or delete happens only after a successful, bounded JSON result, followed
   by cancellation and deadline checks.

Put never reads from the cache. A failure, an invalid result or a cancellation
does not update the cache. Adapter errors pass through to the caller.

This is not an atomic transaction across the database and the cache. If the
database change succeeds and the cache write then fails, the application must
fix the mismatch.

Use at most one of Cached, UpdateCache and ClearCache on a method. Mutation
methods cannot declare their own PageDefaults, and they cannot be HTTP verb
endpoints. An HTTP request annotation may be combined with a mutation policy: its successful
transport result is the mutation result. Check may reject input before this
pipeline starts.

### Pagination and Persistence

**Pagination.** Pagination accepts integer `page` and `size` values and a sort
array such as `["name:asc","id:desc"]`. A field without a direction sorts
ascending. These fail: unknown fields, unknown directions, repeated sort fields
and offsets greater than 1,000,000.

**Sync and async.** Methods that use only PageDefaults keep their synchronous
return behavior. Caching and HTTP exchange use async methods in
JS/TS/Python/C#/Swift and synchronous methods in Java/Kotlin/PHP/Rust. A Tac
Rust HTTPClient method may also be an `async fn`; it awaits the browser
transport through Tachyon's small executor in the WebAssembly module (ADR
0090). A Tac Kotlin HTTPClient method may be a `suspend fun`; the Kotlin/Wasm
module keeps its continuation while the browser transport works (ADR 0090). A
Tac Java HTTPClient method stays synchronous and may return a model from
`client/models`; without a Setup adapter, the TeaVM fiber that runs the
service call waits for the browser transport and the page gets a promise
(ADR 0090). A
Tac Ruby HTTPClient method stays synchronous, and its YARD `@return` may name a
model from `client/models`; without a Setup adapter, the Ruby fiber that runs
the service call waits for the browser transport and the page gets a promise
(ADR 0090). A
Tac Swift HTTPClient method is `async throws` and may return a `Codable`
model; without a Setup adapter the Swift WebAssembly module parks its task
while the browser transport works (ADR 0090).
The synchronous integrations do not quietly adapt futures.

**Repositories.** `context.repository(ProductEntity)` provides `find(page)`,
`save(value)` and `delete(id)`. The adapter's `execute(command, execution)`
receives:

- a fixed mapping of entity, table and id field;
- a page, value or id, depending on the operation.

Results are bounded JSON, not model instances filled in automatically.
Repository paging defaults to size 20, with a maximum of 100. The id field is
the only field allowed for sorting. Identifiers are bounded strings or safe
integers.

Adapters must use parameterized operations and enforce authorization and
tenancy. The annotations are not an ORM (object-relational mapper) and not an
authorization system.

### Structured stores and derived queries

A Store is a contract, like a Spring Data repository interface. It names its
Entity, and each method is a query that Tachyon writes. Tachyon builds the
store and its behavior context from a Setup: the one that
`{"setup":"CatalogSetup"}` names, or by default the store name with `Setup`
instead of `Store`. The partition is the store name in lowerCamel case without
`Store`.

- The store is a provider: a controller or service asks for it with
  `@Needs`, and never builds it. This is the same in Yon and in Tac.
- In a Tac JavaScript or TypeScript service, the owner of the service creates
  the store's context before the service starts, because a browser creates a
  context asynchronously. A root service that asks for a store does not
  declare its own `behaviors` member.
- In a Tac Kotlin, Swift, Java or C# service, Tachyon gives the field a new
  store. A Rust service declares a `std::sync::Arc<ProductStore>` field and
  fills it with `Default` (derive it, or write no `fn new`). In Go, Tachyon builds the store before the service starts. Dart
  builds it with the service. A Ruby service documents its `attr_reader`
  with `# @return [ProductStore]`, and Tachyon builds the store the first time
  the reader runs. An F# service takes the store as a primary constructor
  parameter, `type ProductService(products: ProductStore) =`, and Tachyon
  turns it into `let products = ProductStore()`.
- A Yon Lua store is a class table marked `-- @Store(ProductEntity, {...})`
  whose methods have empty bodies. Tachyon adds the provider role and an
  `-- @OnStart` method that opens its context, so a typed `-- @Needs` field
  receives it. The Setup's entity adapter is a table with an `execute` method.
  Lua has no base classes, so a Lua store declares `find_by_id` and its other
  queries instead of extending `Repository` (ADR 0085).
- A Store without an Entity fails with `TY1308`. Code that only runs SQL
  belongs in a service that asks for a `DataSource`.
- Do not put the Service role and the Store role on one type.

#### The Repository base

Like Spring Data's `CrudRepository`, a Store can extend
`Repository<Entity, Id>` and then gets four methods without declaring them:

- `findById(id)`: the record with that id, or null (a `findOne` query on the
  Id field, whatever its name);
- `findAll(page)`: one page of records ordered by id; pass `{}` for the first
  page of the default size;
- `save(entity)`: inserts or replaces the record through the entity adapter
  and answers the saved record;
- `deleteById(id)`: deletes the record and answers how many records it deleted.

Each language names the base in its own contract form: `extends
Repository<ProductEntity, Integer>` (Java, Groovy), `: Repository<ProductEntity,
Int>` (Kotlin), `: Repository<ProductEntity, int>` (C#), `extends Repository`
(PHP), an embedded `Repository[ProductEntity, int64]` line (Go), `implements
Repository<ProductEntity, int>` (Dart), the supertrait
`Repository<ProductEntity, i64>` (Rust), `protocol ProductStore: Repository`
(Swift), `class ProductStore(Repository[ProductEntity, int], Protocol)`
(Python), `inherit Repository<ProductEntity, int>` (F#), `extends
Repository<ProductEntity, number>` (TypeScript), `extends Repository` with
`/** @extends {Repository<ProductEntity, number>} */` (JavaScript) and
`class ProductStore < Repository` (Ruby). The compiler removes the base and
writes the four methods next to the other queries; TypeScript and JavaScript
type-check against the `Repository` class in `types/tachyon-behaviors.d.ts`.
The Repository must name the Entity of its Store, and only a Store can extend
it; both fail with `TY1308`. In the compiled languages the results are the
runtime's JSON values (`Object`, `Any?`, `BehaviorJSON`, `BehaviorValue`), like
every other query result.

A derived `deleteBy` method (`delete_by_` in snake_case) is a `delete` query:
it removes every record that its predicates match and answers how many it
removed. The entity adapter receives `{"operation":"query","query":{"kind":"delete",…}}`;
the managed SQL datasource runs one `DELETE … WHERE`, FYLO and IndexedDB delete
each matching row, and inside `@AllOrNothing` the delete joins the open
transaction.

A transaction belongs to a service, like Spring's `@Transactional`: put
`@AllOrNothing` on the service method that calls the store. A store call inside
that method joins the open transaction when the store's entity uses the same
entity adapter, even though the store was injected with its own behavior
context. Each runtime keeps the open transactions in storage that belongs to
one call (AsyncLocalStorage, ThreadLocal, AsyncLocal, a context variable, fiber
storage, a zone, a task local, a thread local, one entry for each goroutine, or
the PHP request), keyed by the entity adapter name. An `@AllOrNothing` method on
a Store fails with `TY1308`.

Entity, Store and the schemas they reference belong to the same
explicitly assembled language unit. Each primary role is in its own matching
file.

`server/entities/ProductEntity.js`:

```javascript
@Entity({"adapter":"data"})
@Table('products')
class ProductEntity {}
```

`server/stores/ProductsStore.js`:

```javascript
@Store(ProductEntity, {"fields":["id","status"],"defaultSize":20,"maxSize":100})
class ProductsStore {
  @Query({"operation":"find","where":[{"field":"status","operator":"eq","argument":0}],"orderBy":[]})
  async byStatus(status, @PageDefaults page) {}

  async countByStatus(status) {}
  async existsByStatus(status) {}
}
```

Put both role files in the shared role folders of their realm
(`client/entities` and `client/stores` for Tac).

**Query methods.** Query methods have no body (Python writes `...`, and
JavaScript, TypeScript and Ruby write an empty body) and the language's bounded
JSON result type. The compiler supplies the execution. A body that is not
empty is an error. Tachyon does not silently throw that logic away.

**Derived methods.** Derived method names use
`(find|count|exists)By<Field><Operator>[And...]`, or the equivalent snake_case
names. Operators:

- Equals (also the default);
- NotEquals;
- LessThan, LessThanEqual;
- GreaterThan, GreaterThanEqual;
- In;
- IsNull, IsNotNull.

Every parameter must be used. Unknown fields and ambiguous names fail. Use
structured Query when the name would be unclear.

**Structured Query.** Query options are `operation`, `where` and `orderBy`.
A `find` takes its page request from the parameter marked `@PageDefaults`.

- Predicates combine with AND.
- Operators are `eq`, `ne`, `lt`, `lte`, `gt`, `gte`, `in`, `isNull` and
  `isNotNull`.
- Scalar bindings are finite JSON scalars, including null.
- In accepts at most 100 scalars.
- Not accepted: an expression language, raw SQL, joins, nested paths, OR, any
  other database operators, and bulk mutation.
- Bindings stay data. The adapter that the application selects turns them into
  parameters.

The entity adapter receives a command with this shape:

```text
{ operation: "query", entity, table, idField,
  query: { kind: "find" | "count" | "exists",
           where: [{field, operator, value?}], orderBy, page? } }
```

**Authorization.** The required `authorizeEntity(command, execution)` hook
runs before `execute`. If it is missing or returns false, the query fails
closed (it is refused).

**Results and limits:**

- Find uses `{page,size,offset,sort}`. The annotation owns the ordering, and
  the compiler adds the ID as a tie-breaker.
- The caller's `page.sort` must be empty.
- Count and exists have no page.
- Limits: 64 declared fields, eight predicates, four authored sort fields,
  64 KiB of bindings and 1 MiB of results.
- Find returns an array of objects, no longer than the page size (maximum
  100).
- Count returns a nonnegative safe integer.
- Exists returns a boolean.

In this slice, Query cannot be combined with other method policies. Explicit
repository CRUD still requires authorization and tenancy in the adapter.

## Explicit configuration and profiles

**Settings.** Settings binds an application
snapshot that is supplied explicitly and can be nested. It also accepts an
explicit legacy in-memory string map. It never reads the process environment,
scans packages or runs constructors or getters. Application code decides
which allowed values to pass.

Prefix and key rules:

- Prefixes must start with `TAC_` in Tac or `YON_` in Yon, end with `_`, and be
  at most 128 ASCII characters.
- Full keys match `[A-Z][A-Z0-9_]*` and are at most 256 characters.
- Only the current framework's prefix is accepted. This API does not widen
  Yon's environment allowlist.
- Never put server secrets into Tac configuration or bundles.

Field rules:

- Each field declares an uppercase suffix `name`, and a `type` of string,
  boolean, integer or number.
- A field may declare a typed `default`, and numeric `min`/`max`.
- A missing required value is an error. An unknown key under the selected
  prefix is an error.
- The supplied map may also contain other allowed prefixes of the same
  framework.
- Booleans accept only `true`/`false`.
- Integers use strict integer JSON tokens: no decimal point, exponent, leading
  plus, leading zero or whitespace.
- Numbers use strict JSON number tokens.
- Integers and numbers must be finite, with an absolute value of at most
  9,007,199,254,740,991.
- Numeric constraints do not convert strings or booleans.
- Input limits: 256 entries, 4 KiB per value and 64 KiB of canonical JSON in
  total.

**Setting.** Setting binds one field from the parsed application configuration.
This is the object that `YON_APPLICATION_JSON` carries (see
[application configuration](APPLICATION_CONFIGURATION.md)).

- A path is canonical dotted `yon.*` for Yon or `tac.*` for Tac. Segments are
  lowercase ASCII letters and digits.
- Every underscore in a prefixed property name starts another segment:
  `YON_APP_SIZE` means `yon.app.size`.
- Server secrets never reach a client.
- The string form `"${path}"` or `"${path:default}"` belongs to the Setting
  annotation. It is not interpolation in `application.json`.
- The string form binds the fixed type of a known field (`yon.server.port` is
  an integer). Other fields bind as strings.
- The object form declares `type`, `default`, `min` and `max`, with the
  Settings rules.
- `Behaviors.values(Type, application)` validates the application shape,
  converts every declared field, and returns a bound JSON object keyed by
  field name. It never constructs, changes or reflects over an instance.
  Assign the result where the application wants it.
- These fail with `configuration-properties`: missing values without a
  default, unknown namespaces, a `yon.server.port` that is not an integer, and
  values out of range.

**WhenProfile and WhenSetting.** These apply only to Setup.

- Profiles match any name that is explicitly active.
- The property condition uses exact string equality, or its declared
  `matchIfMissing` value.
- When both are present, both must pass.
- Creation accepts at most 32 unique profile names that match
  `[A-Za-z_$][A-Za-z0-9_$]{0,63}`.
- Tachyon takes a snapshot of the selection inputs. Later changes do not
  reconfigure an existing context.
- Tachyon invents no ambient or default profile.

Error codes, all raised before `configure` runs:

- invalid creation inputs: `configuration`;
- binding failures: `configuration-properties`;
- rejected selection: `inactive-configuration`.

Errors do not repeat the supplied values.

In the example below, `CatalogSettings` and `Endpoint` are schema classes. The
function `createApplicationContext` uses them. Call it inside an explicitly
owned invocation, and pass that invocation's signal:

- `settings` is `{size: 25}`.
- `Behaviors.values` reads the application snapshot, not the separate `values`
  map. If the snapshot is empty, `endpoint` gets the defaults
  `{port: 8080, size: 20}`.

```javascript
@Settings({"prefix":"YON_APP_"})
class CatalogSettings {
  @Min(1)
  @Max(100)
  size = 20;
}

class Endpoint {
  @Setting("${yon.server.port:8080}")
  port;
  @Setting({"path":"yon.app.size","type":"integer","default":20,"min":1,"max":100})
  size;
}

async function createApplicationContext(signal) {
  const values = { yon: { app: { size: 25 }, cache: { enabled: true } } };
  const settings = Behaviors.properties(CatalogSettings, values);
  const application = JSON.parse(process.env.YON_APPLICATION_JSON ?? '{}');
  const endpoint = Behaviors.values(Endpoint, application);
  const context = await Behaviors.create(ApplicationSetup, {
    partition: 'public-catalog', signal, profiles: ['test'], properties: values,
  });
  return { settings, endpoint, context };
}
```

`ApplicationSetup` is a separate selected role module, in
`setups/ApplicationSetup.js`:

```javascript
@Setup
@WhenProfile('development', 'test')
@WhenSetting({"name":"YON_CACHE_ENABLED","havingValue":"true","matchIfMissing":false})
class ApplicationSetup {
  static configure() { return {}; }
}
```

## DTO and method validation

```javascript
@Checked
class ProductInput {
  @NotEmpty
  @Length({"min":1,"max":80})
  name;

  @Min(0)
  price;
}
class ProductService {
  /** @param {ProductInput} input */
  accept(@Check input) { return input; }
}
function validateProductInput() {
  return Behaviors.validate(ProductInput, {name: 'Notebook', price: 5});
}
```

Validation requires a bounded JSON object. It allows unknown fields. It returns
a checked copy or value. It does not construct the DTO, call getters or assign
fields.

Rules for each constraint:

- **Required** rejects missing or null.
- **NotEmpty** requires a string that is not empty and has at least one scalar
  outside Unicode White_Space. White_Space is: U+0009–000D, U+0020, U+0085,
  U+00A0, U+1680, U+2000–200A, U+2028, U+2029, U+202F, U+205F and U+3000.
- **Length** counts UTF-8 bytes, not characters and not UTF-16 units. For arrays
  it counts elements, and for objects it counts keys.
- **Length/Min/Max** ignore missing or null. Add Required when the value is
  required.
- **Numeric bounds** accept only finite JSON numbers, never numeric strings.

Limits of the feature:

- Constraints apply to direct named fields only.
- Not implemented: nested Check, validation groups, walking up the inheritance
  chain, and arbitrary expressions.
- A schema has at most 64 constrained fields.
- Length `min`/`max` are integers from 0 to 1,048,576, and `min` cannot be
  greater than `max`.
- Numeric constraints use the same finite safe-number range as the JSON
  boundary.

Methods that use only Check may stay synchronous and need no context. With
adapter policies, validation runs first. Then come pagination, cache
authorization and read, HTTP, and the method body.

Errors use the code `validation` and a bounded path of field and rule, such as
`name.notBlank`, or `$.json`, `$.object`, `$.schema`. Messages do not include
input values.

Tac validation helps the user interface. It is not server authorization. Yon
must validate again. Accepting unknown fields does not give permission for
mass assignment.

### Outbound HTTP

An HTTPClient contract selects a transport and optional base path. Its request
methods are signatures whose parameter annotations map each argument. Use
`RestClient` from a service when the shape of the request is known only at
call time. The wrapper calls the named transport's
`exchange(request, execution)` and returns its bounded JSON result.

- The adapter owns a trusted base origin.
- Annotation paths are literal absolute paths, not absolute URLs.
- Tachyon rejects traversal, other authorities, fragments and embedded query
  strings.
- Request metadata includes `redirect:'error'`, `credentials:'omit'`,
  cancellation, the deadline and a 1 MiB response limit.
- The adapter must enforce these controls while it reads the response. It must
  not download a body of any size and check afterward.
- There are no automatic retries.

The decorator does not change TypeScript's inferred return type. Use a JSON
return type, and validate the adapter response in application code. Browser
transports still follow browser origin and CORS rules. Neither this annotation
nor configuration does any of these: grant native capabilities, read server
secrets, widen Yon's environment allowlist, or bypass the deployment's egress
policy.

### Gateways

A Gateway method is answered by code in another language. It works only in
Yon. Declare it on a managed role, usually an `HTTPClient` contract, as a signature. The configuration names one of two targets:

- `{"class":"server/services/ReportService.java","method":"summarize"}` names
  a role file in another Yon language. `method` is optional and defaults to the
  annotated method's name. Tachyon writes a generated unit in the target's
  language at `server/gateways/<id>/yon.<ext>`, inside its own snapshot. The
  unit composes that language's role files, creates the class with its own
  dependencies, and calls only the methods that callers declared. It speaks
  Handler Protocol v1 with the operation `gateway.invoke`. It runs like a
  route: same environment policy, its own managed storage, and no Gateway
  table of its own.
- `{"command":["ruby","server/helpers/report.rb"]}` names a program. The
  first entry is a program name on the supervisor's `PATH`. Every entry that
  contains a slash, or starts with a drive letter such as `C:`, is a file
  under `server/helpers/` in the captured snapshot. No shell runs, and the arguments never go on the command line.
  The program reads `{"method":"summarize","arguments":[...]}` on stdin and
  writes the bare JSON result on stdout. Exit status 0 is success; any other
  status is a failure. The program gets only the baseline environment and the
  explicit allowlist, never `YON_APPLICATION_JSON` or a `YON_RESOURCE_*` value.
  This prevents accidental inheritance; it is not a sandbox (see
  docs/THREAT_MODEL.md).

The caller sends only the key `<Type>.<method>` that the build declared,
through the invocation's managed resource endpoint. The supervisor starts a
new process for each call.

| Limit | Value |
| --- | --- |
| Arguments | 32 |
| JSON each way | 1 MiB, including the small envelope |
| JSON depth and values | 32 levels, 10,000 values |
| Calls per invocation | 64 (the 65th fails with `capacity`) |
| Calls at the same time per invocation | 4 |
| Process pool | separate from the handler pool, same size |
| Deadline | the caller's remaining deadline minus the cancellation grace |

Errors reach the caller only as a BehaviorError with one of four codes and
the fixed message `Gateway call failed.`:

- `invalid`: the arguments are not bounded JSON, their count is wrong, or the
  key is not declared;
- `timeout`: the deadline passed or the invocation was cancelled;
- `capacity`: the invocation made more than 64 calls;
- `adapter`: the target failed, exited with another status, wrote something
  that is not bounded JSON, or could not start.

The target's own messages and stderr never reach the caller. The supervisor
logs only `{"event":"handler.gateway_failed","request_id":...,"category":...}`.

Only one hop is allowed. A target class that declares a Gateway is refused at
build time, and a Gateway reached from inside a generated unit fails with
`adapter`. Gateways are refused under Firecracker isolation (`TY2010`),
because resource capabilities are not passed into the microVM.

The build checks every target and fails with `TY1312` when:

- the class file is missing, outside a `server/` role folder, or not in the
  snapshot;
- the class is in the caller's own language (inject it instead);
- its language is not a Yon language;
- the method is missing, static or not concrete;
- the argument counts differ;
- a typed parameter or result is not the language's JSON value type;
- more than one concrete JSON signature matches the declared argument count;
- callers select different argument counts for the same target method;
- a Rust target consumes `self` or uses `&mut self` instead of `&self`;
- the target class declares a Gateway itself;
- a command file is not a regular file under `server/helpers/`.

A misused annotation fails with `TY1308`: in Tac, on an HTTP verb method, on
a method with a body, together with another method annotation, or with a
configuration that is not exactly one of the two target forms. An authored
file under `server/gateways/` fails with `TY1310`. A missing command program
fails the start-up preflight with `TY2112`; the program is looked up on the
`PATH`, not run. A target language whose role files use local events or
lifecycle hooks cannot host a generated unit yet (`TY1312`).

| Language | Declaration in the `HTTPClient` contract |
| --- | --- |
| JavaScript | `@Gateway({...})` then `async summarize(period) {}` |
| TypeScript | `@Gateway({...})` then `async summarize(period: BehaviorJsonValue): Promise<BehaviorJsonValue> {}` |
| Ruby | `#[Gateway({...})]` then `def summarize(period)` and `end` |
| Python | `@gateway({...})` then `def summarize(self, period): ...` (or `async def`) in a `Protocol` class |
| PHP | `#[Gateway({...})]` then `public function summarize($period);` in an `interface` |
| Java | `@Gateway({...})` then `Object summarize(Object period);` in an `interface` |
| Groovy | `@Gateway({...})` then `Object summarize(Object period)` in an `interface` |
| Kotlin | `@Gateway({...})` then `fun summarize(period: Any?): Any?` in an `interface` |
| C# | `[Gateway({...})]` then `BehaviorJSON Summarize(BehaviorJSON period);` in an `interface` |
| Rust | `#[Gateway({...})]` then `fn summarize(&self, period: BehaviorValue) -> BehaviorResult<BehaviorValue>;` in a `trait` |
| Dart | `@Gateway({...})` then `Future<Object?> summarize(Object? period);` in an `abstract interface class` |
| F# | `[<Gateway({...})>]` then `abstract Summarize: period: BehaviorJSON -> BehaviorJSON` in an interface type |
| Go | `// @Gateway({...})` then `Summarize(period BehaviorValue) (BehaviorValue, error)` in an `interface` |

A target method uses the same JSON value type for its parameters. Its result
may be that type or the language's asynchronous or result form. Overloads in
Java, Groovy, Kotlin and C# are selected by argument count and JSON types, not source
order. Every call
starts a process, so a JVM or .NET target pays its start-up time on every
call.

## Language-Native APIs

Each Setup type and behavior type belongs to its explicitly assembled
unit. Native integrations generate direct registrations and constructor calls.
They do not scan the classpath, call methods through reflection or use a global
container. Checked JSON boundaries reject arbitrary application objects. They
do not serialize a description of the object.

| Language | Context creation | Cached and HTTP method result |
| --- | --- | --- |
| JS / TS | `await Behaviors.create(Adapters, {partition, signal})` | Async JSON result |
| Python | `await Behaviors.create(Adapters, {'partition': partition, 'signal': signal})` | `async def`, JSON-compatible built-ins |
| PHP | `Behaviors::create(Adapters::class, ['partition' => $partition, 'signal' => $signal])` | Synchronous JSON-compatible values |
| Java | `Behaviors.create("Adapters", new Behaviors.Options(partition, signal))` | Synchronous `Object`; finite JSON-native scalars, maps and lists |
| Kotlin | `Behaviors.create("Adapters", Behaviors.Options(partition, signal))` | Synchronous `Any?`; finite JSON-native scalars, maps and lists |
| C# | `Behaviors.create(typeof(Adapters), partition, signal)` | `async Task<BehaviorJSON>` |
| Swift | `try Behaviors.create(Adapters.self, partition: partition, signal: signal)` | `async throws -> BehaviorJSON` |
| Rust | `Behaviors::create::<Adapters>(BehaviorOptions::new(partition, signal))?` | `BehaviorResult<BehaviorValue>` |

Query and cache-mutation methods use the same execution and result shape as
cached and HTTP methods in each language. Details:

- C# and Swift: Check parameters use `BehaviorJSON`. Results of Check-only
  methods use `BehaviorJSON` (or C# async `Task<BehaviorJSON>`).
- Swift declares `throws`, also for synchronous validation.
- Rust uses `BehaviorValue` and `BehaviorResult<BehaviorValue>`.
- JVM languages use their strict native JSON maps, lists and scalars, not
  arbitrary DTO instances.

The pure factories are synchronous in every language:

| Language | Configuration binding | Explicit validation |
| --- | --- | --- |
| JS / TS | `Behaviors.properties(CatalogSettings, values)` | `Behaviors.validate(Input, value)` |
| Python | `Behaviors.properties(CatalogSettings, values)` | `Behaviors.validate(Input, value)` |
| PHP | `Behaviors::properties(CatalogSettings::class, $values)` | `Behaviors::validate(Input::class, $value)` |
| Java / Kotlin | `Behaviors.properties("CatalogSettings", values)` | `Behaviors.validate("Input", value)` |
| C# | `Behaviors.properties(typeof(CatalogSettings), values)` | `Behaviors.validate(typeof(Input), value)` |
| Swift | `try Behaviors.properties(CatalogSettings.self, values)` | `try Behaviors.validate(Input.self, value)` |
| Rust | `Behaviors::properties::<CatalogSettings>(&values)?` | `Behaviors::validate::<Input>(value)?` |

`Behaviors.values` takes the parsed application in the language's own JSON
shape. It is spelled like the properties column:

| Language | Application shape | Call |
| --- | --- | --- |
| JS / TS, Python | plain object, dict or array | `Behaviors.values(CatalogSettings, application)` |
| PHP | plain object, dict or array | `Behaviors::values(CatalogSettings::class, $application)` |
| Java / Kotlin | a `Map` from the Yon prelude's `Json.parse` | `Behaviors.values("CatalogSettings", application)` |
| C# | `BehaviorJSON` | `Behaviors.values(typeof(CatalogSettings), application)` |
| Swift | `BehaviorJSON` | `try Behaviors.values(CatalogSettings.self, application)` |
| Rust | `BehaviorValue`; the Yon prelude's `Json` converts with `.into()` | `Behaviors::values::<CatalogSettings>(&application)?` |

To pass profiles and properties at creation:

- JS/TS, Python and PHP add `profiles` and `properties` to their existing
  creation options object or dictionary.
- Java's full overload is
  `new Behaviors.Options(partition, signal, 10000, List.of("test"), values)`.
- Kotlin uses
  `Behaviors.Options(partition, signal, profiles = listOf("test"), properties = values)`.
- C# uses
  `Behaviors.create(typeof(Adapters), partition, signal, profiles: new[] { "test" }, properties: values)`.
- Swift uses
  `try Behaviors.create(Adapters.self, partition: partition, signal: signal, profiles: ["test"], properties: values)`.
- Rust callers fill in `BehaviorOptions.profiles: Vec<String>` and
  `properties: BTreeMap<String, String>`, then pass the options by value.

All earlier defaults and simple creation forms stay valid.

Signals: Python, PHP, C# and Swift use `BehaviorSignal`. Java and Kotlin use
`Behaviors.Signal`. Rust uses `BehaviorSignal::new()`.

Configuration still selects `caches`, `http`, `entities`, the cache
`authorize` behavior and the query `authorizeEntity` behavior. You may leave
out adapters that you do not use. No API chooses a database or gives server
credentials to Tac.

### Java and Kotlin

**Setup.** Java Setup classes declare
`static Behaviors.Configuration configure(Behaviors.Execution execution)`.
Kotlin Setup classes put
`fun configure(execution: Behaviors.Execution): Behaviors.Configuration` inside
a companion object.

- `Behaviors.Configuration` contains typed `CacheAdapter`, `HttpAdapter`,
  `EntityAdapter` and `Authorization` implementations.
- Cache reads return `Behaviors.Entry`. Writes receive `Behaviors.Write` with
  the execution, the policy and the expiry.
- Setup names are generated registry keys, not class loader lookups.

**Services.** Services declare `Behaviors.Context behaviors` (Java) or
`val behaviors: Behaviors.Context` (Kotlin). Generated constructors receive
this context explicitly.

- Use `context.repository("ProductEntity")` and
  `context.evict("Catalog", "list", arguments)`. Eviction normalizes PageDefaults
  arguments.
- Contexts support Java try-with-resources, Kotlin `use` and explicit
  `dispose()`.
- Names that begin with `__ty_` are reserved for generated implementation
  members.

Complete examples: [Java](../crates/tachyon-core/tests/fixtures/behavior/authored/java/server),
[Kotlin](../crates/tachyon-core/tests/fixtures/behavior/authored/kt/server),
[Java adapters](../crates/tachyon-core/tests/fixtures/behavior/JvmRuntime.java), and
[Kotlin adapters](../crates/tachyon-core/tests/fixtures/behavior/JvmRuntime.kt).

### C#

Setup classes provide `static BehaviorAdapters configure(BehaviorExecution
execution)`.

- Collections contain `BehaviorCacheAdapter`, `BehaviorHTTPAdapter` and
  `BehaviorEntityAdapter` implementations.
- Adapter methods return tasks.
- Cache writes receive the policy and expiry together with the execution.
- `authorize` is an async function that returns `bool`.

Run `UseCatalog` inside the invocation or the lifetime of the native owner:

```csharp
class Catalog {
    [Needs]
    public BehaviorContext behaviors;

    public BehaviorJSON page([PageDefaults({"defaultSize":20,"maxSize":100,"sort":["id"]})] BehaviorJSON input) { return input; }
}

class CatalogOwner {
    public static void UseCatalog() {
        using var signal = new BehaviorSignal();
        using var context = Behaviors.create(typeof(Adapters), "public-catalog", signal);
        var catalog = new Catalog(context);
    }
}
```

- `BehaviorJSON.Parse`, `From`, `Object` and `Array` create unchangeable,
  bounded JSON.
- PageDefaults arguments have the type `BehaviorJSON`.
- Use `context.repository(typeof(ProductEntity))` and
  `await context.evict("Catalog", "list", arguments)`.
- Contexts support `using`, `Dispose()` and `dispose()`.
- Adapters can observe cancellation through `execution.Signal.Token`.
- Errors expose `Code` and an optional `Path`, without reflection over
  application objects.

Fixtures: the [annotated fixture](../crates/tachyon-core/tests/fixtures/behavior/csharp-annotations.cs)
and the [adapter contract fixture](../crates/tachyon-core/tests/fixtures/behavior/csharp-runtime.cs).
The [configuration](../crates/tachyon-core/tests/fixtures/behavior/csharp-configuration.cs)
and [validation](../crates/tachyon-core/tests/fixtures/behavior/csharp-validation.cs)
fixtures show native schema markers and factory calls.

### Swift

Setup classes provide `static func configure(_ execution: BehaviorExecution)
-> BehaviorAdapters`. It may be `throws`. Labeled execution arguments are also
supported.

- Adapter maps hold `BehaviorCacheAdapter`, `BehaviorHTTPAdapter` and
  `BehaviorEntityAdapter` values that are backed by closures.
- Adapter operations are `async throws`.
- The authorization closure returns `Bool`.

Run `useCatalog` inside the lifetime of the native owner:

```swift
final class Catalog {
    @Needs
    let behaviors: BehaviorContext

    func page(@PageDefaults({"defaultSize":20,"maxSize":100,"sort":["id"]}) _ input: BehaviorJSON) throws -> BehaviorJSON { return input }
}

func useCatalog() throws {
    let signal = BehaviorSignal()
    let context = try Behaviors.create(Adapters.self, partition: "public-catalog", signal: signal)
    defer { context.dispose() }
    let catalog = Catalog(behaviors: context)
}
```

- `BehaviorJSON` is a finite JSON enum (`.null`, `.bool`, `.number`,
  `.string`, `.array`, `.object`) with strict `parse` and `encoded`
  boundaries. PageDefaults arguments use this type.
- Every decorated method declares `throws`. Synchronous PageDefaults methods stay
  synchronous.
- Use `try context.repository(ProductEntity.self)` and
  `try await context.evict("Catalog", "list", arguments: arguments)`.
- `execution.signal.aborted` reports context disposal and owner abort.
- Errors expose `code` and an optional `path`.

Fixtures: the [annotated fixture](../crates/tachyon-core/tests/fixtures/behavior/swift-annotations.swift)
and the [adapter contract fixture](../crates/tachyon-core/tests/fixtures/behavior/swift-runtime.swift).
The [configuration](../crates/tachyon-core/tests/fixtures/behavior/swift-configuration.swift)
and [validation](../crates/tachyon-core/tests/fixtures/behavior/swift-validation.swift)
fixtures show typed native calls.

### Python, PHP and Rust

**Python and PHP:**

- Python Setup classes use a static `configure(execution)` that returns the
  selected adapter dictionaries.
- PHP uses a static `configure($execution)`. Adapter dictionaries contain
  explicit callables, such as `['get' => [Backend::class, 'get']]`.
- Python and PHP provide `Behaviors.construct`/`Behaviors::construct` with
  explicit named constructor arguments.
- Repository methods take the model class. Eviction takes the owning service
  instance. This matches the JS/TS ownership pattern.
- PHP validation accepts associative arrays or `stdClass` objects. Use
  `new stdClass()` for an empty DTO object. `[]` is an array and is rejected.

**Rust:**

- Rust Setup types return `BehaviorResult<BehaviorConfiguration>` from
  `fn configure(&BehaviorExecution)`.
- Configuration entries are owned `BehaviorCacheAdapter`,
  `BehaviorHttpAdapter` and `BehaviorEntityAdapter` trait objects.
- Services receive `BehaviorContext` explicitly. Generated `Type::new`
  constructors require the complete, explicit list of fields.
- `BehaviorValue` is the strict JSON boundary. Decorated methods accept
  `BehaviorValue` arguments and return `BehaviorResult<BehaviorValue>`.
- Use `context.repository::<ProductEntity>()?` and
  `context.evict(&catalog, "list", arguments)?`.
- `context.dispose()` cancels the context. Dropping its last shared owner also
  cancels it.
- The runtime installs no async executor and no permissive native-bridge
  serializer.

Complete examples: [Python](../crates/tachyon-core/tests/fixtures/behavior/authored/py/server),
[PHP](../crates/tachyon-core/tests/fixtures/behavior/authored/php/server), and
[Rust](../crates/tachyon-core/tests/fixtures/behavior/rust_annotations.yon.rs).

### Go

Go annotations are comments: `// @Service`, `// @Needs` and
`// @Cached` ([ADR 0050](adr/0050-go-annotations-are-comments.md)).
Use one space after `//`. Each marker takes its own line before the type,
method, field or parameter. A marked parameter needs a multiline parameter
list and a trailing comma, as ordinary Go does.

These comments are valid Go and keep their spelling under `gofmt`. Go tools
can parse the file without removing annotations. The examples in this guide
leave out the `package` line; add one, such as `package main`, before you run
`gofmt` on a copied example. Type checking still needs
the declarations that the program uses, including compiler-provided framework
types. Tachyon removes its marker text without changing source line numbers.
Text in strings, raw strings and block comments is not an annotation.
Lowercase documentation comments such as `// @param` remain ordinary comments.
A marker name that is close to a Tachyon name (`// @GetRote`) is an error
that suggests the right name. Other `// @Word` comments, such as the
`// @Summary` and `// @Router` lines of swaggo/swag, stay ordinary comments.
A marker line holds only the marker and its one-line arguments.
The old `#[Name]` spelling is refused. Run `ty migrate fix --write` to convert
Go markers; the command leaves other languages' attributes alone.

- A behavior method has a named receiver, `BehaviorValue` parameters and the
  results `(BehaviorValue, error)`. `BehaviorValue` is `any` holding JSON data,
  so numbers are `float64`.
- A Setup type declares `Configure(execution *BehaviorExecution)
  (*BehaviorConfiguration, error)` and returns `NewBehaviorConfiguration()`
  with its adapters.
- Go has no generic methods, so `Behaviors.create`, `validate`, `properties`,
  `values` and `repository` are the functions `BehaviorsCreate[T]`,
  `BehaviorsValidate[T]`, `BehaviorsProperties[T]`, `BehaviorsValues[T]` and
  `BehaviorsRepository[T]`.
- A behavior error is `*BehaviorError{Code, Message, Path}`.
  `BehaviorErrorCode(err)` reads its code.
- A declaration without a body, such as a derived query or a Convert method, is a
  method with an empty body or a method of a Go `interface`.
- Go has no constructor. Tachyon builds a managed type as `&T{}` and assigns
  its injected fields.
- A test case is a method whose name starts with `Test` and returns `error`.
  The test type declares `behaviors *BehaviorContext`.

Complete example: [Go](../crates/tachyon-core/tests/fixtures/behavior/authored/go/server).

### C++

C++ markers are bare PascalCase standard attributes
([ADR 0099](adr/0099-cpp-is-a-tac-webassembly-service-language.md)). A
class marker goes after the class key, `class [[Service]] OrderService`,
because an attribute before `class` belongs to no entity. A member or
parameter marker leads its declaration: `[[Needs]] OrderStore& orders;`,
`[[OnEvent]] void placed(const PlacedOrder& order)`,
`Task<Order> find([[RequestParam]] int page);`. Several markers may share
one `[[A, B(1)]]`. Lowercase spellings such as `[[service]]`, a namespace
such as `[[tachyon::Service]]` and unknown PascalCase names fail with
`TY1371`; standard attributes such as `[[nodiscard]]` and vendor attributes
such as `[[gnu::cold]]` pass through. Tachyon removes its markers before
`em++` compiles, so no line number changes. C++ is a Tac language only for
now. Behavior markers go on synchronous service methods with a body, and a
C++ Store keeps its rows in IndexedDB. C++ has no `[[Setup]]` yet, so
`Behaviors::properties<T>(...)` and `Behaviors::values<T>(...)` bind an
explicit configuration snapshot. The rule table lists where each marker
works in C++.

## Verification surfaces

| Test | What it covers |
| --- | --- |
| `scripts/behavior-runtime-test.mjs` | Tests the adapter contracts on their own. |
| `crates/tachyon-core/tests/it/behavior_annotations.rs` | Runs the real Yon supervisor and Tac compiler. |
| `scripts/behavior-browser-test.mjs` | Runs emitted JS/TS pages and proves cancellation when a component is removed. |
| Polyglot compiler fixtures | Run annotated source through real native compilers and runtimes. |
| `scripts/behavior-jvm-test.mjs` and `scripts/behavior-native-runtime-test.mjs` | Also test native adapter contracts: strict JSON, authorization on hits and evictions, pagination, transport policy and lifecycle failures. |
| Extension fixtures | Cover the order of cache mutations, authorized structured and derived queries, explicit configuration selection, and pure validation boundaries. |

Limits of this testing:

- Swift execution requires macOS.
- The native Windows gate owns Windows UI execution and packaging. A
  cross-platform C# runtime test does not cover them.
- These tests do not qualify arbitrary third-party adapters, databases or
  deployment policies.

This guide records the implemented contracts and where to test them. It does
not claim that every release, external adapter or deployment has been
qualified.
