# Tachyon Rust Release Engineering

## Release Boundary

Engineering phases 0–7 implement:

- the compiler;
- the supervised server boundary;
- view semantics and islands;
- five native host families;
- migration tooling;
- automation for enterprise qualification.

`VERSION` records the product release identity. The internal Cargo package
identity stays `0.0.0-phase4`. This way, nobody can mistake package metadata
for the public product version.

Stable Tachyon releases keep the existing product identity, which uses UTC
CalVer (calendar versioning). Internal Rust package versions are
implementation metadata. They do not replace the product version in `VERSION`
and in the artifact manifest. The build embeds `VERSION` at compile time in
`ty --version` and in native artifact manifests. So when the release identity
changes, the executable cannot keep reporting the internal Cargo version.

## Branch Model

- The Rust cutover is complete. `main` is the default integration branch.
- Feature and fix branches use `codex/*`. They are merged through reviewed
  pull requests that target the current default branch.
- Release branches use `release/<UTC-CalVer>`. Create them from the updated
  remote default branch after the implementation is merged. Keep them forever.
- Keep old rewrite branches and local-recovery branches for comparison. Do
  not replace all of main with a recovered source tree.
- Never force-push release history. Never move an existing version tag.

## Release Inputs

Every release comes from:

- an immutable commit and an annotated tag;
- the exact `VERSION`;
- the committed `Cargo.lock`;
- the exact `rust-toolchain.toml`;
- pinned GitHub Actions;
- canonical contract versions;
- clean, temporary native builders;
- an explicit `SOURCE_DATE_EPOCH`.

## Required Artifacts

Each target archive contains:

- `ty` or `ty.exe`;
- license and third-party notices;
- installation notes;
- `manifest.json`;
- contract compatibility metadata.

The release contains:

- SHA-256 checksums that fail closed (any mismatch counts as a failure);
- a CycloneDX SBOM (software bill of materials);
- signed build provenance;
- keyless Sigstore signatures bound to the release workflow;
- release notes for people to read.

## Release CI

The Rust release workflow runs in CI (continuous integration). A tag starts
it, and it never creates its own tag. It:

1. verifies the tag, `VERSION`, the changelog, and the commit state;
2. runs all CI and compatibility gates;
3. builds on native runners for each target;
4. runs the produced binary on each runner;
5. packages deterministic target archives;
6. generates checksums and an SBOM;
7. creates an attestation for every published archive;
8. prepares a draft that is not public, from the tag that already exists;
9. downloads the prepared assets on Linux x64/arm64, macOS x64/arm64, and
   Windows x64;
10. verifies every checksum, GitHub provenance attestation, and Sigstore
    signature;
11. tests each raw binary and the real installer, which fails closed. The
    tests cover schema contracts, the native companion ABI (application
    binary interface), client plans, and production start. The downloaded
    Linux binary also runs the browser/storage and website typing gates
    against the runtime that it emitted;
12. makes the release public only after every verification succeeds.

The workflow never creates or moves a tag. It never overwrites a release
asset. If a verification fails, the draft stays non-public so that people can
investigate.

Cross-building may add to the matrix. It never replaces native evidence for
promotion.

Release qualification always runs on GitHub-hosted runners. `rust-ci.yml`
chooses a self-hosted runner only for pushes to branches when
`TACHYON_SELF_HOSTED` is `true`, and a release tag is not a branch.
[SELF_HOSTED_RUNNERS.md](SELF_HOSTED_RUNNERS.md) describes the self-hosted
runners that maintainers can use for branch CI.

[RECONCILIATION.md](RECONCILIATION.md) lists the local features being
reconciled. Its acceptance gates must pass on the actual downloaded draft
assets, not only on a local developer executable. The existing `v26.35.07`
assets never change. The restored features and the input-focus fix need a new
qualified version.

## Failure Policy

If the gate for one platform fails, downgrade or leave out that platform. Do
not label it wrongly and do not disable the gate. Security fixes for healthy
targets do not have to wait for an unrelated preview platform. However, the
release notes must state the exact reduced matrix.

Never skip a contract, security, upgrade, rollback, packaging, or native
execution test only to keep the release matrix green.

## Rollback

Before a release can be supported:

- installer changes have an uninstall and rollback plan;
- every persistent format has fixtures for forward and backward
  compatibility;
- irreversible migrations are explicit and require a verified backup;
- the previous supported binary can be reinstalled from kept artifacts;
- users can see the rollback limitations before an upgrade begins.
