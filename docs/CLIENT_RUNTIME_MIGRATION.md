# Client runtime and native companion migration

This guide describes the reconciled implementation after v26.35.07. Do not
assume these APIs exist in an older installed binary because a website or a
version label mentions them. Check `ty --version` and run the acceptance gates
below against the exact binary being installed or published.

## The instance runtime

Each page or component default-class instance receives a non-enumerable
`this.tac` binding before mount hooks run. Constructors and field initializers
must not use it; use `@onMount` instead. The public type is `TacRuntime` in
`types/tachyon-env.d.ts`.

```javascript
export default class Catalog {
  items = [];

  @onMount
  async load() {
    const response = await this.tac.fetch('/api/catalog',
      { credentials: 'omit' }, { cache: 'network-first' });
    if (!response.ok) throw new Error('Catalog unavailable');
    this.items = await response.json();
  }
}
```

The API includes `fetch`, `invalidate`, `precache`, `clearAssetCache`, `publish`,
`subscribe`, `retained`, and `render`. After state changes outside a framework
event or subscription callback, call `await this.tac.render()`. Page mount
hooks precede the first render; component mount hooks run after their DOM is
connected and should request a render when they change displayed fields. This is a client
renderer, not SSR or an application-owned render-closure registry. Runtime
methods are excluded from enumerable state so hot-update snapshots remain
cloneable.

Ordinary JS/TS module imports, top-level constants, and helper functions remain
legal. Native reflection restrictions do not impose a class-only grammar on
browser modules.

## Response caching: explicit privacy boundaries

`fetch(input, init?, options?)` returns a standard `Response`. Relative URLs
resolve against the current document. Its optional IndexedDB cache is
independent of the browser HTTP cache, which is bypassed for network requests
so it cannot silently override the chosen policy.

| `options.cache` | Behavior |
| --- | --- |
| `network-first` (default) | Request current data; fall back to a stored response only on network failure. |
| `cache-first` | Use a non-expired stored response first. |
| `reload` | Require the network and refresh an eligible stored response. |
| `no-store` | Never read or write the persistent cache; require the network. |

Persistence is allowed only for same-origin GET/HEAD requests with explicit
`credentials: 'omit'`, without Authorization, and without request
`cache: 'no-store'`. Responses declaring `Cache-Control: private` or `no-store`,
or any `Vary`, are not stored. Credentialed reads still work but go directly to
the network. Do not cache secrets, tokens, user-specific data, or data whose
authorization can change. The runtime does not implement authenticated cache
partitioning. Aborted requests reject rather than returning cached data.

Requests with `Range` or `If-Range` bypass persistent-cache reads and writes,
including offline fallback. A partial response (`206` or any `Content-Range`
header) is never stored or returned from an older cache record as a complete
response. These requests require the network even under `cache-first`.

Keys default to `fetch:<METHOD>:<absolute URL>`. A caller-supplied `options.key`
must identify the actual resource and representation; reusing it for unrelated
URLs intentionally aliases them. Invalidate after successful mutations:

```javascript
await this.tac.fetch('/api/catalog/item', {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ name: 'Example' }),
}, { invalidateKeys: ['catalog'], invalidatePrefixes: ['catalog:'] });

await this.tac.invalidate(['catalog'], ['catalog:']);
```

The response store holds at most 128 entries, at most 256 KiB each; records
expire after 24 hours. A full store clears atomically before the next write.
IndexedDB operations and body reads settle within bounded two-second windows;
denied storage, corruption, and quota failure fall back to network behavior.
HTTP error responses are returned as errors, not replaced by stale successes.

The separate service worker honors declared `tac.config.js` cache rules only
for anonymous public API reads. Credentialed/Authorization/no-store requests
bypass and evict matching worker entries before lookup; private, no-store and
Vary responses evict earlier entries. Known packaged files may bootstrap from
ordinary document/module requests, but the worker fetches them anonymously and
verifies their bytes against compiler-emitted SHA-256 fingerprints before
caching. Arbitrary HTML and extension-looking URLs are not trusted assets.
API bodies are limited to 256 KiB, packaged assets to 4 MiB, and worker storage
to 256 entries/32 MiB. Native hosts do not register this browser service worker.
Changing worker code changes the cache version as well as the worker script.

`precache(urls)` warms the current generated service-worker cache with eligible
same-origin public assets. It does not cache cross-origin or private responses.
Each request is bounded, at most 512 URLs are accepted, each body is at most
4 MiB, and one warm-up has a 30-second/32-MiB budget. Components referenced by
untaken branches are warmed at idle within those bounds. `clearAssetCache()`
removes Tachyon static caches; it does not clear IndexedDB or persisted fields.

## Explicit `$` and `$$` persistence

```javascript
export default class Preferences {
  $draft = '';       // sessionStorage: the current tab
  $$theme = 'light'; // localStorage: survives browser sessions
}
```

The stored value overrides the declared default before mount hooks. A value
is written on assignment, not initial declaration. Reassign an object after
changing it: nested mutation alone does not run the persistence setter.
Assigning `undefined` removes the stored key. Keep values JSON-serializable and
small (64 KiB serialized limit); these APIs are not secure storage.

Page scope combines the page module and pathname. Component scope combines the
component name and pathname; provide a `persist-id` prop to distinguish two
instances of the same component. Storage denial or invalid JSON preserves a
usable in-memory default. Storage changes are not automatically broadcast into
already-mounted instances in other tabs.

## Document-local signals and decorators

```javascript
export default class Counter {
  @publish('counter.value')
  $count = 0;

  @subscribe('counter.value')
  displayedCount = 0;

  @publish('counter.result')
  async calculate() { return this.$count * 2; }
}
```

Decorators occupy their own lines on instance members of the exported default
class. `@publish` and `@subscribe` accept an optional quoted signal name;
without it the member name is used. `@onMount` applies only to methods. The
compiler lowers metadata before TypeScript transpilation; no native browser
decorator implementation or unsafe expression evaluation is required. Invalid
arguments, static/private members, reserved runtime names, and decorators
outside that class fail before emission.

Published fields emit their initial value and every assignment. Methods emit
their returned value or resolved Promise value; rejection publishes nothing.
Persistent field accessors remain intact when decorated. Subscribed fields
receive the retained value immediately; subscribed methods receive only new
publications. Subscription-driven changes rerender the client document.

The imperative form is available when cleanup or replay needs to be explicit:

```javascript
const stop = this.tac.subscribe('counter.value', value => {
  this.displayedCount = value;
}, { immediate: true, signal: abortController.signal });
this.tac.publish('counter.value', 4);
const latest = this.tac.retained('counter.value');
stop();
```

Signals are local to the document, not server topic logs or cross-tab messages.
Retained values are cloned snapshots, bounded to 128 names and 64 KiB each;
use JSON-compatible cloneable values. `{ retain: false }` delivers without
replacing an existing retained value. There are at most 128 subscribed topics,
1,024 listeners, and 32 synchronous nested deliveries. Component removal and
HMR dispose owned subscriptions; an AbortSignal or the returned function can
stop them earlier. Subscriber failures are isolated and expose only a generic
error marker, never payloads or exception text.

## Counted loops remain bounded

```html
<loop :for="let i = 0; i < 3; i++"><span>{i}</span></loop>
<loop :for="let i = 6; i >= 0; i -= 2"><span>{i}</span></loop>
<loop :for="const item of items"><span>{item.name}</span></loop>
```

The counted form requires one `let` binding, a comparison against that same
binding, and `++`, `--`, `+= positiveMagnitude`, or `-= positiveMagnitude` in
the correct direction. Zero, negative literal steps, mismatched bindings, and
extra statements fail compilation. Non-finite runtime bounds/steps produce no
iterations; a step that makes no numeric progress or more than 10,000
iterations fails with a bounded error. A separate 100,000-iteration budget
bounds all loops together, including nested empty loops that emit no nodes.
Outer lexical locals remain visible to
nested loops. `if`/`else`/`for` aliases and declaration-free iterable bindings
remain accepted; this reconciliation does not remove them.

## Browser Wasm to target-native companions

The former Wasm component ABI is retired. Keep browser page/component behavior
in `tac.js` or `tac.ts`. Add page-local native source beside `tac.html`:

| Native source | Targets |
| --- | --- |
| `tac.swift` | macOS and iOS |
| `tac.kt` | Android |
| `tac.cs` | Windows |
| `tac.rs` | macOS, Linux, Windows |

The platform-specific language takes precedence over the Rust desktop
fallback. Each route receives its own selected companion and state. Native-only
pages fail web compilation with an actionable diagnostic; add JS/TS if the
page also targets web. Dart/Python are not Tac companion targets.

Native hosts render the same Tac document in their platform web view. Native
companions run in the host and reach OS APIs through their language SDK. The
bridge is restricted to the packaged local application, not remote pages or
subframes. Its protocol carries the compiler's canonical route for every
`init`, `get`, `set`, or `call` operation.

Native methods are asynchronous in the browser. Use `{await method()}` in a
template or await the returned Promise in authored code. Initialization reads
field values but never calls methods to discover their return values. Field
assignments settle through the host before rerender; explicit method calls
refresh the field snapshot afterward. Per-field revisions prevent a delayed
reply from undoing later typing; refresh skips fields with queued writes.
Native method discovery must not cause side effects. The native publish
channel feeds the same retained document bus and preserves bounded early
publications.

Native assignments admit at most 128 queued writes. Each native call has a
ten-second browser wait limit. This timeout is not native-code preemption or
mutation rollback: application-owned native work may still finish after the
caller stops waiting. Keep companions responsive and move long-running work
off the host's interaction path. A hung companion may require an application
relaunch; failed mutations are not automatically replayed.

Android pins AndroidX WebKit `1.14.0` and uses its frame-aware asynchronous
message bridge, with no legacy JavaScript-interface fallback. A runtime
without `WEB_MESSAGE_LISTENER` reports that the Android native bridge is
unavailable; update Android System WebView or use a compatible runtime.
The bridge allows 128 pending document calls with ten-second deadlines and
uses one worker with a 128-request queue. Navigation rejects pending document
calls and discards stale queued work; activity destruction shuts down the
worker. These controls do not forcibly stop an already-running companion.

macOS and iOS share the same bounded Foundation JSON parser and canonical
request serialization. This replaces the handwritten Swift value scanner;
a byte-level guard enforces the 64 KiB/64-level limits and rejects duplicate
root keys before Foundation decodes the request.

Browser event handlers receive an implicit DOM event before authored arguments.
Native methods receive only the authored JSON arguments: `on:click="setCount(7)"`
invokes the native method with `7`, not a DOM event. Explicit expressions such
as `$event.target.value` still provide their resolved value.

Automatic native member discovery currently supports fields and zero-argument
public methods. The `setCount(7)` example requires an explicitly authored
native member table accepting that argument; parameterized methods are not
automatically bound. Use a field assignment or an explicit table until typed
parameter discovery is implemented.

## Acceptance against the installed or published binary

```sh
TAC_BIN=/absolute/path/to/ty node scripts/runtime-browser-test.mjs
TAC_BIN=/absolute/path/to/ty node scripts/service-worker-browser-test.mjs
TAC_BIN=/absolute/path/to/ty node scripts/storage-browser-test.mjs
TAC_BIN=/absolute/path/to/ty node scripts/native/companion-test.mjs
TAC_BIN=/absolute/path/to/ty node scripts/website-browser-test.mjs
TAC_BIN=/absolute/path/to/ty bash scripts/compat/verify-ledger.sh
```

Browser gates require Node, Playwright Chromium, and (for the HTTPS offline
case) OpenSSL. The desktop native companion gate requires the host packaging
toolchain, Rust, and Python 3; it compiles two routes with the selected binary
and executes the generated ABI, including native OS access and publication.
Platform UI gates remain separate: cross-compiling an archive is not native
interaction evidence. Historical ADR/evidence documents describe older
architectures, not the support level of this one.
