# ADR 0009: Platform Navigation Over a Client Renderer

- Status: Superseded by ADR 0015
- Date: 2026-07-27

## Context

The legacy implementation ships a single-page application: a 1,060-line client
renderer, a 605-line DOM helper layer, and the router, cache, and hydration
code around them. It exists to deliver two things a user can feel — navigation
that is instant, and navigation that is smooth.

The Rust implementation reaches those goals from a different starting point.
Every route is already prerendered to static HTML, so a plain link works with
no JavaScript at all. What a client renderer would add is speed and polish, not
capability.

Reproducing it would mean porting the largest and most defect-prone subsystem
in the legacy codebase, and owning a DOM reconciler forever, to reach an end
state the browser now reaches natively.

`CONTEXT.md` is explicit that the rewrite is not a line-by-line port: stable
behavior, source conventions, and contracts carry forward, while internal
implementation structure is greenfield.

## Decision

Do not port the client renderer. Adopt the platform features that deliver the
same two user-visible properties.

Every generated page opts into cross-document view transitions with
`@view-transition { navigation: auto; }`, served as a stylesheet. Every
generated page carries speculation rules that prefetch same-origin routes at
moderate eagerness, so a route is usually already fetched before it is clicked.

Both are inert where unsupported: navigation simply proceeds normally.

The development server's policy gains `'inline-speculation-rules'`, the CSP
keyword defined for this exact case. The rules payload is JSON, not executable
script, so no general inline-script allowance is introduced.

## Consequences

The two user-visible benefits are delivered by roughly forty bytes of CSS and a
JSON block, instead of by 1,665 lines of client JavaScript that would need
testing, fuzzing, and long-term maintenance. Pages keep working with JavaScript
disabled. There is no reconciler to desynchronise from the server's HTML,
which removes an entire category of defect.

Against that, state does not survive navigation the way it can in a
single-page application: a document is genuinely replaced. An application that
needs state to outlive a route change must hold it in an island, in storage, or
on the server. Prefetching also trades bandwidth for latency, and browsers
without support get correct but ordinary navigation.

This is a deliberate divergence from the legacy implementation and is recorded
as such in `PARITY_LEDGER.md` rather than presented as parity.

## Rejected Alternatives

Porting the renderer was rejected for the reasons above: highest cost and
highest risk in the codebase, to reach where the platform already is.

Writing a smaller bespoke renderer was rejected as the worst of both. It still
owns a reconciler, still desynchronises, and gives up the legacy version's one
advantage, which is years of production hardening.

Leaving navigation entirely unenhanced was rejected because instant and smooth
navigation are real properties users notice, and the platform now supplies them
for almost nothing.

## Acceptance Gate

- [x] Generated pages opt into cross-document view transitions.
- [x] Generated pages carry speculation rules that prefetch same-origin routes.
- [x] A same-origin route is prefetched before it is clicked, proven in a real
      browser.
- [x] No content-security-policy violation is reported.
- [x] Navigation still works with the features unsupported or JavaScript off.
