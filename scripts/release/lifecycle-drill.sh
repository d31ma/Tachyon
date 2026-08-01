#!/usr/bin/env bash
# Release lifecycle drill: verify, install, use, upgrade, roll back, uninstall.
#
# This is the cutover-gate exercise. It runs against real artifacts produced by
# build-artifact.sh, in an isolated prefix, and asserts at every step that the
# installed tool works and that removal leaves nothing behind.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXPECTED_VERSION="$(tr -d '[:space:]' < "${REPO_ROOT}/VERSION")"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/ty-lifecycle-XXXXXX")"
PREFIX="${WORK}/prefix"
trap 'rm -rf "${WORK}"' EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }

checksum() {
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

verify_checksums() {
  if command -v sha256sum > /dev/null 2>&1; then
    (cd "$1" && sha256sum --quiet --check SHA256SUMS)
  else
    (cd "$1" && shasum -a 256 --status --check SHA256SUMS)
  fi
}

echo "==> building two artifacts to exercise an upgrade and a rollback"
"${REPO_ROOT}/scripts/release/build-artifact.sh" "${WORK}/build-a" > /dev/null
FIRST="$(find "${WORK}/build-a" -maxdepth 1 -type d -name 'tachyon-*' | head -1)"
[ -n "${FIRST}" ] || fail "the first artifact was not produced"

echo "==> reproducibility: the same source must produce the same binary"
"${REPO_ROOT}/scripts/release/build-artifact.sh" "${WORK}/build-b" > /dev/null
SECOND="$(find "${WORK}/build-b" -maxdepth 1 -type d -name 'tachyon-*' | head -1)"
if [ "$(checksum "${FIRST}/bin/ty")" != "$(checksum "${SECOND}/bin/ty")" ]; then
  fail "two builds of one commit produced different binaries"
fi
echo "    ok   the binary is bit-identical across builds"

echo "==> verifying the artifact before trusting it"
verify_checksums "${FIRST}" || fail "published checksums did not verify"
echo "    ok   every published file matches SHA256SUMS"

echo "==> tamper detection"
TAMPER="${WORK}/tamper"
cp -R "${FIRST}" "${TAMPER}"
printf '\0' >> "${TAMPER}/bin/ty"
if verify_checksums "${TAMPER}" 2> /dev/null; then
  fail "a modified binary passed checksum verification"
fi
echo "    ok   a modified artifact fails verification"

echo "==> install"
mkdir -p "${PREFIX}/bin"
install -m 0755 "${FIRST}/bin/ty" "${PREFIX}/bin/ty"
"${PREFIX}/bin/ty" --version > /dev/null || fail "the installed binary does not run"
INSTALLED="$("${PREFIX}/bin/ty" --version)"
[ "${INSTALLED}" = "${EXPECTED_VERSION}" ] \
  || fail "installed version '${INSTALLED}' does not match VERSION '${EXPECTED_VERSION}'"
echo "    ok   installed ${INSTALLED}"

echo "==> the installed tool builds a real project"
PROJECT="${WORK}/project"
mkdir -p "${PROJECT}/client/pages"
printf '<main aria-label="Release"><h1>Release</h1></main>' > "${PROJECT}/client/pages/tac.html"
"${PREFIX}/bin/ty" build "${PROJECT}" > /dev/null || fail "the installed binary cannot build"
[ -f "${PROJECT}/dist/index.html" ] || fail "the installed binary published nothing"
echo "    ok   the installed tool built and published a project"

echo "==> upgrade"
# The second artifact stands in for a later release; upgrading must replace the
# binary in place and leave the previous one recoverable.
cp "${PREFIX}/bin/ty" "${WORK}/ty.previous"
install -m 0755 "${SECOND}/bin/ty" "${PREFIX}/bin/ty"
"${PREFIX}/bin/ty" --version > /dev/null || fail "the upgraded binary does not run"
"${PREFIX}/bin/ty" build "${PROJECT}" > /dev/null || fail "the upgraded binary cannot build"
echo "    ok   upgraded in place and still builds"

echo "==> rollback"
install -m 0755 "${WORK}/ty.previous" "${PREFIX}/bin/ty"
"${PREFIX}/bin/ty" --version > /dev/null || fail "the rolled-back binary does not run"
[ "$("${PREFIX}/bin/ty" --version)" = "${INSTALLED}" ] || fail "rollback produced a different version"
"${PREFIX}/bin/ty" build "${PROJECT}" > /dev/null || fail "the rolled-back binary cannot build"
echo "    ok   rolled back to ${INSTALLED} and still builds"

echo "==> uninstall"
rm -f "${PREFIX}/bin/ty"
[ ! -e "${PREFIX}/bin/ty" ] || fail "the binary survived uninstall"
REMAINING="$(find "${PREFIX}" -type f | wc -l | tr -d ' ')"
[ "${REMAINING}" = "0" ] || fail "uninstall left ${REMAINING} file(s) behind"
echo "    ok   uninstall left nothing behind"

echo "PASS: release lifecycle drill"
