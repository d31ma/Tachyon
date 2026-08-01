# Phase 4 Implementation Plan

## Architecture

Phase 4 adds a `native` module to `tachyon-core`; it does not add a crate.
`NativeCompiler` composes the existing web compiler, then plans native routes
from the fully rendered documents. This keeps discovery, Yon context,
component expansion, escaping, diagnostics, and rollback behavior on one
tested path.

The new data path is:

```text
ProjectDiscovery
  -> WebCompiler temporary resolved bundle
  -> NativePlanner
  -> Native UI v1 + WebSurface artifacts
  -> MacOsHostGenerator
  -> swiftc
  -> staged .app + manifests
  -> atomic publication
```

The temporary web bundle lives under a guarded project-local temporary
directory and is deleted after planning.

## Interfaces

```rust
pub enum BuildTarget {
    Web,
    Macos,
}

pub struct NativeBuildOptions {
    pub output_directory: PathBuf,
}

pub struct NativeBuildResult {
    pub output_directory: PathBuf,
    pub application_bundle: PathBuf,
    pub route_count: usize,
    pub native_node_count: usize,
    pub web_surface_count: usize,
    pub sha256: String,
}

pub struct NativeCompiler;

impl NativeCompiler {
    pub fn build(
        project_root: impl AsRef<Path>,
        options: &NativeBuildOptions,
    ) -> Result<NativeBuildResult, Failure>;

    pub async fn build_async(
        project_root: impl AsRef<Path>,
        options: &NativeBuildOptions,
    ) -> Result<NativeBuildResult, Failure>;
}
```

Internal boundaries:

```rust
struct NativePlanner;

impl NativePlanner {
    fn plan(
        route: &str,
        source_path: &str,
        rendered_html: &str,
    ) -> Result<PlannedNativeRoute, Failure>;
}

struct MacOsHostGenerator;

impl MacOsHostGenerator {
    async fn generate(
        application: &NativeApplication,
        routes: &[PlannedNativeRoute],
        web_bundle: &Path,
        stage: &Path,
    ) -> Result<GeneratedMacOsHost, Failure>;
}
```

Canonical serializable types for Native UI v1, Capability Manifest v1, and
Artifact Manifest v1 live in `tachyon-contracts`.

## Vertical Slices

1. Add failing compiled-binary tests for the CLI, output contracts, fallback,
   accessibility, and rollback.
2. Add canonical Rust contract types and schema-compatible wire tests.
3. Implement strict native configuration and resolved-tree planning.
4. Implement declarative state validation and WebSurface document generation.
5. Generate deterministic SwiftUI source, resources, Info.plist, and manifests.
6. Compile and ad-hoc sign a real macOS application in a staged output.
7. Add the macOS black-box interaction, accessibility, and screenshot
   comparison runner.
8. Update CI, threat model, support language, architecture, release notes, and
   the Phase 4 evidence ledger.

## Recovery and Security

Every external tool is spawned directly with fixed arguments. Tool output is
bounded before it enters a diagnostic. Native publication reuses the existing
staged rename-and-rollback primitive. The previous native output remains
untouched until Swift compilation, resource generation, contract validation,
signing, and manifest hashing all succeed.

WebSurface decisions are data, not generated Swift branches. The host decodes
Native UI v1 and applies a fixed adapter allowlist. Remote content never
receives a message handler. Local navigation is contained to the application
resource root.

## Explicit Deferrals

- iOS, Android, Windows, and Linux native hosts;
- third-party native adapter registration;
- arbitrary Tac controller JavaScript on native;
- native capabilities other than deny;
- signed release identities, notarization, installers, updates, and stable
  distribution;
- pixel-identical rendering across different platform control toolkits.
