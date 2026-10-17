# ADR 0091: Role Folders Allow Nesting and Require Explicit Registration

- Status: Accepted
- Date: 2026-10-01
- Amends: [ADR 0031](0031-realms-mirror-each-other.md) (shared realm
  folders) and [ADR 0044](0044-an-annotated-file-holds-one-class.md)
  (behavior-only Tac services)
- Preserves: [ADR 0043](0043-models-hold-data-types.md) (data-only models),
  [ADR 0047](0047-annotation-names-are-plain-english.md) (marker names),
  [ADR 0048](0048-yon-routes-come-from-annotations.md) (annotation routes),
  [ADR 0064](0064-code-is-not-duplicated.md) (annotation-free utilities) and
  [ADR 0088](0088-remotes-are-the-canonical-outbound-role-folder.md)
  (`clients/` stays a compatibility name for `remotes/`)
- Related: [ADR 0092](0092-settings-and-validation-data-live-in-models.md)
  (settings and validation data move from `helpers/` to `models/`)

## Decision

The client and server role folders stay the place where Tachyon finds roles.
Inside a role folder, authors may organize files in nested folders, such as
`client/services/billing/InvoiceService.ts`. Nested services and controllers
find their collaborators across the whole realm. Tachyon uses the real
relative paths when it composes source, rebases imports and hashes services.
A nested folder does not create a new injection namespace, so two variants of
the same service in one language are still an error. Discovery keeps the
existing rules: it opens no symbolic links, reads files in a fixed order, and
stops at 4096 entries and 32 folder levels.

A concrete class, struct, object or record in `services/`, `stores/`,
`remotes/` (or `clients/`), `setups/`, `entities/`, `helpers/` or
`converters/` needs that folder's marker. A server controller needs
`RequestRoute`, `SocketRoute`, `ErrorHandler` or `IdentityProvider`. The
marker spelling and the languages that support it stay language-specific.
A missing marker fails with `TY1363`. The message names the file and the
marker spelling for its language. The placement, class suffix, one-class and
role-specific checks still apply.

This replaces the ADR 0044 rule that let a Tac service with only behavior
annotations leave out `Service`. There is no switch that registers concrete
unmarked code. An existing app adds the marker to a class that really has the
role, or moves the class to its plain folder.

Abstract classes, interfaces, protocols, traits, type aliases, files that
only re-export, and assets are not roles. Compiler output is not authored
source. Tests outside application discovery stay fixtures. Models, plain
shared code, native errors and annotation-free static utilities keep their
own rules. Helpers are formal roles: a `*Service` class in `helpers/` fails
the suffix check and belongs in `services/` with `Service`. Where a language
has no Helper support yet, a Helper fails with a clear message instead of
being accepted unchecked.

`ty migrate check` reports each missing marker as a `role.marker` finding.
It does not add markers, build Store arguments, choose a controller route or
give helper code an injectable lifetime. The author decides the role and moves
misplaced collaborators. `clients/` stays accepted; folders with retired names
keep their migration errors.

Settings and validation data holders that used to sit in `helpers/` move to
`models/`. ADR 0092 describes that move.

## Native Rust services and their dependencies

A native Rust service is built with the other Rust services of the same
language that it needs, in the namespace of the service that owns them. Their
constructor metadata is kept while their role markers are removed, and the
authored files are checked before that step. The browser keeps services in
separate modules as before. A shared native provider graph is needed for
singletons shared across pages and for dependency lifecycle hooks; building a
dependency inside its owner does not give those guarantees. Choosing a trait
implementation needs an explicit typed binding; Tachyon does not infer one.

## Caller-owned C# Tac Helpers

A C# Tac Helper works when its caller creates it. A marked Helper keeps its
typed constructor and must be created with `new` outside its own class. The
Setup or Service that creates it owns any context it receives. Helper
provider and lifetime declarations on the class, injected Helper dependencies
and Helper lifecycle or event hooks are refused before the authored metadata
is removed. Field `Needs` constructor lowering works only for typed
constructors that are called explicitly. C# has no managed Helper injection
and no automatic Helper cleanup.

## Swift Tac Helpers — 2026-10-17

A Swift Tac Helper is built by the field that needs it. Tachyon writes
`@Needs private let cartHelper: CartHelper` as
`private let cartHelper: CartHelper = CartHelper()`, in a service and in a
Helper under `client/helpers/` at any depth, so a Helper may need other
Helpers. Each class that needs a Helper owns its own instance, which lives
and goes away with that class. A Swift Helper therefore has an `init()`
without arguments. It has no lifetime of its own and no `OnStart`, `OnStop`
or `OnEvent`; the event check refuses those with `TY1405`, as it does for any
type that is not the service. Named does not select a Swift Helper.

Evidence: `tac_web::swift_helper_tests` builds a Swift prototype service that
needs a Helper which needs another Helper, runs it under Node, and checks that
each service instance and its replacement have their own Helpers.

## Amendment (2026-10-03): one structure check before the build

The rules of this ADR, ADR 0067 (file names) and ADR 0064 (utilities) run as
one structure check. Project discovery reads every file of both realms at any
depth and checks each file before the compiler writes output or starts a
toolchain (`dart`, `cargo`, `go`, `dotnet` and the others). It collects the
problems of every file into one report, so one run lists all of them. Each
diagnostic names the file, points at the declaration or import, and gives the
fix in the language's own spelling. Within one file the checks stop at the
first problem, because the later checks read the class that the earlier ones
accept.

The check covers, per role folder and language:

1. the role marker of the folder (`TY1363`);
2. the role suffix of a marked class and its role folder (`TY1310`);
3. the file name, which is the class name in the language's file name style
   (`TY1318`, `TY1310`);
4. one primary class per role file, or the language's form of it: a Go struct
   with its methods, a Lua class table, an F# type, a Rust type with its
   `impl` blocks (`TY1310`);
5. dependencies that point down and never reach a controller (`TY1327`). A
   utility class in `client/shared/utils` or `server/utils` names no role class:
   every class of its realm may call it, so it cannot depend on one. The
   other design rules check `client/shared/utils` too (ADR 0069 amendment);
6. realm boundaries: a JavaScript, TypeScript or Dart file never imports a file
   of the other realm with a relative path (`TY1369`). Client code reaches the
   server through an HTTPClient; server code never imports client code.

Models, utilities, tests, generated files, the native Go module in
`server/packages` (ADR 0095) and the support files in `client/shared` keep
their own rules. A folder that is not a role folder never needs a marker.
`ty migrate check` lists missing markers (`role.marker`), imports of the other
realm (`role.realm_import`) and the dependency findings; `ty migrate fix
--write` renames a file whose class has another name.
