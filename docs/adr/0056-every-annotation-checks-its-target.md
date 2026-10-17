# ADR 0056: Every Annotation Checks Its Target

- Status: Accepted
- Date: 2026-09-26
- Builds on: [ADR 0044](0044-an-annotated-file-holds-one-class.md) (one class
  per file), [ADR 0046](0046-tachyon-annotations-are-global.md) (global
  annotations), [ADR 0047](0047-annotation-names-are-plain-english.md) (names),
  [ADR 0048](0048-yon-routes-come-from-annotations.md) (routes and clients),
  [ADR 0050](0050-go-annotations-are-comments.md) (Go comment markers),
  [ADR 0051](0051-controller-error-handlers.md) (error handlers),
  [ADR 0054](0054-ruby-is-a-yon-language-and-a-tac-service-language.md)
  (Ruby) and [ADR 0055](0055-annotation-arguments-use-types.md) (typed
  arguments and typed events)

## Context

Each language path checked annotations in its own way. The JavaScript path,
the path for the other languages, the Yon parser, the event engine and the
Ruby path each had their own lists of names and their own messages. The same
mistake gave different answers in different languages, and some mistakes gave
no answer at all. For example, `@Cached` on a field said "This behavior
annotation belongs on a type", a `Fake` field outside a test class was
accepted, and `@Order` without `@OnEvent` was accepted in some languages.

## Decision

- One rule table, `crates/tachyon-core/src/annotation_rules.rs`, has one rule
  for every annotation name in the registry. A rule says:
  - where the annotation goes: a class, a method, a field or a parameter;
  - whether it takes arguments: none, optional or required;
  - which languages can use it in Tac and in Yon;
  - whether it needs an instance member (not `static`);
  - which annotations it needs on the same member (`Order` needs `OnEvent`),
    which ones it cannot share a member with (`GetRoute` and `PostRoute`), and
    which annotation its class must carry (`Fake` needs `Test`, `GetRoute`
    needs `RequestRoute`);
  - whether a method may have it on at most one parameter (`FromBody`);
  - what it means, in one plain sentence;
  - its error code (TY1308, TY2015 for routes, TY1502 for Schedule).
- Every source file of every language passes the same gate
  (`annotation_layout::validate`), and the gate runs the table after the role
  and layout checks. So Tac and Yon, and all twelve languages (JavaScript,
  TypeScript, Python, PHP, Java, Kotlin, C#, Rust, Dart, Go, Swift and Ruby),
  get the same checks and the same messages.
- A message names the annotation as the language spells it (`@cached`,
  `#[Cached]`, `[Cached]`, `// @Cached`), the member it is on, where it
  belongs, what it means, and the file and line of the annotation. For
  example: "@Cached is on a field, but it belongs on a method. Cached keeps a
  method's answer and returns it again for the same arguments
  (server/services/PriceService.kt:12)".
- The deeper paths use the same table for their own messages ("accepts no
  arguments", "belongs on"), for the languages each annotation reaches
  (Ruby's behavior check, Component in Tac services), and for the role
  markers that Tac services remove (`Service`, `Lifetime`, `NeedsAll`,
  `Component` and a bare `Store`, now in one place for every Tac language).
- Ruby has no checks of its own: its annotations go through the same table.
  A new language joins by adding its extension to the table's language list
  and its sample classes to the table's tests.
- The detailed value rules stay in the one validator each annotation already
  has, shared by every language (for example `resilience.rs` for Retry,
  `queries.rs` for Store and Query, `configuration.rs` for Settings,
  `events.rs` for OnEvent and Order, `routes.rs` for route paths and status
  codes). The table owns the target, the argument presence, the reach and the
  combinations.
- A test fails when a registry name has no rule, when a rule names an unknown
  language or annotation, or when a language that a rule reaches has no
  accepted and no refused example.
- Editors: the TypeScript declarations type each annotation for its target,
  so `tsc` and editors refuse, for example, a class annotation on a method in
  TypeScript and in JavaScript checked with JSDoc, with standard and with
  legacy decorators. `Convert` and `Query` must return a Promise. The other
  languages cannot read Tachyon's markers in their editors, so for them the
  build is where the check happens.

- `NeedsAll` takes no arguments, like Lombok's `@RequiredArgsConstructor`:
  every declared field can be filled. A field list is refused with "NeedsAll
  takes no field list; every field can be filled", and
  `ty migrate fix --write` removes it. Empty parentheses (`@NeedsAll()`,
  `#[NeedsAll()]`) stay accepted, because they are the common spelling in
  every language. A class with `NeedsAll` declares no constructor
  (`constructor`, `__init__`, `__construct`, `initialize`, `init`, `fn new`,
  or a Java, Kotlin, C# or Dart constructor): NeedsAll builds the class. Go
  has no constructors, so it has nothing to check.

## Consequences

- Misuse that was accepted before is now refused. The CHANGELOG lists it.
- One table is the place to read or change where an annotation goes.
- Python route and client parameter markers live inside the parameter's type
  (`order: Annotated[Order, from_body]`, ADR 0048 as amended), so those files
  stay valid Python. The table checks them like every other marker, and it
  refuses the old `@from_body order` form with a message that shows the new
  one.
- Every other Python marker that sits on a field or a parameter follows the
  same rule (amended 2026-09-26): `repo: Annotated[OrderStore, needs]`,
  `count: Annotated[int, min(1), max(100)] = 1`,
  `query: Annotated[OrderQuery, check]`, `key: Annotated[str, cache_key]`.
  The first `Annotated` argument is the declared type that validation and
  injection read, and the markers keep their snake_case names and their
  Spring meaning. Class and method markers stay decorators, which Python
  accepts. The table refuses the old decorator form on a field or a parameter
  and shows the new one; `ty migrate fix --write` converts it and adds
  `from typing import Annotated`. The website checks every published Python
  file with `python3 -m py_compile`.

## Amendment 2026-10-04: Python Tac reach, F# streams and Java

- Python Tac services have no behavior or storage adapter (ADR 0093). The
  table's Tac list for those annotations (`TAC_BEHAVIOR_ADAPTER`) leaves
  Python out, so the table refuses them once with its usual message, and
  tools that read the table see the refusal. `OnEvent`, `Order` and
  `InBackground` keep the full Tac list: Python listeners work (ADR 0093
  amendment "events and topics").
- `StreamRoute` lists F# in Yon. F# stream members return `seq<'T>` or
  `IAsyncEnumerable<'T>`, and the parser message already named F#.
- Java Tac services stay outside the table's Tac language list for now.
  TeaVM services are read by their own declaration reader, which accepts
  only the annotations that the TeaVM path lowers and refuses the rest with
  `TY1405`. Adding Java to the Tac list would make the table refuse Java a
  second time, with a different code, wherever its lists and the reader's
  list differ. The table lists Java only where a shared path reads it
  (`Service`, `NeedsAll`). Java joins the Tac list when the TeaVM reader
  reads the table instead of its own list; until then, tools take Java's
  refusals from `TY1405`.

## Amendment 2026-10-17: Python Tac behaviors, stores and clients

- Python Tac services now have behavior, storage and HTTP adapters (ADR 0093
  amendment of 2026-10-17). The table drops `TAC_BEHAVIOR_ADAPTER` and adds
  Python to the Tac lists of the behavior, storage, `Store`, `Query`,
  `HTTPClient` and request annotations, to `LocalState` and
  `SessionState`, and to `ValueSource`. `StreamRequest` and `SocketRequest`
  keep their JavaScript, TypeScript and Go list.
