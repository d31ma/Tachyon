# ADR 0099: C++ Is a Tac WebAssembly Service Language

- Status: Accepted (Tac slice; the Yon slice is pending)
- Date: 2026-10-04
- Builds on: [ADR 0035](0035-rust-services-reach-the-web-as-webassembly.md)
  (the linear-memory page bridge),
  [ADR 0046](0046-tachyon-annotations-are-global.md) (markers need no import),
  [ADR 0056](0056-every-annotation-checks-its-target.md) (the shared rule
  table), [ADR 0065](0065-webassembly-services-reach-the-browser-through-tachyon.md)
  (WebAssembly services reach the browser through Tachyon),
  [ADR 0067](0067-a-class-file-is-named-after-its-class.md) (one class per file, named after it)
  and [ADR 0081](0081-release-webassembly-uses-the-smallest-compiler-per-language.md)
  (release WebAssembly)
- Exception to: [ADR 0062](0062-yon-uses-native-http-types.md)
  (C++ has no standard HTTP or JSON types, so Tachyon ships C#-shaped ones)

## Context

C++ has the largest audience of the languages that Tachyon did not support.
Emscripten compiles it to WebAssembly with linear memory, so it runs in every
browser, like Rust, Go and Swift. Tachyon already pins Emscripten 6.0.10 for
the mruby and Lua runtimes (ADR 0049, ADR 0085).

C++ has no runtime reflection, no standard JSON library and no standard HTTP
library. The owner decided (2026-09-27) that Tachyon uses no third-party
library and no foreign function interface for them: Tachyon ships its own
header-only types, shaped like .NET's `System.Text.Json.Nodes` and
`System.Net.Http`, and generates the model codecs at build time.

## Decision

### Files

- A C++ Tac file is a header-only `.hpp` file with one class (or one
  `enum class` for a model), named after the file (ADR 0067):
  `client/services/<Name>Service.hpp`, `client/models/<Name>.hpp` and
  `client/tests/<Name>Test.hpp`. There is no `.cpp` file: a header-only file
  is the simplest layout that an editor understands on its own.
- An authored file may start with `#pragma once`, standard `#include <...>`
  lines and `#include "..."` lines that name other authored files, so that an
  editor finds the types. Tachyon composes one translation unit per page
  program, so it removes `#pragma once`, the quoted includes and
  `#include <tachyon/...>`, and keeps the standard includes.
- A C++ file holds no namespace, no template class, no free function and no
  file-level variable. A service declares no nested type: models go in
  `client/models`. These forms fail with `TY1371`.

### Markers

- Markers are bare PascalCase standard attributes, like every other Tachyon
  language (ADR 0046): `class [[Service]] OrderService { ... };`,
  `[[Needs]] OrderStore& orders;`, `[[OnEvent]] void placed(const PlacedOrder& order)`.
- The C++ grammar decides the places. A class attribute goes after the class
  key (`class [[Service]] Name`); an attribute before `class` belongs to no
  class and fails with `TY1371`. Member and parameter attributes lead the
  declaration. Several markers may share one `[[A, B(1)]]` or follow each
  other (`[[OnEvent]] [[Order(10)]]`).
- PascalCase keeps the markers clear of the names that the C++ standard
  reserves for its own attributes (lowercase). So Tachyon refuses lowercase
  spellings such as `[[service]]` ("write [[Service]]"), a namespace such as
  `[[tachyon::Service]]`, and an unknown PascalCase name, with the closest
  marker as a suggestion (`TY1371`). Standard attributes (`[[nodiscard]]`,
  `[[maybe_unused]]`, ...) and vendor attributes (`[[gnu::...]]`) pass
  through.
- The rule table lists C++ (`hpp`) as a Tac language and is the one place
  that decides where each marker works. Today C++ reaches every Tac marker
  except `Setup`, `WhenProfile`, `WhenSetting`, `UseProfiles`,
  `TestSettings`, `StreamRequest` and `SocketRequest`. Those fail with the
  rule's own code (most often `TY1308`), and the message names the languages
  where they work. The same shared checker (`PlacedMarker`) reports
  a wrong target, missing or extra arguments, a missing partner, a clash and a
  class requirement, in the C++ spelling. A `[[Needs]]` field whose type is
  neither a C++ service nor a C++ HTTP client fails with `TY1372`.
- Tachyon removes the markers before it compiles, so the compiler never sees
  them. Editors warn about unknown attributes; `ty init` writes a `.clangd`
  file with `-std=c++20 -Wno-unknown-attributes` for C++ projects.
- `TY1317` refuses direct browser interop in C++ as in the other languages:
  `emscripten` headers and `emscripten::val`, `EM_JS`, `EM_ASM`,
  `EMSCRIPTEN_KEEPALIVE` and the `import_module`, `import_name` and
  `export_name` attributes.

### Services

- Public non-static fields are page fields; public non-static methods are
  page methods. A `const` field is read-only. Page names are the C++ names in
  lowerCamel case. Overloaded page methods fail with `TY1371`, because a page
  call names a method only by its name.
- `[[Needs]] T& name;` injects another C++ service of the same page program.
  Tachyon lowers it to `T& name = ::tachyon::TacNeeds::of<T>();`, so the
  class keeps its default constructor. Every C++ service is a program-wide
  singleton (Spring's default scope): the page and every `[[Needs]]` field see
  the same object. Tachyon compiles the needed services before the services
  that need them, and refuses a circle and a `[[Needs]]` type that is not a
  C++ service in `client/services`.
- `class [[Service, Lifetime({"value":"prototype"})]] TallyService` makes a
  new object for each component that uses the service and for each
  `[[Needs]]` field that names it; `"singleton"` is the default.
  `"application"` fails with `TY1372`, because a C++ page program has no
  native host to keep an application object. `[[Named("FastGreeterService")]]`
  beside `[[Needs]]` chooses the class that fills a base-class field.
- A `[[NeedsAll]]` class declares no constructor. Each reference field
  without an initial value is a `[[Needs]]` field, and each other field
  without an initial value is a component property: the page passes it once
  per component, so a `[[NeedsAll]]` service is a prototype unless
  `[[Lifetime]]` says otherwise. A property is never empty, so a
  `std::optional` or pointer property fails with `TY1331`.
- `[[LocalState]] int count = 0;` and `[[SessionState]]` keep a public page
  field in `localStorage` or `sessionStorage`, as in the other languages.
- `[[Description("Keeps the score")]]` gives a class a one-line description
  for tools; on a model it needs another Tachyon marker beside it.
- `ApplicationEventPublisher applicationEventPublisher;` publishes a model
  with `publishEvent(model, retain)`. `[[OnEvent]]` takes the event model as
  its one parameter; `{"replay", "condition", "id"}` options, `[[Order(n)]]`
  and `[[InBackground]]` work as in the other languages, and a returned model
  is published again. `[[OnStart]]` and `[[OnStop]]` run on activation and
  disposal. Listener and hook methods may be private: Tachyon adds a `friend`
  declaration for the generated binding class.

### JSON and models

- The prelude is a few header-only files that Tachyon writes into every
  composed program: `tachyon/json.hpp`, `tachyon/http.hpp`,
  `tachyon/task.hpp` and the page bridge.
- `tachyon::json` follows `System.Text.Json.Nodes`: `JsonNode`,
  `JsonObject`, `JsonArray`, `JsonValue`, `JsonValueKind`, `JsonException`
  and `JsonSerializer`. The API uses C++ naming: `JsonNode::parse`,
  `toJsonString`, `getValue<T>`, `getValueKind`, indexers, `add`, `remove`,
  `containsKey`, `count`, `deepClone`, `deepEquals`;
  `JsonSerializer::serialize`, `serializeToNode` and `deserialize<T>`.
- The parser is strict RFC 8259 with the limits of the other preludes:
  1 MiB of text and 64 levels of nesting. Numbers are doubles; an integer
  read checks that the number is whole and fits the type.
- C++ has no reflection, so Tachyon generates a `JsonCodec<Model>` for every
  model in `client/models` at build time (the `JsonSerializer` equivalent).
  A missing key keeps the field's C++ default, like `System.Text.Json`; a value
  of the wrong kind throws `JsonException`. An `enum class` model crosses the
  bridge as its enumerator name. `bool`, numbers, `std::string`,
  `std::vector`, `std::optional`, `std::map<std::string, T>`, `JsonNode` and
  models nest freely.

### Build and bridge

- Tachyon composes one `services.cpp` per page program: its prelude, the
  models and their codecs, the services in dependency order, one generated
  binding class per service and the service table. `#line` directives point
  compiler errors at the authored file and line.
- `em++` compiles it with `-std=c++20 -fwasm-exceptions -sSTANDALONE_WASM
  --no-entry -sMALLOC=emmalloc -sALLOW_MEMORY_GROWTH=1`, `-O1` in development
  and `-Oz` (which runs Binaryen) in release. The build is cached by compiler
  version, flags and program.
- The module exports `tac_wasm_alloc`, `tac_wasm_dealloc`,
  `tac_native_invoke`, `tac_native_free` and `tac_http_settle`, and imports
  `tac_host_emit`, `tac_host_storage`, `tac_host_web`, `tac_host_epoch_ms`,
  `tac_host_http_start`, `tac_host_http_cancel` and `tac_host_task_done`
  from `tachyon`:
  the same linear-memory protocol as Rust and Swift (`init`, `get`, `set`,
  `call`, `event`, `dispose`). A small WASI shim prints standard output and
  standard error to the console.
- Exceptions are on. Every request runs inside `try`, so a C++ exception
  (`JsonException`, `std::runtime_error`, ...) becomes the request's error
  answer and never reaches the page as a trap.
- C++ links into no native host. Like Go and Java, a C++ service runs as
  WebAssembly in the browser and in the web view of every native target.
  A native capability that needs a linked companion does not accept C++.

### Sync and async

- A page method is synchronous unless it returns `Task<T>` (`tachyon::Task`,
  a C++20 coroutine type that Tachyon ships). A `Task` method is listed in
  `asyncMethods`; the page awaits it like a Rust `async` method. The method
  starts at once and returns a task number; when its coroutine finishes,
  Tachyon reports the answer with `tac_host_task_done`. The page side reuses
  the Rust task bridge unchanged (`tac_host_http_start`, `tac_host_http_cancel`,
  `tac_http_settle`, `tac_host_task_done`).
- An exception that leaves a task becomes the task's error answer; an
  `HttpRequestException` also carries its Tachyon code (`HTTP_STATUS`,
  `HTTP_NETWORK`, `CANCELLED`, `DEADLINE`, ...) as the error's `code`.

### HTTP

- `tachyon::http` follows `System.Net.Http`: `HttpClient` (`sendAsync`,
  `getAsync`, `getFromJsonAsync<T>`, `postAsJsonAsync`), `HttpRequestMessage`
  (`method`, `requestUri`, `headers`, `content`), `HttpResponseMessage`
  (`statusCode`, `reasonPhrase`, `headers`, `content`,
  `isSuccessStatusCode`, `ensureSuccessStatusCode`), `HttpMethod`
  (`HttpMethod::Get`, ...), `HttpStatusCode`, `HttpContent`
  (`readAsString`, `readFromJson<T>`), `StringContent`, `JsonContent` and
  `HttpRequestException`. Methods return `Task<...>` (C# returns `Task<...>`
  too); `readAsString` is synchronous because the body has already arrived.
- Every request goes through the page's same-origin browser transport (the
  one Rust and C# use): JSON bodies, `credentials: omit`, no redirects, a
  30-second deadline and a 1 MiB answer limit. A status outside 2xx fails
  with `HttpRequestException` code `HTTP_STATUS`.
- A declarative client is a class in `client/remotes`:
  `class [[HTTPClient("/api")]] OrderClient { public: [[GetRequest("/orders/{id}")]] Task<Order> find(int id); };`.
  Its methods have no body; Tachyon writes them. `{name}` in the path takes
  the parameter of that name, `[[RequestBody]]` sends a parameter as the JSON
  body, `[[RequestParam]]` adds it to the query and
  `[[RequestHeader("X-Name")]]` sends it as a header. A service asks for the
  client with `[[Needs]] OrderClient& orders;`.
- Each method of a declarative client sends its request with the client's
  adapter name: the class name without `Client`, in lowerCamel case
  (`GreetingClient` sends with `greeting`), as `@HTTPClient` names its
  adapter in the other languages. A plain `HttpClient` sends with `http`.
  The browser transport does not read the name; a test unit uses it to find
  the answer (see Test units).

### Validation and converters

- `struct [[Checked]] SignUp` with `[[Required]]` (on a `std::optional` or
  pointer field), `[[NotEmpty]]` (text), `[[Length({"min":1,"max":80})]]`
  (text, `std::vector`, `std::map`, `std::set`), `[[Min(0)]]` and `[[Max(9)]]`
  (numbers) gets a generated `TacChecks<SignUp>`. `[[Check]]` on a service
  or HTTP client parameter of a `[[Checked]]` model runs it first, so a bad
  argument fails with the `validation` code and the field path
  (`name.notBlank`) before the method runs or the request leaves.
- A `[[Converter]]` class in `client/converters` declares methods without a
  body, such as `Summary toSummary(const Product& product);`. Tachyon writes
  each one: it copies same-named fields (nested models, lists and maps of
  models too) and follows the `[[Convert({"fields":[...]})]]` plan for the
  rest, with the shared limits (32 entries, source paths of 8 names, an
  absent path is null). A `JsonNode` to `JsonNode` method needs a plan. A
  target field with no source, a source that may be empty for a field that
  is not `std::optional`, and a value of another kind fail the build and
  name the field. A service gets the converter with `[[Needs]]`.

### Helpers, topics and behaviors

- A `[[Helper]]` class in `client/helpers` is a plain C++ class that a
  service gets with `[[Needs]]`, like another service.
- A `[[Topic]]` model in `client/models` gives each page listener of that
  model its topic stream, as in the other WebAssembly Tac languages.
- `[[Cached]]`, `[[UpdateCache]]`, `[[ClearCache]]` (with `[[CacheKey]]`
  parameters and `[[CacheSettings]]` on the class), `[[Retry]]`,
  `[[MaxAtOnce]]`, `[[AllOrNothing]]` and a `[[PageDefaults]]` parameter go on
  a synchronous instance method with a body that returns a value or `void`.
  Tachyon wraps the body: the shared planner checks the options, and a small
  C++ runtime runs the policy and keeps cached values in the page's built-in
  cache through the storage host. A `Task` method and an `[[HTTPClient]]`
  method cannot carry these markers yet, because the policies run around a
  synchronous body.

### Stores

- `struct [[Entity({"adapter":"indexeddb"}), Table("notes")]] NoteEntity` in
  `client/entities` (or `client/models`) has exactly one public `[[Id]]`
  field. `class [[Store(NoteEntity)]] NoteStore : public
  tachyon::Repository<NoteEntity, std::string>` in `client/stores` declares
  query methods without a body, such as
  `std::vector<NoteEntity> findByText(const std::string& text);`, and
  `[[Query(...)]]` for the explicit form. The shared query planner reads the
  method names, so a C++ Store builds the same plans as the other languages.
  Tachyon writes each method, and the `Repository` base gives `findById`,
  `findAll`, `deleteById` and `save`. The return type
  must fit the query: `std::vector<T>` for find, `std::optional<T>` for
  findOne, `bool` for exists and a whole number for count and delete.
- The rows live in the browser's IndexedDB through the page's entity mirror.
  `[[AllOrNothing]]` on a service method collects the writes and commits them
  in one transaction, or rolls them back when the method throws.
- Only the `"indexeddb"` adapter builds. Another adapter needs a C++
  `[[Setup]]`, which is still pending, so it fails with `TY1372`. A project
  has one Store for each Entity, and a Store method has one name.

### Settings

- `struct [[Settings({"prefix":"TAC_SHOP_"})]] ShopSettings` binds its public
  fields from `TAC_` values; `[[Min]]` and `[[Max]]` bound a number field and
  an initializer is its default. `[[Setting("${tac.app.title:Shop}")]]` binds
  one field of a model from the application configuration.
- C++ has no `[[Setup]]` yet, so a service binds an explicit snapshot:
  `Behaviors::properties<ShopSettings>(values)` from a
  `std::map<std::string, std::string>` or from the application `JsonNode`,
  and `Behaviors::values<Model>(application)`. The shared rules check the
  plan at build time and the same limits at run time; a bad value fails with
  the `configuration-properties` code.

### Test units

- A test unit is `client/tests/<Name>Test.hpp` with one class marked
  `[[Test(<Name>TestSetup)]]`; the TestSetup lives beside it. `ty test`
  composes one program with the test prelude, the models, every service and
  client, the TestSetup, the unit and a generated `main`, compiles it with
  clang++ (or `TAC_CPP_TEST_COMPILER`) and runs the binary. It needs no
  Emscripten. Each public method whose name starts with `test` and that has
  no parameter is a case; a method with `[[ValueSource]]` takes one.
- A case may return `Task<void>`. The test transport answers at once, so the
  task has finished when the case returns; a task that still waits fails its
  case.
- A static `configure()` method of the TestSetup returns `BehaviorAdapters`.
  `adapters.http["greeting"]` is a `BehaviorHTTPAdapter`, a function that
  takes the `tachyon::http::HttpRequestMessage` and returns the answer body
  as a `JsonNode`, for example
  `tachyon::json::JsonSerializer::serializeToNode(Greeting{"Hi"})`. Tachyon calls
  `configure()` again for each case, so a case never sees another case's
  answers. A request whose adapter has no answer fails with an
  `HttpRequestException` whose code is `HTTP_ADAPTER`; an answer function may
  throw `HttpRequestException("HTTP_STATUS")` to test a failure. No request
  leaves a test unit.
- `[[Fake]] GreetingClient greetingClient;` is a field of the test class that
  holds the stand-in object itself: no reference, no pointer, no `static`.
  Its type is a C++ service or HTTP client of the project (`TY1372`
  otherwise). Tachyon writes a member `construct<T>()` into the test class:
  while it makes a `T`, every `[[Needs]]` field of a Fake's type gets the
  Fake field's object instead of the program-wide one. A shared service that
  is first made during `construct` keeps the shared objects, so a Fake never
  leaks into a later case. `CounterService counter;` still makes the service
  with the shared objects. A `[[Fake]]` field in the TestSetup class is
  refused, because no `construct` reads it there.
- `[[FreshStart]]` on the test class or its TestSetup makes every shared
  service again before each case, so no case sees another case's state.
- `[[ValueSource([1, 2, 3])]]` on a case with one parameter runs it once for
  each input, named `testName[1]`, `testName[2]` and so on (ADR 0080). The
  list is JSON with 1 to 256 numbers, strings or booleans; Tachyon writes
  each input as a C++ literal into the call.
- Executed-method coverage reads C++ services: the public non-static methods
  with a body are the methods that a case must call. In a test unit, Tachyon
  adds `BehaviorCalls::record("CounterService.increment");` at the start of
  each of them, and each case reports the methods it ran. A call on a Fake
  client does not count, because a client has no written methods. The call
  is recorded only at run time, so a `constexpr` method still compiles; a
  call that the compiler evaluates does not count.

### Size

- Measured with em++ 6.0.10 in a release build (`-Oz`, emmalloc):
  a page with one small service (the `ty init` greeting) loads 74.9 KB of
  WebAssembly (29.2 KB with Brotli) and a 14.6 KB engine module (3.7 KB with
  Brotli). The browser-gate fixture, with four services, an HTTP client and
  two models, is 122.7 KB. The budget for a one-service page is 100 KB of
  WebAssembly (40 KB with Brotli).
- `std::to_chars` for doubles would add about 100 KB, so the JSON writer
  prints numbers with the shortest of `%.15g`, `%.16g` and `%.17g` that
  reads back exactly.

## Pending

- Yon C++ handlers (controllers, routes, middleware, error handlers, sockets,
  relays, Yon test units), compiled per handler with clang++.
- A C++ `[[Setup]]` with a behavior context, and with it `[[WhenProfile]]`,
  `[[WhenSetting]]`, `[[UseProfiles]]` and `[[TestSettings]]`, Store adapters
  other than `"indexeddb"`, and the behavior markers on `Task` and
  `[[HTTPClient]]` methods. The rule table refuses these for C++ until they
  land.
- The curated web APIs (Clock, Preferences, ...) for C++ `[[Needs]]` fields.
- `[[StreamRequest]]` and `[[SocketRequest]]` stay JavaScript, TypeScript and
  Go only, as in the other WebAssembly languages; a C++ service receives the
  messages with `[[OnEvent]]`.
- Lifetime `"application"` stays refused: a C++ service runs in the page and
  has no native host.
- `ty migrate` rewriting the legacy C++ Yon handlers.

## Consequences

- C++ teams write Tac services in their own language with Tachyon's usual
  markers, and editors understand the files.
- Tachyon owns and maintains a JSON implementation and a model-codec
  generator for C++.
- Pages that use a C++ service download one more WebAssembly module, about
  the size of a Swift or Go one without their runtimes.
