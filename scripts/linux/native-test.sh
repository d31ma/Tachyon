#!/usr/bin/env bash
# Execute the generated WebKitGTK application, not the retired widget model.
set -euo pipefail

FIXTURE=$(mktemp -d "${TMPDIR:-/tmp}/ty-linux-native.XXXXXX")
trap 'rm -rf "$FIXTURE"' EXIT
OUT="${FIXTURE}/dist/linux"
APP="${OUT}/NativeGate"
python3 scripts/native/desktop-fixture.py "$FIXTURE" linux

if [[ -z "${TAC_BIN:-}" ]]; then
  cargo build --locked --bin ty
  TAC_BIN="${CARGO_TARGET_DIR:-target}/debug/ty"
fi
echo '==> building the Linux WebView host and two-route Rust companion'
timeout 240s "$TAC_BIN" build "$FIXTURE" --target linux
test -x "$APP/bin/NativeGate"
test -f "$APP/bin/libtachyoncompanion.so"
test -f "$APP/resources/NativeIndex.json"
test -f "$APP/resources/WebBundle/index.html"
test -f "$APP/resources/WebBundle/items/_id/index.html"
test -f "$APP/resources/WebBundle/shared/native-gate.js"
test -f "$OUT/tachyon.host.json"
test -f "$OUT/artifact-manifest.json"
cmp "$OUT/web/index.html" "$APP/resources/WebBundle/index.html"
python3 - "$OUT/tachyon.host.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
    host = json.load(source)
assert host['schemaVersion'] == 3 and host['renderMode'] == 'bundle', host
assert len(host['companions']) == 2, host
PY

echo '==> driving rendered web controls through the real accessibility bus'
export XDG_STATE_HOME="$FIXTURE/state"
export XDG_DATA_HOME="$FIXTURE/data"
export GDK_BACKEND=x11
export GTK_A11Y=atspi
# Document transitions still need WebKit's compositor on headless Xvfb.
# Use Mesa software rendering and shared-memory buffers, not disabled backing stores.
export LIBGL_ALWAYS_SOFTWARE=1
export WEBKIT_DMABUF_RENDERER_FORCE_SHM=1
export APP
timeout 150s xvfb-run --auto-servernum --server-args='-screen 0 1024x1400x24' \
  dbus-run-session -- bash -c '
    set -euo pipefail
    "$APP/bin/NativeGate" &
    host=$!
    trap '\''kill "$host" 2>/dev/null || true; wait "$host" 2>/dev/null || true'\'' EXIT
    timeout 120s python3 scripts/linux/a11y-probe.py
  '

LOG="$XDG_STATE_HOME/tachyon/dev.tachyon.desktop-gate.jsonl"
test -f "$LOG"
for event in controller.created controller.mounted controller.active companion.loaded; do
  grep -q "\"event\":\"$event\"" "$LOG"
done
echo 'PASS: Linux WebView, shared JS/CSS, native ABI, publish, dynamic routes, and route isolation'
