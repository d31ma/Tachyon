# Tachyon Rust Release Engineering

## Release Boundary

Engineering phases 0–7 implement the compiler, supervised server boundary,
view semantics and islands, five native host families, migration tooling, and
enterprise qualification automation. The product release identity
is recorded in `VERSION`; the internal Cargo package identity remains
`0.0.0-phase4` so package metadata cannot be mistaken for the public product
version.

Stable Tachyon releases retain the existing UTC CalVer product identity.
Internal Rust package versions are implementation metadata and do not replace
the product version recorded in `VERSION` and the artifact manifest.
`VERSION` is embedded at compile time in `ty --version` and in native artifact
manifests, so changing the release identity cannot leave the executable on the
internal Cargo version.

## Branch Model

- The Rust cutover is complete; `main` is the default integration branch.
- Feature and fix branches use `codex/*` and land through reviewed pull
  requests targeting the current default branch.
- Release branches use `release/<UTC-CalVer>` and start from the updated
  remote default branch after implementation has landed. Keep them forever.
- Preserve historical rewrite and local-recovery branches for comparison.
  Do not wholesale replace main with a recovered source tree.
- Never force-push release history or move an existing version tag.

## Release Inputs

Every release is derived from:

- an immutable commit and annotated tag;
- exact `VERSION`;
- committed `Cargo.lock`;
- exact `rust-toolchain.toml`;
- pinned GitHub Actions;
- canonical contract versions;
- clean ephemeral native builders;
- explicit `SOURCE_DATE_EPOCH`.

## Required Artifacts

Each target archive contains:

- `ty` or `ty.exe`;
- license and third-party notices;
- installation notes;
- `manifest.json`;
- contract compatibility metadata.

The release contains fail-closed SHA-256 checksums, a CycloneDX SBOM, signed
build provenance, keyless Sigstore signatures bound to the release workflow,
and human-readable notes.

## Release CI

The Rust release workflow is tag-driven and never creates its own tag. It:

1. verifies tag, `VERSION`, changelog, and commit state;
2. runs the complete CI and compatibility gates;
3. builds on native target runners;
4. executes the produced binary on each runner;
5. packages deterministic target archives;
6. generates checksums and an SBOM;
7. attests every published archive;
8. stages a non-public draft from the pre-existing tag;
9. downloads the staged assets on Linux x64/arm64, macOS x64/arm64, and
   Windows x64;
10. verifies every checksum, GitHub provenance attestation, and Sigstore
    signature;
11. exercises each raw binary and the real fail-closed installer, including
    schema contracts, native companion ABI, client plans and production start;
    the downloaded Linux binary also runs browser/storage and website typing
    gates against its own emitted runtime;
12. makes the release public only after every verification succeeds.

The workflow never creates or moves a tag and never overwrites a release
asset. A verification failure leaves a non-public draft for investigation.

Cross-building may supplement the matrix but never substitutes for native
promotion evidence.

The local feature reconciliation inventory is [RECONCILIATION.md](RECONCILIATION.md).
Its acceptance gates must pass on the actual staged download, not just a local
developer executable. Existing `v26.35.07` assets remain immutable; restored
features and the input-focus fix require a new qualified version.

## Failure Policy

If one platform gate fails, downgrade or omit that platform rather than
mislabeling it or disabling the gate. Security fixes for healthy targets need
not wait for an unrelated preview platform, but release notes must state the
exact reduced matrix.

Never skip a contract, security, upgrade, rollback, packaging, or native
execution test merely to keep the release matrix green.

## Rollback

Before a release can be supported:

- installer changes have an uninstall and rollback plan;
- every persistent format has forward and backward compatibility fixtures;
- irreversible migrations are explicit and require backup verification;
- the previous supported binary can be reinstalled from retained artifacts;
- rollback limitations are visible before upgrade begins.
