# Security Policy

## Reporting a Vulnerability

Use GitHub's private vulnerability reporting for the `d31ma/Tachyon`
repository. Do not open a public issue, discussion, or pull request containing
an unpatched vulnerability, exploit, secret, or private user data.

Include:

- affected commit or release;
- affected target and configuration;
- minimal reproduction or proof;
- expected impact;
- known workarounds;
- whether active exploitation is suspected.

Maintainers will acknowledge a complete report, establish a private
coordination channel, assess affected versions, and communicate a remediation
and disclosure plan. Exact response targets will be published before Tachyon
claims a supported Rust release.

## Supported Versions

The Rust implementation is Tachyon's sole in-tree implementation. Security
fixes apply to the latest immutable release unless its release notes state a
narrower affected range. Platform maturity remains governed by
`docs/SUPPORT_TIERS.md`; publishing a stable CLI release does not promote a
native target to `supported`.

## Security Update Policy

- Critical fixes receive priority over feature work.
- Release artifacts are never silently replaced.
- A corrected release receives a new immutable version and provenance.
- Vulnerable targets may be removed or downgraded independently.
- Workarounds are documented only when they fail closed and do not create a
  larger vulnerability.

## Scope

The initial threat model is `docs/THREAT_MODEL.md`. Particularly sensitive
areas include parsers, filesystem containment, template escaping, handler
supervision, capability bridges, WebSurface isolation, installers, updater
logic, CI, and signing.
