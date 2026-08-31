#!/usr/bin/env bash
# Live Android WebView and Kotlin qualification; requires a booted device.
set -euo pipefail
exec node "$(dirname "${BASH_SOURCE[0]}")/native-test.mjs" "$@"
