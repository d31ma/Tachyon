# Persistent prerender cache — status & productionization plan

Build-speed work for large frontends. The bottleneck is **prerendering** (~86% of a
cold build), not the compiler (~12%). A persistent cache that reuses the rendered
HTML of unchanged pages removes most of it — a bigger win than any Rust port, and it
stays in JavaScript. This doc tracks what's shipped and what's left to productionize.

## Measured impact (synthetic projects, `bun run bench:build`)

| pages | cold | warm (0 changed) | partial (5 changed) | incremental (watch) |
|------:|-----:|-----------------:|--------------------:|--------------------:|
| 200 | 1850ms | 371ms | 422ms | 22ms |
| 500 | 4709ms | 843ms | 886ms | 40ms |

Cold→warm ≈ **5.4×**. Editing a few files (`partial`) barely differs from `warm`:
once prerender is cached, the compile phase is the floor — the only slice Rust could
touch. Cache-hit HTML is **byte-identical** to a non-cached build (verified).

## Done

- [x] **Deterministic ids (default).** `Compiler.idGenerator` — stable per-module
      prefix + monotonic counter; unique by construction, reproducible across builds.
      Escape hatch: `TAC_RANDOM_IDS=1`. Soak test: `tests/compiler/deterministic-ids.test.js`.
- [x] **Prerender cache prototype** (opt-in `TAC_PRERENDER_CACHE=1`). Keyed on compiled
      page+wrapper module bytes; on hit, reuses stored HTML (still rewrites module
      imports for hydration). `src/compiler/index.js` → `prerenderRoutes`.
- [x] **Build-timing harness.** `scripts/bench/` — cold / warm / partial / incremental
      with phase split and cache hit-rate. `benchPhase` in `src/cli/bundle.js`.

## To productionize (remaining)

- [ ] **Broaden the cache key.** Today it covers page+wrapper module bytes only. Fold in:
      - the document **shell** (title/scripts/asset hashes) — a shell change must invalidate.
      - **wrapper-wraps-nested-pages**: editing a wrapper must invalidate every page nested in it.
      - Acceptance: editing one shared asset / wrapper invalidates exactly the affected routes, nothing stale.
- [ ] **Live-data opt-out.** Pages that `fetch()` at prerender aren't a pure function of
      source. Add a per-page opt-out (marker/config) that skips caching for them.
- [ ] **Cache lifecycle.** `.tac-cache/` needs a size cap + LRU eviction, a schema/version
      tag (bust on framework upgrade), and a `.gitignore` entry. Decide CI persistence
      (restore/save `.tac-cache` between runs — that's where the CI win lands).
- [ ] **Flip the cache default on** once the above holds, with `TAC_NO_PRERENDER_CACHE` escape hatch.
- [ ] **Correctness gate in CI.** A test that asserts cache-hit output == no-cache output
      across a fixture with wrappers, nested pages, and shared assets.
- [ ] **Watch-mode integration.** The in-memory watch incremental (already ~40ms) and the
      persistent cache should share one invalidation model, so a cold start after a watch
      session reuses the cache instead of re-rendering everything.

## Risks / open questions

- Non-deterministic render inputs (time, random, live data) silently poison the cache →
  the live-data opt-out and the correctness gate are the guards. Keep `TAC_RANDOM_IDS`
  as the escape hatch if deterministic ids ever surface a problem.
- Cache key uses a 32-bit hash of module bytes → negligible collision odds at repo scale,
  but the correctness gate is what actually proves output equality.

## Running the harness

```
bun run bench:build                          # default sizes 50/200/500, edit 5 pages
bun run bench:build --sizes 200/40,1000/200  # custom "<pages>/<components>"
bun run bench:build --changed 20             # partial rebuild edits 20 pages
bun run bench:build --no-cache               # cold-only, shows the compile/write/prerender split
```
