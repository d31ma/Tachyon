# Phase 3 Implementation Plan

> Historical plan: ADR 0015 supersedes Tac SSR and hydration decisions, and
> ADR 0016 removes Yon templates, route context, and build-time execution.

## Boundary

Phase 3 extends the existing modular monolith. It adds no crate and no internal
service. `tachyon-contracts` owns View IR and View Source Map wire types;
`tachyon-core` owns parsing, validation, component lowering, client-plan
generation, incremental reuse, and the generated browser runtime;
`tachyon-cli` remains orchestration and presentation.

## Public Interfaces

```rust
pub struct ViewIr {
    pub contract_version: u8,
    pub source: String,
    pub root: ViewNode,
}

pub enum ViewNode {
    Element(ViewElement),
    Text(ViewText),
    Conditional(ViewConditional),
    Iteration(ViewIteration),
    Component(ViewComponent),
}

pub struct ViewSourceMap {
    pub contract_version: u8,
    pub output: String,
    pub sources: Vec<String>,
    pub mappings: Vec<ViewSourceMapping>,
}

pub struct TemplateProgram { /* validated immutable AST */ }

impl TemplateFrontend {
    pub fn compile(
        source: &str,
        source_path: &str,
        components: &ComponentRegistry,
    ) -> Result<TemplateProgram, Failure>;
}

impl ClientViewRenderer {
    pub fn render(
        program: &TemplateProgram,
        components: &ComponentRegistry,
        route: &str,
    ) -> Result<ClientRenderPlan, Failure>;
}

impl WebCompiler {
    pub async fn build_async(
        project_root: impl AsRef<Path>,
        options: &BuildOptions,
    ) -> Result<BuildResult, Failure>;
}
```

The synchronous `WebCompiler::build` remains for non-async library callers and
drives the same deterministic Phase 3 pipeline. The compiled CLI and
development server use `build_async`.

`BuildOptions` gains `incremental: bool`, defaulting to true. `BuildResult`
adds `compiled_routes()` and `reused_routes()`.

## Vertical Slices

1. Add typed View IR and View Source Map contracts, then serialize validated
   interpolation ASTs into the client plan.
2. Add the bounded expression parser and lower conditional and iteration
   syntax without evaluating it during the build.
3. Discover, parse, and lower components with properties, slots, cycle checks,
   and expansion budgets.
4. Keep Yon handler execution outside the build and view pipeline.
5. Emit deterministic component mount metadata, component modules, and the
   external client renderer.
6. Emit View IR/source maps, then add verified incremental reuse and atomic
   build state.
7. Add multi-diagnostic recovery, adversarial tests, real-browser checks, CI,
   security documentation, and completion evidence.

Every slice preserves the prior published output on failure.

## Black-Box Acceptance Corpus

The compiled `ty` executable must prove:

- JavaScript and Python Yon handlers are discovered without executing `GET`,
  and `yon.html` is rejected;
- both control syntaxes, nested loops, escaped bindings, dynamic attributes,
  web components, component properties, and slots render correctly;
- View IR and source maps validate against their canonical schemas;
- every component mount policy has deterministic client-plan and module behavior;
- a real Chromium session creates the initial DOM, schedules activation,
  handles an interaction, and rerenders structural changes;
- malformed expressions/controls, component cycles,
  invalid slots/islands, expansion limits, and missing values fail safely;
- two independent source errors appear in one Diagnostics v1 report;
- unchanged Tac routes reuse verified output, component changes invalidate
  consumers, handler-only routes emit no view, corrupt state misses safely, and
  `--no-incremental` forces recompilation;
- failed builds retain the complete prior output.

## Security and Operational Review

- No template path reaches JavaScript evaluation or a shell.
- Browser render values are escaped at their output context.
- View IR, source maps, manifests, diagnostics, and incremental state exclude
  Yon response values and secrets.
- Component module URLs are compiler-generated and checked as same-origin.
- Props are public and cannot authorize a request.
- Handler execution remains outside builds and retains the Phase 2 process and
  environment limits at request time.
- Cache state is untrusted, path-contained, digest-verified, and disposable.
- Parser, evaluator, expansion, output, diagnostics, handler, and cache reads
  are bounded.
