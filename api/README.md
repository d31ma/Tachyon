# Tachyon Machine Contracts

This directory is the canonical source for Tachyon's versioned machine
contracts. Code, generated artifacts, documentation, and examples must agree
with these schemas.

Each contract has:

- an immutable major-version directory;
- a Draft 2020-12 JSON Schema with a stable `$id`;
- one accepted example;
- one rejected example proving that validation is active.

Within a major version, changes must be backward-compatible and additive.
Breaking changes require an accepted RFC, a migration path, a deprecation
window, compatibility fixtures, and a new major-version directory.

The `tachyon-contracts` crate embeds this corpus. Its tests validate every
schema against its meta-schema and prove both examples against the compiled
validator.
