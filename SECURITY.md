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

The Rust rewrite has completed engineering phases 0–7, but version
`26.35.07` remains a release candidate with no supported or preview tier.
Supported released versions of the existing implementation are listed in the
public release notes. See `docs/SUPPORT_TIERS.md` for the evidence required to
promote a Rust target.

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
