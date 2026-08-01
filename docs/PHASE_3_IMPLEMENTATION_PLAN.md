# Phase 3 Implementation Plan

## Boundary

Phase 3 extends the existing modular monolith. It adds no crate and no internal
service. `tachyon-contracts` owns View IR and View Source Map wire types;
`tachyon-core` owns parsing, evaluation, component expansion, context
composition, rendering, incremental reuse, and generated browser runtime;
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

impl TemplateRenderer {
    pub fn render(
        program: &TemplateProgram,
        context: &serde_json::Map<String, serde_json::Value>,
        components: &ComponentRegistry,
        route: &str,
    ) -> Result<RenderedView, Failure>;
}

impl RouteContextComposer {
    pub async fn compose(
        route: &RouteNode,
        supervisor: &HandlerSupervisor,
    ) -> Result<RouteContextSnapshot, Failure>;
}

impl WebCompiler {
    pub async fn build_async(
        project_root: impl AsRef<Path>,
        options: &BuildOptions,
    ) -> Result<BuildResult, Failure>;
}
```

The synchronous `WebCompiler::build` remains for non-async library callers and
drives the same Phase 3 pipeline with a current-thread runtime. The compiled
CLI and development server use `build_async`.

`BuildOptions` gains `incremental: bool`, defaulting to true. `BuildResult`
adds `compiled_routes()` and `reused_routes()`.

## Vertical Slices

1. Add typed View IR and View Source Map contracts, then compile and render
   interpolation over a fixed JSON context.
2. Add the bounded expression parser and lower/render both conditional and
   iteration syntaxes.
3. Discover, parse, lower, and expand components with properties, slots, cycle
   checks, and expansion budgets.
4. Extend Handler Protocol v1 operation handling for typed `view.context`
   contributions and compose all route handlers.
5. Emit deterministic island wrappers, component modules, and the external
   activation runtime.
6. Emit View IR/source maps, then add verified incremental reuse and atomic
   build state.
7. Add multi-diagnostic recovery, adversarial tests, real-browser checks, CI,
   security documentation, and completion evidence.

Every slice preserves the prior published output on failure.

## Black-Box Acceptance Corpus

The compiled `ty` executable must prove:

- JavaScript and Python class fields and async GET results render one Yon page;
- both control syntaxes, nested loops, escaped bindings, dynamic attributes,
  web components, component properties, and slots render correctly;
- View IR and source maps validate against their canonical schemas;
- every hydration policy has deterministic markup and module behavior;
- a real Chromium session preserves SSR, schedules activation, replays an
  interaction, and marks failure without blanking content;
- duplicate context, malformed expressions/controls, component cycles,
  invalid slots/islands, expansion limits, and missing values fail safely;
- two independent source errors appear in one Diagnostics v1 report;
- unchanged static routes reuse verified output, component changes invalidate
  consumers, handler routes rerender, corrupt state misses safely, and
  `--no-incremental` forces recompilation;
- failed builds retain the complete prior output.

## Security and Operational Review

- No template path reaches JavaScript evaluation or a shell.
- Context and component values are escaped at their output context.
- View IR, source maps, manifests, diagnostics, and incremental state exclude
  values and secrets.
- Island module URLs are compiler-generated and checked as same-origin.
- Props are public and cannot authorize a request.
- Handler execution retains the Phase 2 process and environment limits.
- Cache state is untrusted, path-contained, digest-verified, and disposable.
- Parser, evaluator, expansion, output, diagnostics, handler, and cache reads
  are bounded.
