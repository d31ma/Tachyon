# Native-first hybrid rendering

Status: greenfield, Native UI schema version 1.

Every non-web target is native-first. Application developers author the same
HTML used by the web build; there is no application-wide render-mode flag and
no SwiftUI, Compose, WinUI, or GTK view language in an application project.

## Pipeline

1. The compiler resolves Tac components, slots, bindings, and structural
   control tags into the platform-neutral View IR.
2. The native planner evaluates bounded route state and walks the resolved
   tree one subtree at a time.
3. Supported HTML semantics and explicit semantic roles lower to native
   controls.
4. The smallest unsupported safe subtree becomes an isolated local
   `WebSurface`; supported parents, siblings, and unrelated descendants remain
   native.
5. The platform host reads Native UI v1, reconciles native state, mounts local
   surfaces, and dispatches declared events through the controller contract.
6. Same-origin navigation from a local surface returns to the native route
   stack.

Control tags never reach a platform renderer. A surviving `if`, `else`, `for`,
or `loop` is a compiler failure, not an unknown HTML element.

## Authored tag behavior

- Standard supported elements become native controls or native layout
  containers.
- A custom element may select a native adapter with an explicit supported role,
  such as `role="button"` or `role="banner"`; adapters are semantic rather than
  tied to one design-system prefix.
- Tac components are composed before planning, so their invocation tags do not
  become unknown native elements.
- An unknown custom or web component becomes a bounded local `WebSurface` when
  its subtree is safe to isolate.
- Malformed HTML, invalid state, unsafe URLs, unsupported capability requests,
  and a subtree that cannot be isolated safely fail closed with a diagnostic.

Fallback is local, not contagious. One unsupported chart does not convert its
native heading, button, or surrounding route into an application-wide WebView.

## Security boundary

Local surfaces load only generated bundle assets through the platform's local
asset scheme and receive a restrictive content-security policy. Remote
surfaces are HTTPS-only and host-pinned. Neither local nor remote surfaces
receive a general native bridge; remote content always records `bridge: none`.
Capability Manifest v1 remains deny-by-default.

## Bundle contract

Each target publishes beneath `dist/<target>/` even when several targets are
built by one command. Native UI v1 contains the entry route and one resolved
tree per route. Each live snapshot contains `schemaVersion`, `route`, and
`root`; element nodes carry stable IDs, adapter identity, attributes,
accessibility, events, and children. `WebSurface` nodes name their isolated
local payload or approved remote origin.

The host refuses a mismatched schema, unresolved control flow, an undeclared
event, an unsafe surface, or a capability absent from the manifest. Fallback
decisions and surface counts remain inspectable in build output and the
artifact manifest.

## Removed compatibility switch

`--render-mode` and `TAC_RENDER_MODE` are rejected with a migration diagnostic.
The old whole-application `webview` mode is deliberately not retained: native
adapters and local subtree fallback are now one unconditional planning model.
