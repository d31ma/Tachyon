#!/usr/bin/env bash
# Phase 5 Linux native gate.
#
# Compiles `ty` inside the pinned container, generates a GTK4 application from
# a fixture project, launches it headlessly, and asserts that the generated
# host reached its lifecycle states and isolated its WebSurface subtree.
set -euo pipefail

FIXTURE=/tmp/ty-linux-fixture
OUT="${FIXTURE}/dist/linux"
APP="${OUT}/PhaseFive"

rm -rf "${FIXTURE}"
mkdir -p "${FIXTURE}/client/pages"

cat > "${FIXTURE}/tachyon.json" <<'JSON'
{"application":{"name":"Phase Five","id":"dev.tachyon.phase-five","version":"0.1.0","entry_route":"/"}}
JSON

cat > "${FIXTURE}/client/pages/tac.html" <<'HTML'
<main aria-label="Phase Five demo">
  <h1>Phase Five</h1>
  <p>Cross-platform native adapters.</p>
  <button aria-label="Increase count" data-tachyon-action="increment:count">Add one</button>
  <output aria-label="Count" data-tachyon-bind="count" data-tachyon-state="0">0</output>
  <input aria-label="Your name" data-tachyon-bind="name" data-tachyon-state="" placeholder="Name">
  <details aria-label="More detail"><summary>More detail</summary><p>Disclosure content.</p></details>
  <x-chart aria-label="Sales chart"><p>Chart fallback</p></x-chart>
</main>
HTML

echo "==> building ty"
cargo build --locked --bin ty

echo "==> generating the Linux application"
"${CARGO_TARGET_DIR:-target}/debug/ty" build "${FIXTURE}" --target linux

echo "==> asserting published layout"
test -x "${APP}/bin/PhaseFive"
test -f "${APP}/resources/NativeIndex.json"
test -f "${APP}/resources/NativeUI/root.json"
test -f "${APP}/dev.tachyon.phase-five.desktop"
test -f "${OUT}/artifact-manifest.json"
test -f "${OUT}/capability-manifest.json"
test -f "${OUT}/project/tachyon_host.c"
grep -q '"target": "linux"' "${APP}/resources/NativeUI/root.json"
grep -q '"bridge": "none"' "${APP}/resources/NativeUI/root.json"
grep -q "default-src 'none'" "${OUT}"/web-surfaces/*/index.html

echo "==> launching the generated application headlessly"
export XDG_STATE_HOME=/tmp/ty-linux-state
export GDK_BACKEND=x11
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1
rm -rf "${XDG_STATE_HOME}"

# The application is launched under an X server and a session bus so that
# at-spi2-core can publish the accessibility bus the probe reads.
export APP
xvfb-run --auto-servernum --server-args='-screen 0 1024x1400x24' \
  dbus-run-session -- bash -c '
    set -u
    "${APP}/bin/PhaseFive" &
    host=$!
    python3 scripts/linux/a11y-probe.py
    probe=$?
    kill "${host}" 2>/dev/null || true
    wait "${host}" 2>/dev/null || true
    exit "${probe}"
  '

LOG="${XDG_STATE_HOME}/tachyon/dev.tachyon.phase-five.jsonl"
echo "==> lifecycle log"
cat "${LOG}"
grep -q '"event":"controller.created"' "${LOG}"
grep -q '"event":"controller.mounted"' "${LOG}"
grep -q '"event":"controller.active"' "${LOG}"
grep -q '"event":"route.opened"' "${LOG}"
grep -q '"event":"websurface.attached"' "${LOG}"

echo "PASS: Linux native gate"
