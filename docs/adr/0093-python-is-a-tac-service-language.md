# ADR 0093: Python Is a Tac Service Language

- Status: Accepted
- Date: 2026-10-03
- Builds on: [ADR 0030](0030-components-use-services.md) (page singletons
  and prototype services),
  [ADR 0043](0043-models-hold-data-types.md) (models),
  [ADR 0084](0084-webassembly-services-stay-on-the-page-thread-until-dotnet-needs-a-worker.md)
  (services run on the page thread) and
  [ADR 0091](0091-role-folders-allow-nesting-and-require-explicit-registration.md)
  (role markers)
- Amends: [ADR 0031](0031-realms-mirror-each-other.md) (Python was a Yon
  language only)

## Context

Python was a Yon language: Yon runs it with native CPython. Tac had no Python
backend, so a team that writes its server in Python had to write its page
services in another language. Ruby and Lua reached Tac through an interpreter
compiled to WebAssembly (ADR 0049, ADR 0085). Pyodide is a maintained build
of CPython for the browser, with a module loader and conversions between
JavaScript and Python values. The full Pyodide distribution is over 200 MB,
and by default it can fetch missing packages from the network, so Tachyon has
to pin and limit what it loads.

## Decision

### Services

- A Tac service may be a Python class in `client/services/<Name>Service.py`,
  marked with `@service` like a Yon Python service. Helpers in
  `client/helpers/` and models in `client/models/` may be Python too.
- The page reads and writes public fields and calls public methods through
  the existing Tac service protocol. Fields and method results cross as JSON
  through explicit codecs; models from `client/models/` are typed on both
  sides. A read-only field cannot be written from the page.
- `Needs` and `NeedsAll` fields with explicit types build the service's
  dependencies, including Python Helpers. `Named` selects one declared
  provider. Only a `NeedsAll` field of a Service can be a page property. A
  Helper that its caller creates itself must be created in the caller's own
  code. Singleton and prototype lifetimes follow ADR 0030. A dependency graph stops at 4096 acquisitions and 32
  levels, and a cycle is refused.
- `@on_start` and `@on_stop` run synchronously when the owner starts and is
  disposed. Disposal releases the owner's Python objects and the JavaScript
  proxies that point at them.

### Runtime

- The project opts in with `"python": {"runtime": "pyodide-314.0.7",
  "profile": "offline"}` under `tac` in `client/manifest.json`. Any other
  runtime or profile is refused.
- The runtime is Pyodide 314.0.7 (CPython 3.14.2, ABI 2026_0). Its five core
  files (`pyodide.mjs`, `pyodide.asm.mjs`, `pyodide.asm.wasm`,
  `python_stdlib.zip`, `pyodide-lock.json`) are pinned in the compiler by
  byte size and SHA-256. The developer puts them in `python-web/314.0.7/`
  in the Tachyon cache. `ty` never downloads or installs them, and
  `ty doctor` reports whether they are present and match.
- A build copies the runtime into
  `.tachyon/services/python/314.0.7/` only when a route reaches a Python
  service. The files go through the normal build steps: hashes,
  pre-compression, cache keys, route budgets and the service worker, so an
  installed app works offline. A route with a Python service gets a
  512 KiB script budget unless the project sets its own; the WebAssembly
  file may be up to 10 MiB and the other runtime files up to 4 MiB each.
- (Amended 2026-10-16) The build also writes `THIRD_PARTY_NOTICES.txt`
  beside the runtime in `.tachyon/services/python/314.0.7/`, as Ruby and Lua
  do. It holds the licenses of Pyodide (MPL-2.0, with the address of its
  source code), CPython 3.14.2 (PSF License) and the libraries linked into
  `pyodide.asm.wasm`: hiwire, HACL\*, libffi, zlib, bzip2, Zstandard,
  liblzma, SQLite, Emscripten, musl, compiler-rt, libc++ and libc++abi. The
  text is kept in `runtimes/python-web/THIRD_PARTY_NOTICES.txt`, taken from
  the upstream sources of the pinned release.
- Only the standard library in the pinned archive and the project's own
  modules are available. Pyodide loads its index, lock file and standard
  library from the copied files, and Tachyon installs no packages.

### Limits

These fail at build time with `TY1605`, which names the file:

- `async def` methods and asynchronous `@on_start` or `@on_stop`. The bridge
  is synchronous; the asynchronous settlement contract of other Tac
  languages is not connected to Python yet.
- Streams and browser API resources (see the amendment below for events).
- Constructor arguments that are not typed `Needs` or `NeedsAll` fields, and
  `Needs` fields that name no Python provider.
- A missing, changed or incomplete runtime.
- (Amended 2026-10-04) A browser API in the service's code, such as
  `Clock()`, `SecureRandom` or `Geolocation`. The runtime has no browser API
  adapter, so the use failed only at run time before. Use the API in a
  JavaScript, TypeScript or compiled service and pass the value to the Python
  service as an argument.

(Amended 2026-10-04) The Python runtime has no behavior or storage adapter.
The annotation rule table (ADR 0056) therefore leaves Python out of
`LocalState`, `SessionState`, `CacheSettings`, `Cached`, `CacheKey`,
`ClearCache`, `UpdateCache`, `PageDefaults`, `Retry`, `MaxAtOnce` and
`AllOrNothing` in Tac. A Python Tac service with one of them fails once, with
`TY1308` "... is not available in Python Tac code; it works in ...". Before, a
saved field built and was never saved, and a synchronous `@retry` method
failed with "must use async def" while an `async def` failed with `TY1605`.
`OnEvent`, `Order` and `InBackground` stay available: see the events and
topics amendment below.

Python Tac code cannot call the server through a client either. The rule
table leaves Python out of `HTTPClient` and its request annotations
(`GetRequest`, `PostRequest`, `PutRequest`, `PatchRequest`, `DeleteRequest`,
`StreamRequest` and `SocketRequest`), so a Python Tac client fails with
`TY1308`, as Lua does (ADR 0085). Call the server from a JavaScript,
TypeScript or compiled service and pass the answer to the Python service as
an argument.

At run time a request or answer over 1 MiB, a member request over 64 KiB,
more than 256 live owners, or a call on a busy owner is refused. Python runs
on the page thread, so a long computation blocks the page. Threads,
subprocesses, native sockets, the operating-system file system and native
extensions are not available. Third-party packages are not supported. This
decision covers web builds; Python services in native app hosts are not part
of it.

### Tests

Python Tac test units run with `ty test`, under the same supervisor as the
other languages.

(Amended 2026-10-04) `ty init --tac py` and `ty add language client py`
scaffold a Python greeting service with its test, and write the pinned
`tac.python` setting into `client/manifest.json`. The developer still copies
the runtime files; `ty doctor` names the folder.

## Consequences

- A Python team can write page services in Python and share models with a
  Yon Python server.
- The first visit downloads about 13 MB of runtime before compression, much
  more than a JavaScript or Rust service. The route budget and `ty doctor`
  make that cost visible.
- Asynchronous methods, browser API resources, streams and third-party packages
  need their own decisions before they work in Python.

## Amendment 2026-10-04: events and topics

- A Python Tac service may have synchronous `@on_event` methods, with the
  same options as in Yon (`id`, `condition`, `replay`, `@order` and
  `@InBackground`). The page event bus calls them; a returned model is
  published again unless the listener is `@InBackground`.
- A field typed `ApplicationEventPublisher` is the page publisher. Tachyon
  sets it after it creates the service; `publish_event(model, retain)`
  sends a model from `client/models/` to the page bus.
- A listener for a model marked `@topic` opens the topic stream
  `/.tachyon/topics/<name>` by itself, like the other WebAssembly services.
- `async def` methods, streams and browser API resources still fail with
  `TY1605`.

## Amendment 2026-10-11: routes with several web runtimes

- [ADR 0100](0100-a-route-with-several-web-runtimes-gets-a-measured-allowance.md)
  gives a route that loads more than one web runtime a measured script
  allowance. Python is not one of those runtimes, and the 512 KiB budget of
  a Python route does not change. When a Python route also loads other web
  runtimes, its budget is the larger of the two.

## Amendment 2026-10-17: async methods, behaviors, saved fields, stores and clients

- An `async def` method builds. The page gets a promise: the bridge answers
  `{"task": n}`, and the shared task settlement of the other Tac languages
  sends the answer when the coroutine finishes. Disposal cancels the
  owner's unfinished tasks, and their callers get `CANCELLED`.
- `@on_start` may be `async def`. A call that arrives before it finishes
  waits for it. `@on_stop` and `@on_event` stay synchronous; an
  `async def` one still fails with `TY1605`.
- Python reaches the page's storage and HTTP hosts. With no adapter in its
  Setup, a Python behavior context uses the page cache for `CacheSettings`,
  the page entity store (adapter `indexeddb`) for `Entity` and
  `AllOrNothing`, and the browser HTTP transport for a generated
  `HTTPClient`. A Setup adapter still replaces each of them. Python
  therefore joins the Tac lists of `CacheSettings`, `Cached`, `CacheKey`,
  `ClearCache`, `UpdateCache`, `PageDefaults`, `Retry`, `MaxAtOnce`,
  `AllOrNothing`, `Store`, `Query`, `HTTPClient`, `GetRequest`,
  `PostRequest`, `PutRequest`, `PatchRequest`, `DeleteRequest`,
  `PathVariable`, `RequestParam`, `RequestBody` and `RequestHeader`.
- A `Store` in `client/stores/` or an `HTTPClient` in `client/remotes/` is a
  `Protocol` contract, as in Yon. The build writes its class, and a `Needs`
  field reaches it like any other provider.
- `LocalState` and `SessionState` fields are written in the type, as
  `Annotated[str, local_state]`. The page saves and restores them like the
  saved fields of the other languages; the field's page name is its
  lowerCamelCase name.
- `ValueSource` works in Python Tac test units, which run under native
  CPython as in Yon.
- `StreamRequest` and `SocketRequest` stay refused with `TY1308`. Only
  JavaScript, TypeScript and Go hold a browser stream or socket in a Tac
  service.
- This amendment replaces the 2026-10-04 limits on behaviors, saved fields
  and `HTTPClient`, and the `async def` limit in "Limits" above.
