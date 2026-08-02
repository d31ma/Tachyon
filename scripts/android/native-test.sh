#!/usr/bin/env bash
# Phase 5 Android native gate.
#
# Assembles the generated APK, installs it on a booted emulator or device,
# launches it, and asserts that declared accessible names reached the platform
# accessibility tree and that native interaction drives the bound state.
#
# Requires ANDROID_HOME (or ANDROID_SDK_ROOT), `gradle`, and one booted device.
set -euo pipefail

SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "${SDK}" ]; then
  echo "ANDROID_HOME or ANDROID_SDK_ROOT must be set" >&2
  exit 1
fi
export PATH="${SDK}/platform-tools:${PATH}"

FIXTURE=/tmp/ty-android-fixture
PACKAGE=dev.tachyon.phase_five
BUNDLE_ID=dev.tachyon.phase-five
OUT="${FIXTURE}/dist/android"

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

echo "==> assembling the Android application"
"${CARGO_TARGET_DIR:-target}/debug/ty" build "${FIXTURE}" --target android

echo "==> asserting published layout"
test -f "${OUT}/PhaseFive/PhaseFive.apk"
test -f "${OUT}/PhaseFive/project/app/src/main/AndroidManifest.xml"
test -f "${OUT}/PhaseFive/project/app/src/main/assets/NativeIndex.json"
test -f "${OUT}/artifact-manifest.json"
grep -q '"target": "android"' "${OUT}/native-ui/root.json"

echo "==> installing and launching"
adb wait-for-device
adb install -r "${OUT}/PhaseFive/PhaseFive.apk"
adb shell am force-stop "${PACKAGE}" || true
adb shell run-as "${PACKAGE}" rm -f "files/tachyon/${BUNDLE_ID}.jsonl" || true
adb shell am start -W -n "${PACKAGE}/.MainActivity"
sleep 4

UI=/tmp/ty-android-ui.xml
dump() {
  adb shell uiautomator dump /sdcard/tachyon-ui.xml > /dev/null 2>&1
  adb shell cat /sdcard/tachyon-ui.xml > "${UI}"
}
probe() { python3 scripts/android/ui-probe.py "${UI}" "$@"; }

echo "==> asserting accessible names and native widget classes"
dump
probe names

echo "==> asserting native interaction"
COORD=$(probe locate "Increase count")
test -n "${COORD}"
adb shell input tap ${COORD}
sleep 2
dump
probe expect-text 1

echo "==> lifecycle log"
LOG=$(adb shell run-as "${PACKAGE}" cat "files/tachyon/${BUNDLE_ID}.jsonl")
echo "${LOG}"
for event in controller.created route.opened controller.mounted controller.active \
             websurface.attached state.increment; do
  echo "${LOG}" | grep -q "\"event\":\"${event}\"" || {
    echo "missing lifecycle event ${event}" >&2
    exit 1
  }
done

echo "PASS: Android native gate"
