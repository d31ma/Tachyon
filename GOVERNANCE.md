# Tachyon Governance

## Principles

Tachyon is governed through public technical evidence, documented decisions,
and maintainership proportional to sustained contribution. No title overrides a
contract, test, security gate, or release requirement.

## Roles

- **Contributors** propose and implement changes.
- **Maintainers** review changes, own subsystems, and keep CI and documentation
  healthy.
- **Release maintainers** control protected release environments, signing
  identities, and support promotion.
- **Security maintainers** receive private reports and coordinate disclosure.

One person may hold multiple roles, but release approval and security-sensitive
changes should receive independent review whenever the contributor pool allows.

## Decision Process

- Routine reversible changes use pull-request review.
- Hard-to-reverse internal decisions use ADRs.
- New or breaking public behavior uses RFCs.
- Security-sensitive behavior updates the threat model.
- Platform promotion requires the evidence in `docs/SUPPORT_TIERS.md`.

When consensus is unavailable, maintainers record the alternatives, evidence,
decision owner, and appeal path. Silent or undocumented authority is not a
decision mechanism.

## Compatibility

Stable behavior is changed only with documented migration, compatibility
fixtures, and an appropriate deprecation window. Security fixes may accelerate
that window when continued compatibility would preserve a vulnerability; the
release notes must explain the exception.

## Maintainer Changes

Maintainers are added after sustained, high-quality contribution and dependable
review. Inactive maintainers may move to emeritus status without losing credit.
Access is removed promptly after departure or compromise and reviewed
periodically under least privilege.
