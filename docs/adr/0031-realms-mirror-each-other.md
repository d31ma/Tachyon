# ADR 0031: Realms Mirror Each Other

- Status: Accepted; qualification in progress
- Date: 2026-09-15
- Amends: [ADR 0024](0024-strict-annotation-modules.md) (role folders beside
  a Yon entry), [ADR 0025](0025-required-constructor-inference.md),
  [ADR 0028](0028-owned-application-events-and-lifecycle.md) (EventPublisher),
  [ADR 0030](0030-components-use-services.md)
- Preserves: client-only Tac, REST-only Yon, filesystem routing
- Amended by: [ADR 0044](0044-an-annotated-file-holds-one-class.md), which
  lets a role file hold only its one class and its imports
- Amended by: [ADR 0047](0047-annotation-names-are-plain-english.md): annotations and role folders have plain English names (for example `Needs`, `NeedsAll`, `Store`, `OnStart`, `stores/`); the old names are refused and `ty migrate fix --write` renames them
- Amended by: [ADR 0048](0048-yon-routes-come-from-annotations.md): Yon routes come from annotations in `server/controllers/` (`RequestRoute`, `GetRoute`, ...), not from `server/routes/` folders; clients use `HTTPClient` and `GetRequest`, ...; `OPTIONS.schema.json` is replaced by schemas built from models

- Amended by: [ADR 0091](0091-role-folders-allow-nesting-and-require-explicit-registration.md): nested organization within realm role folders and explicit supported role registration.
- Amended by: [ADR 0093](0093-python-is-a-tac-service-language.md): Python is also a Tac service language.

## Context

Tac and Yon developed their annotation sets and folder layouts separately:

- Tac's services and collaborators are shared under `client/<role>/`. Yon's
  sit beside each route entry.
- Yon has Controller, Component, bare Repository, Inject and Stream. Tac has
  Resource and a browser Client.
- Both publish local events by returning a value from an `@EventPublisher`
  method. Tachyon invented this; Spring has nothing like it.

In Spring, code injects `ApplicationEventPublisher`. Listeners carry
`classes`, `condition` and `id`, and they can return further events.
`@Order` sets their order, and `@Async` runs them separately.

## Decision

Both realms use one layout, one marker set and one event model. This applies
in every language that a realm supports, including native Tac services.

### Folders

- Role folders are shared per realm. The client folders are
  `client/services`, `client/clients`, `client/repositories`,
  `client/configurations`, `client/entities` and `client/delegates`. The
  server mirrors them: `server/services`, `server/clients`,
  `server/repositories`, `server/configurations`, `server/entities` and
  `server/delegates`.
- A Yon route directory holds only its `yon.<ext>` entry and its
  `controllers/` folder. Controllers are the one role that stays with the URL,
  because they are the URL. A role file beside a route fails with `TY1310`.
  The error names the file's shared destination.
- Every route entry combines the shared roles of its language with its own
  controllers. A shared service is one class. Each Yon invocation still owns
  its own instance graph.

### Markers

- Both realms have these markers: every shared behavior annotation,
  `Service`, `Scope`, `Repository` (bare and entity-bound), `Client`,
  `Component`, `Inject`, `Resource`, `RequiredArgsConstructor`,
  `PostConstruct`, `PreDestroy`, `EventListener`, `Order` and `Async`.
- Only Yon has `Controller` and `Stream`. Both belong to the HTTP interface,
  which Tac does not have.
- `Resource` names depend on the realm:

  | Realm | `Resource` names |
  | --- | --- |
  | Browser | `element`, `signal`, `cacheManager`, `resourceLoader`, `renderScheduler` |
  | Server | `dataSource`, `redis`, `applicationEventPublisher` and, in JavaScript and TypeScript, `signal` |

- Tac `Inject(A, B)` names provider types and binds them to the lower-camel
  fields `a` and `b`, as Yon does. A Tac `Component` is an injectable role
  with no process rights. `Relay` stays retired everywhere.

### Events

- `EventPublisher` is removed. To publish, a class first gets a publisher in
  one of two ways:
  - it declares an uninitialised `applicationEventPublisher` field, which
    `RequiredArgsConstructor` infers like `dataSource`; or
  - it declares a `Resource` with that name.

  It then calls `publishEvent(event, value)` or
  `publishEvent(event, value, retain)`.
- `EventListener` takes `event` or `events` (Spring's `classes`),
  `condition`, `id` and `replay`. `condition` is a bounded expression over
  `value` and `event`, for example
  `value.status == 'paid' && value.total >= 10`. The compiler compiles it once
  into a matcher. Every runtime evaluates that matcher without an evaluator of
  its own.
- When a listener returns `{event, value}`, or an array of them, the runtime
  publishes the return value again. It ignores any other return value. An
  `Async` listener runs after the current delivery and never publishes again,
  as in Spring.
- `Order(n)` orders the listeners of one event, lowest first. Listeners
  without an order count as 0 and keep their declaration order.
- `Async` is accepted where the realm's language runs asynchronous methods:
  Tac JS/TS (JavaScript/TypeScript), native Tac services on their host queue,
  and Yon JS/TS/Python. Elsewhere it fails at compile time, as asynchronous
  methods already do.
- Amendment (2026-10-17): Yon Rust also accepts `InBackground`. In JS/TS and
  Python the deferred listener is awaited after the other listeners of the
  same event, so the delay is an order, not a second thread. Rust keeps that
  order with a synchronous method: the listener runs after every other
  listener of the event, its failure is logged, and its return value is never
  published again.

## Consequences

- `ty migrate` reports a role file beside a route as `Changed`, with its
  shared destination. It reports an `@EventPublisher` method as `Changed`,
  with the injected publisher.
- The website, the docs and every fixture move to the shared layout.
