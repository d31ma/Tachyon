# Yon Controllers and Required Constructor Dependencies

Yon supports controller and provider annotations at compile time. It does not
need a NestJS or Spring runtime dependency. Controllers in `server/controllers/`
are route entry points. Workers keep their separate `Handler` protocol.

Managed types use strict role names. Tachyon looks for them only in captured
entry scopes of the same language. There is no runtime package scanner and no
global dependency container.

## An endpoint without handwritten injection

`RequestRoute("/products")` declares the controller's base path.
`GetRoute` on a method makes it answer GET requests.
Providers live in the shared folder of their realm:

- `server/services`
- `server/stores`
- `server/clients`
- `server/setups`
- `server/entities`
- `server/helpers`

Every controller uses these shared providers. Controllers never compose with
other controllers. No `yon.*` marker or `OPTIONS.schema.json` file is needed;
both old forms are refused (ADR 0048).

`server/stores/CatalogStore.js`:

```javascript
@Store
@NeedsAll
class CatalogStore {
  all() { return [{ id: 1, name: 'Anvil' }]; }
}
```

`server/services/ProductsService.js`:

```javascript
@Service
@NeedsAll
class ProductsService {
  catalogStore;
  list() { return this.catalogStore.all(); }
}
```

`server/controllers/ProductsController.js`:

```javascript
@RequestRoute('/products')
@Public
@NeedsAll
class ProductsController {
  productsService;
  @GetRoute
  getProducts() { return this.productsService.list(); }
}
```

A provider outside its shared role folder fails validation. Put services in
`server/services/`, not under a controller or URL folder. `ty migrate fix --write`
converts safe old routing forms and reports any remaining manual steps.

These modules have no authored `Needs` list, no constructor and no import between
them. Tachyon does the following:

1. It validates their original paths.
2. It assembles the captured sources.
3. It infers the required provider fields.
4. It lowers them into its existing explicit constructor-call plan.

A shared service is one class. Each Yon invocation still owns its own graph of
instances. The leaf store also uses an empty constructor marker.

A Store is a contract that names its Entity, and Tachyon writes each of its
queries (ADR 0022 amendment). A service can inject configured Yon resources
through DataSource, Redis or ApplicationEventPublisher fields. See [managed storage](MANAGED_STORAGE.md) and
[application events](APPLICATION_EVENTS.md).

Run source annotations through `ty`, not directly through the language runtime.

Provider roles are shared by both realms. Controller route annotations,
`StreamRoute` included, belong to Yon. `client/services/<Name>Service.js` is the behaviour that
a component view addresses by instance name (ADR 0030, ADR 0031).

You do not need a runtime annotation package. Ordinary dependencies must still
be available. The compiler recognizes NeedsAll on JS/TS managed
roles. The markers are global, so no example imports them (ADR 0046).

## Required-field inference

These rules apply to RequestRoute controllers, Service, HTTPClient and
Helper types:

- `NeedsAll()` selects direct fields that are not initialized
  and not static. It keeps declaration order. It generates constructor
  assignments and provider tokens.
- `NeedsAll` takes no field list (ADR 0056). A list such as
  `NeedsAll('right', 'left')` is refused. The class declares no constructor.
- Initialized fields and static state are not selected. Defaults are kept.
  This is the same rule as Tac (ADR 0057): a field without an initial value
  is required, and a field with one is plain state. Yon has no props, so
  every required field is a dependency.
- A required field and a `Needs` field are never null, so the build refuses
  one that is declared nullable with `TY1331` (ADR 0057 amendment): a
  TypeScript `store?:` or `store: T | undefined`, a Python `T | None` or
  `Optional[T]`, a PHP `?T $store`, a Kotlin, Dart or C# `T?` and a Rust
  `Option<Arc<T>>`. Write `store!: T`, `store: T`, `T $store`,
  `lateinit var store: T` (Kotlin), `late final T store;` (Dart) or
  `readonly T _store;` (C#). A field that may stay empty is plain state and
  has a starting value. `ty migrate fix --write` rewrites the simple cases.
- A field that holds an injected class uses the language's variable case
  (`TY1332`, ADR 0057 amendment): `orderStore` in JavaScript, TypeScript,
  Java, Kotlin, Dart and Go, `$orderStore` in PHP, `order_store` in Python,
  Ruby and Rust, `_orderStore` for a private or internal C# field and
  `OrderStore` for a C# property or a public or protected field. Only the
  case is checked, not the words. The name-based lookup of JavaScript
  (lowerCamel) and Ruby (snake_case) keeps working, because those are the
  same cases. The framework's `behaviors` field keeps its name. Go and Rust fields have no initializer, so every named Go
  field and every Rust field is required. A Ruby `#[NeedsAll]` class gets
  every `attr_reader`, `attr_accessor` or `attr_writer` field from the
  provider of the same PascalCase name (`attr_reader :order_store` needs
  `OrderStore`; `#[Needs]` with `#[Named("OrderStore")]` above the line names
  another one),
  and it writes no `initialize`.
- A required field without a provider stops the build. The message names the
  class, the field and the type, for example "Unresolved dependency
  MissingService: MoneyController needs field 'money' of type MissingService,
  but no provider MissingService is declared". The field is never `null` or
  `nil` at run time.
- Typed languages require an exact concrete provider type in the same
  assembled unit. Rust requires `Arc<Provider>` or `std::sync::Arc<Provider>`.
- JavaScript requires an exact lowerCamel match of the provider name:
  `MoneyService` maps to `moneyService`, and `HTTPClient` maps to `httpClient`.
  It does not guess from other field names. It does not choose between
  collisions.
- A selected dependency fails closed if it is missing, ambiguous, optional,
  generic, an interface or function-typed. This is bounded source analysis,
  not reflection.

Constructor generation needs a named body. A Rust leaf is
`#[NeedsAll] struct MoneyService {}`, with the marker on its
own line. Do not use a unit struct that ends in a semicolon. The compiler
generates `fn new`. Do not write another constructor next to the marker.
Go has no constructors. Tachyon builds a Go role as `&T{}` and assigns each
injected field (ADR 0045).
Constructors, inheritance and field shapes still follow the shared
constructor grammar.

Constructor inference does not:

- bind `Setting`;
- load configuration;
- inject primitive settings;
- instantiate DTOs (data transfer objects).

Separately, the reserved Yon provider tokens are `DataSource`, `Redis` and
`ApplicationEventPublisher` (ADR 0027, ADR 0031). JavaScript binds them by
field name. Every other language binds them by field type. Keep the explicit
ownership of `Behaviors.values`, `Behaviors.properties` and custom behavior
adapters.

## Language spellings

| Language | Required-constructor marker | Example dependency field |
| --- | --- | --- |
| JavaScript | `@NeedsAll` | `moneyService;` |
| TypeScript | `@NeedsAll` | `readonly money!: MoneyService;` |
| Python | `@needs_all()` | `money: MoneyService` |
| PHP | `#[NeedsAll]` | `public MoneyService $money;` |
| Java | `@NeedsAll` | `final MoneyService money;` |
| Kotlin | `@NeedsAll` | `val money: MoneyService` |
| C# | `[NeedsAll]` | `readonly MoneyService money;` |
| Rust | `#[NeedsAll]` | `money: Arc<MoneyService>,` |
| Dart | `@NeedsAll` | `final MoneyService money;` |
| Go | `// @NeedsAll` | `money *MoneyService` |

Each marker takes its own complete line, directly before the named
declaration. The website's Strict annotation layout guide has complete
entry-scoped role files for all ten languages.

Explicit `Needs` lists on a class and handwritten constructors are still supported when
you choose them instead of generated constructors. Their spellings are:

| Language | `Needs` list spelling |
| --- | --- |
| JS/TS | `@Needs(MoneyService)` |
| Python | `@needs(MoneyService)` |
| Java | `@Needs(MoneyService.class)` |
| Kotlin | `@Needs(MoneyService::class)` |
| C# | `[Needs(typeof(MoneyService))]` |
| PHP | `#[Needs(MoneyService::class)]` |
| Rust | `#[yon::needs(MoneyService)]` |
| Go | `// @yon::needs(MoneyService)` |

An explicit `Needs` list together with an explicit NeedsAll field
list keeps its earlier positional contract.

## Calling another language

A managed role may declare a method that another language answers. The
method has no body. Its configuration names a class in another Yon language
or a program under `server/helpers/`:

| Language | Gateway spelling |
| --- | --- |
| JS/TS | `@Gateway({"class":"server/services/ReportService.java"})` |
| Python | `@gateway({"command":["ruby","server/helpers/report.rb"]})` |
| Java, Kotlin, Dart | `@Gateway({...})` |
| C# | `[Gateway({...})]` |
| PHP, Rust | `#[Gateway({...})]` |
| Go | `// @Gateway({...})` |

The arguments and the result are bounded JSON in the language's JSON value
type. Only one hop is allowed. `server/gateways/` is reserved for the units
that Tachyon generates for class targets. See
[Gateways](BEHAVIOR_ANNOTATIONS.md#gateways) and
[ADR 0042](adr/0042-a-gateway-method-calls-another-language.md).

## HTTP methods and scope

Controller endpoints are instance methods marked `GetRoute`, `PostRoute`,
`PutRoute`, `PatchRoute` or `DeleteRoute`. Method names start with the same verb:
`getOrders`, `GetOrders` or `get_orders`. In Python they take self, in Rust
&self, and in Go a receiver. A method path follows the class path.

`FromPath`, `FromQuery`, `FromBody`, `FromHeader` and `FromCookie` bind
arguments. One unmarked argument may receive the request in the language's own
type (ADR 0062): a Fetch `Request` in JavaScript and TypeScript,
`urllib.request.Request` in Python, a `Net::HTTP` request in Ruby,
`com.sun.net.httpserver.HttpExchange` in Java and Kotlin, `HttpRequestMessage`
in C#, `http::Request<Vec<u8>>` in Rust, a `package:shelf` `Request` in Dart and
`*http.Request` in Go. PHP has no request object: a PHP method takes no request
parameter and reads `$_SERVER`, `$_GET`, `$_POST`, `$_COOKIE` and `php://input`.
`YonRequest` and `YonResponse` are removed and refused at build time. Declared types
validate input before the method runs; model return types validate its answer.
Model records reject undeclared fields, including nested records and list
elements. Required fields must be present, optional fields may be absent, and
the schema publishes `additionalProperties: false`. Explicitly untyped (`any`)
values remain open. The same rule applies to requests and responses. Fields
of a parent model (`extends`, a Python base class, a Go embedded struct) are
part of the record. When a parent is not in `server/models/`, or a field type
cannot be read, the record accepts extra fields instead of refusing them.
JavaScript uses JSDoc types. Input mismatches return 400; response mismatches
return 500. `OPTIONS` publishes the generated schema and `HEAD` uses GET.
Unsupported verbs on known paths return 405 with Allow. See
[ADR 0048](adr/0048-yon-routes-come-from-annotations.md).

Scopes:

- Controllers default to request scope. Other managed providers default to
  singleton scope. Both reuse one instance per token, but only inside the
  current invocation.
- Lifetime("transient") creates a new instance for each resolution.
- Shared diamonds keep their identity within a scope. Transient edges do not.
- Singleton-to-request capture, cycles and missing tokens fail before
  execution.
- No instance lasts across HTTP requests, worker ticks or independent
  invocations.

## Responses, streams and boundaries

All ten languages may return ordinary JSON-compatible data. The adapter
serializes it once, with status 200 and Content-Type application/json.
Strings stay JSON strings, even when they contain JSON text.

For a custom status, headers, redirects, raw bodies and relay answers, answer
with the language's own response: a Fetch `Response`, an
`(HTTPStatus, headers, body)` tuple in Python, a `[status, headers, body]` array
in Ruby, `http_response_code()`, `header()` and `echo` in PHP, a write on the
same `HttpExchange` (`getResponseHeaders()`, `sendResponseHeaders()`,
`getResponseBody()`) in Java and Kotlin, `HttpResponseMessage` in C#,
`http::Response<Vec<u8>>` in Rust, a `package:shelf` `Response` in Dart and
`*http.Response` in Go. Rust handlers build with Cargo and the `http` crate; Dart
handlers fetch `shelf`. An explicit Content-Type: text/html passes through
without interpolation or rendering. Tac stays client-rendered. Support for
ordinary asynchronous methods in compiled languages is separate from
generator streams.

Use standard JSON codecs where they exist:

- Java and Kotlin expose Json.parse and Json.stringify.
- C# can use System.Text.Json directly.
- Rust Json::parse and Json::stringify return Result and keep errors. Return
  Json or a supported Into<Json> value (maps, lists, strings, primitives,
  Option or unit).
- Go returns nothing, any value the runtime can encode as JSON, an `error`,
  or `(value, error)`. A `*http.Response` answers as it is. A plain Go
  `Handler` method may also leave out the request parameter.

Tachyon rejects malformed JSON. It never silently replaces it with an empty
object. See ADR 0026.

Streaming uses native generator syntax in JS/TS/Python/PHP/Kotlin/C#/F#/Dart/Ruby,
`coroutine.yield` in Lua, a range-over-function iterator in Go: the method
returns `iter.Seq[Model]`, or `iter.Seq2[Model, error]` when the stream can fail,
and an `Iterator` in Rust: the method returns `impl Iterator<Item = Model>`, or
`impl Iterator<Item = Result<Model, E>>` when the stream can fail.
`StreamRoute` marks the method: it answers GET requests with server-sent events,
and the method name starts with `stream` (`streamTicks`, `StreamTicks`,
`stream_ticks`). A generator under `GetRoute` is refused, and so is the retired
`Stream`; `ty migrate fix --write` converts `GetRoute` with `Stream`. The server
sends `event: end` when the method finishes (ADR 0058). Python supports
synchronous generators, not async generators. Java and Groovy stay single-response. The existing limits for
deadline, output, cancellation and process-tree cleanup apply. Helper
grants no extra process permissions. Outside programs use explicit relay
calls or a Gateway command.

`SocketRoute("/chat")` marks a controller class that keeps a WebSocket open
(ADR 0060). It works like Jakarta's `@ServerEndpoint`: Tachyon starts one
handler process for each connection, so the class's fields hold that
connection's state. `OnOpen(session)`, `OnMessage(session, message)`,
`OnClose(session)` and `OnError(session, error)` mark its methods; only
`OnMessage` is required, and each marker is used at most once. The message
parameter has a model type, and Tachyon checks every message against it
before the method runs. `session.send(message)` answers,
`session.close()` closes, and `session.openSessions()` lists every open
connection of the same path. Guards fail closed: JSON text only (1003 for
binary, 1007 for a message that does not fit), 64 KiB per message (1009),
32 open sockets for the server (503), five idle minutes (1001), one hour per
connection, the page's own origin only (403), and 426 for a plain request.
When OnOpen or OnMessage fails, OnError runs and the socket stays open, as in
Jakarta; the error text goes to the server log, not to the browser.
Every Yon language holds a socket, because its runner or prelude reads the
socket frames while the class runs. Each language names the session methods in
its own style: `open_sessions()` in Python, Ruby and Rust, `OpenSessions()` in
Go and C#, Jakarta's `getOpenSessions()` in Java, and `openSessions()` in
JavaScript, TypeScript, Kotlin, PHP and Dart. In Go, `session.Request()`
returns the request that opened the socket as a `*http.Request`, with the
query string and headers. A method that throws, panics or
returns an error (Go, Rust) counts as a failure. ADR 0060 lists each
language's marker and session shape.

A relay program's deadline is the time left in the request, shared by every
relay in that request. The handler sees that time as `request.deadline_ms`:
the milliseconds left when the supervisor wrote the request, minus the
cancellation grace. A relay program gets the baseline environment, the
explicit allowlist, `YON_HANDLER_PROTOCOL` and its handler's `NO_PROXY`
values, never `YON_APPLICATION_JSON` or a `YON_RESOURCE_*` value. The
baseline differs by platform (see docs/THREAT_MODEL.md). Removing these
values prevents accidental inheritance; it is not a sandbox, because a
program that runs as the same user can still read its parent's environment
and the project's files. Every relay failure reaches the handler as HTTP 502
with the one fixed message `Component invocation failed.`

## Who may call a route

Every route says who may call it (ADR 0068). Tachyon refuses every route by
default, like Spring Security.

- `@Public`: anyone may call the route.
- `@SignedIn`: any signed-in user may call it.
- `@Allow(Admin, Editor)`: only a signed-in user with one of these roles may
  call it. A role is a type, an empty class in `server/roles/<Role>.<ext>`:
  `Admin.class` in Java, `Admin::class` in Kotlin and PHP, `typeof(Admin)` in
  C#, the bare name elsewhere. Rust writes `#[Allow(Admin)]`; `#[allow(...)]`
  stays a Rust attribute.

Write the rule on the controller class, on a route method, or on both; the
method wins. A `@SocketRoute` class takes the rule on the class. A route
without a rule stops the build with `TY1322`, and no setting turns this off.

One class marked `@IdentityProvider`, in
`server/controllers/<Name>IdentityProvider.<ext>`, says who is signed in. Its
method `identify` (`Identify` in C# and Go) takes the native request (or one
`@FromHeader`, `@FromCookie` or `@FromQuery` value) and returns the user and
their role names, `{"user": "ada", "roles": ["Admin"]}`, or no value. The
server calls it before every `@SignedIn` or `@Allow` route, whatever the
language of the route, and answers 401 without an identity, 403 without one
of the roles and 500 when `identify` fails. The route method does not run in
these cases. A guarded route without an identity provider, or two identity
providers, stops the build with `TY1323`.

## Error handlers

An error handler turns a typed controller error into a public answer. You
write the answer once, not in every route (ADR 0051).

- An error type is a native error class or type. It lives in
  `server/errors/<Type>.<ext>` and has no Tachyon annotation.
- An `@ErrorHandler` class lives in
  `server/controllers/<Name>ErrorHandler.<ext>`. A project may have several.
  Every controller of the same language uses them.
- `@HandlesError` goes on an instance method and takes no argument. The
  declared type of the first parameter is the error that the method handles.
  JavaScript declares it with JSDoc: `@param {OrderNotFound} error`. An
  optional second parameter receives the request. The method name is free.
- `@ResponseStatus` on the method sets a status from 400 to 599. The default
  is 500.
- The method returns a model or a response. Tachyon checks the answer like a
  controller answer. `/api.json` and `OPTIONS` list each error type, its
  status and its answer shape.

`server/errors/OrderNotFound.js`:

```javascript
class OrderNotFound extends Error {}
```

`server/controllers/OrdersErrorHandler.js`:

```javascript
@ErrorHandler
class OrdersErrorHandler {
  /**
   * @param {OrderNotFound} error
   */
  @HandlesError
  @ResponseStatus(404)
  missing(error) {
    return { code: 'order-not-found' }
  }
}
```

`server/controllers/OrdersController.js`:

```javascript
@RequestRoute('/orders')
@Public
class OrdersController {
  @GetRoute
  getOrder() {
    throw new OrderNotFound('order 42')
  }
}
```

`GET /orders` answers 404 with `{"code":"order-not-found"}`.

How Tachyon chooses a method:

- The nearest type wins. A handler for a subclass runs before a handler for
  its base class, whatever the file or declaration order.
- Go uses `errors.As`, so a wrapped error matches too. A concrete error type
  wins over a catch-all `error` parameter.
- Rust matches the error type of the controller method's `Result`. The
  parameter borrows it, such as `error: &OrderNotFound`. A
  `Box<dyn Error>` result matches through the boxed error.
- Two methods for the same error type are refused, also across handler files.

Safety:

- An error that no method handles keeps the fixed 500 answer.
- An error thrown inside a handler method also gets the fixed 500 answer.
  Tachyon does not call a handler again for it.
- Nothing from the error, such as its message or stack, reaches the client.
  Only the value that the handler returns does.
- The order is: controller, then the error handler, then middleware `after`.
- Error handlers cover controller methods and the services they call. They
  do not cover middleware, workers, provider construction or Tac code.

Refused forms, with a hint to the right one: `ControllerAdvice`,
`RestControllerAdvice`, `ExceptionHandler`, `ExceptionFilter` and
`UseFilters`; `@HandlesError("Name")` with an argument; a missing,
`any`/`Object` or non-error parameter type; a static method; more than two
parameters; a route annotation on a handler method.

## Validation and current limits

- Each managed type must have the required role suffix, the exact file name
  and the shared realm folder. Controllers live in `server/controllers/`.
- Discovery orders definition dependencies deterministically. All tokens
  resolve in that captured entry scope of the same language.
- Included modules do not resolve relative imports recursively. Ordinary
  imports in entry points keep their supported language behavior.
- Managed types are top-level, in the default package/namespace. Tachyon does
  not support any of these: nested managed classes, registration inheritance,
  named DI (dependency injection) namespaces, interface tokens, lazy proxies,
  factories or global scanning. Routing parameter annotations do not select providers.
- Limits per invocation:

  | Limit | Value |
  | --- | --- |
  | Providers | 128 |
  | Dependencies per provider | 32 |
  | Verbs | seven |
  | Assembled source | 1 MiB |
  | Planned constructions | 4,096 |
  | Identifier length | 128 bytes, ASCII only |

- Invalid graphs and inferred dependencies report TY2015. An invalid role
  layout reports TY1310. The compiler does not call providers to infer
  dependencies.
- Handler and generated Yon-prefixed names are reserved in annotated units.
  Do not mix a plain Handler and an annotated controller in the same unit.
- Tachyon does not add guards, interceptors, filters or implicit module
  visibility.

Unmanaged behavior types keep their explicit construction contract, including
`Behaviors.construct(Type, values)`. Browser Tac Client graphs use generated
owner construction. Native Tac page construction and explicit context disposal
do not change. Tachyon adds no global Tac dependency container.

See [project structure](BEHAVIOR_PROJECT_STRUCTURE.md),
[shared behavior APIs](BEHAVIOR_ANNOTATIONS.md),
[ADR 0031](adr/0031-realms-mirror-each-other.md),
[ADR 0021](adr/0021-yon-annotation-frontend.md),
[ADR 0024](adr/0024-strict-annotation-modules.md),
[ADR 0025](adr/0025-required-constructor-inference.md) and
[ADR 0027](adr/0027-scoped-discovery-and-managed-resources.md).
