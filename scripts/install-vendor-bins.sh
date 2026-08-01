#!/bin/sh
# Install the `chex` and `ttid` binaries Tachyon drives at runtime.
#
# Tachyon is binary-first: it consumes CHEX (schema validation) and TTID
# (identifier generation) as standalone binaries from their GitHub Releases,
# spawned by the vendored shims in src/vendor/. Each project's installer
# downloads the right binary for your OS/arch, verifies its checksum, and puts
# it on your PATH.
#
# Windows: run the PowerShell installers instead:
#   irm https://github.com/d31ma/Chex/releases/latest/download/install.ps1 | iex
#   irm https://github.com/d31ma/TTID/releases/latest/download/install.ps1 | iex
set -eu

curl -fsSL https://github.com/d31ma/Chex/releases/latest/download/install.sh | sh
curl -fsSL https://github.com/d31ma/TTID/releases/latest/download/install.sh | sh

echo "Verifying binaries are on PATH..."
chex --help >/dev/null 2>&1 && echo "  chex OK" || echo "  chex NOT on PATH — add its install dir (e.g. ~/.local/bin) to PATH" >&2
ttid --help >/dev/null 2>&1 && echo "  ttid OK" || echo "  ttid NOT on PATH — add its install dir (e.g. ~/.local/bin) to PATH" >&2
