# ADR 0005: Versioning, Compatibility, and Support Evidence

- Status: Accepted
- Date: 2026-07-26

## Context

The rewrite must evolve machine contracts, preserve the current developer
workflow where intentional, and eventually replace a released implementation
without confusing build output with platform support.

Tachyon already uses a UTC CalVer product version. Cargo packages require
SemVer-compatible package metadata, while public schemas need independent
major versions.

## Decision

- Retain Tachyon's UTC CalVer identity for product releases.
- Use internal Cargo package versions as implementation metadata.
- Record the exact product version, commit, toolchains, contracts, target,
  inputs, outputs, and digests in Artifact Manifest v1.
- Version each public machine contract independently by major directory.
- Prefer additive changes within a major contract version.
- Require an RFC, migration, compatibility tests, and deprecation window for a
  breaking contract change.
- Preserve current public behavior through neutral compatibility fixtures, not
  by importing legacy internals.
- Use the support vocabulary in `docs/SUPPORT_TIERS.md`.
- Never promote a platform from cross-compilation alone.

During Phase 0, the rewrite version is `0.0.0-phase0` and is not publishable as
a Tachyon preview or stable release.

## Consequences

- Product and internal package versions can evolve without conflation.
- Artifacts are self-describing and independently verifiable.
- Platform marketing claims require evidence owned by release engineering.
- Legacy differences must be intentional, documented, and testable.
- Contract consumers can negotiate compatibility without parsing human text.

## Rejected Alternatives

- **One version for every schema and package**: forces unrelated compatibility
  changes into lockstep.
- **Semantic versioning for the public product immediately**: unnecessary
  disruption to current Tachyon release identity.
- **Support on successful cross-build**: provides no native runtime evidence.
- **Big-bang replacement without dual-run fixtures**: hides regressions until
  cutover.

## Acceptance Gate

- Artifact Manifest v1 records the required provenance fields;
- the release and support documents use the same vocabulary;
- rewrite documentation explicitly distinguishes legacy baseline, preview,
  and stable cutover;
- no Phase 0 workflow publishes a Tachyon release.
