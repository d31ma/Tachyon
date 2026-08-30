# Tachyon Support

## Current Release Status

The Rust rewrite has completed engineering phases 0–7 and subsequent client,
server-isolation, and native-runtime work. Version `26.35.07` is a release
candidate, not a supported or preview application-framework release. There is
no compatibility, uptime, security-support, or production-use promise for its
artifacts. Current implementation evidence and remaining promotion gates are
recorded in `docs/PROJECT_PLAN.md`, `docs/CUTOVER.md`, and
`docs/SUPPORT_TIERS.md`.

Use:

- GitHub issues for reproducible public bugs and feature proposals;
- GitHub discussions for usage and architecture questions;
- private vulnerability reporting for security issues.

Before opening an issue, include the Tachyon version or commit, operating
system, architecture, relevant toolchain versions, minimal reproduction, actual
result, expected result, and sanitized diagnostics.

Platform status uses only the vocabulary in `docs/SUPPORT_TIERS.md`. A platform
listed as buildable or simulator-tested must not be represented as supported.
