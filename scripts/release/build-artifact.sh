#!/usr/bin/env bash
# Builds one release artifact for the host target.
#
# The artifact is a directory plus an archive, a manifest recording every
# input that determines the output, and a checksum file. Everything a third
# party needs to verify the artifact independently is inside it.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-${REPO_ROOT}/target/release-artifact}"
VERSION="$(tr -d '[:space:]' < "${REPO_ROOT}/VERSION")"
TOOLCHAIN="$(grep -oE '"[0-9]+\.[0-9]+\.[0-9]+"' "${REPO_ROOT}/rust-toolchain.toml" | tr -d '"' | head -1)"
# A fixed timestamp keeps the archive reproducible.
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-0}"

TARGET="${TACHYON_RELEASE_TARGET:-}"
if [[ "${TARGET}" == *apple-darwin ]]; then OS=macos
elif [[ "${TARGET}" == *linux* ]]; then OS=linux
elif [[ "${TARGET}" == *windows* ]]; then OS=windows
else
  case "$(uname -s)" in Darwin) OS=macos ;; Linux) OS=linux ;; *) OS=windows ;; esac
fi
if [[ "${TARGET:-$(uname -m)}" == aarch64* || "${TARGET:-$(uname -m)}" == arm64* ]]; then
  ARCH=aarch64
else
  ARCH=x86_64
fi
NAME="tachyon-${VERSION}-${ARCH}-${OS}"
STAGE="${OUT}/${NAME}"

echo "==> building ty ${VERSION} for ${ARCH}-${OS}"
cd "${REPO_ROOT}"
BUILD_ARGS=(build --release --locked --bin ty)
if [[ -n "${TARGET}" ]]; then BUILD_ARGS+=(--target "${TARGET}"); fi
if [[ "${TACHYON_AUDITABLE:-0}" == "1" ]]; then
  cargo auditable "${BUILD_ARGS[@]}"
else
  cargo "${BUILD_ARGS[@]}"
fi

rm -rf "${OUT}"
mkdir -p "${STAGE}/bin"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
if [[ -n "${TARGET}" ]]; then TARGET_DIR="${TARGET_DIR}/${TARGET}"; fi
BINARY=ty
if [[ "${OS}" == windows ]]; then BINARY=ty.exe; fi
cp "${TARGET_DIR}/release/${BINARY}" "${STAGE}/bin/${BINARY}"
chmod 0755 "${STAGE}/bin/${BINARY}"
cp LICENSE NOTICE "${STAGE}/"
cp docs/SUPPORT_TIERS.md "${STAGE}/SUPPORT_TIERS.md"

cat > "${STAGE}/INSTALL.md" <<EOF
# Tachyon ${VERSION}

Install by copying \`bin/${BINARY}\` onto your PATH.

See https://tachyon.del.ma/docs/installation for platform-specific steps.

Verify the artifact before installing:

    shasum -a 256 -c SHA256SUMS

Uninstall by removing that file. The CLI artifact writes nothing else.
See SUPPORT_TIERS.md for the supported target contract.
EOF

echo "==> recording the manifest"
COMMIT="$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || printf '%040d' 0)"
cat > "${STAGE}/manifest.json" <<EOF
{
  "contract_version": 1,
  "release_version": "${VERSION}",
  "commit": "${COMMIT}",
  "source_date_epoch": ${SOURCE_DATE_EPOCH},
  "target": { "os": "${OS}", "architecture": "${ARCH}" },
  "toolchains": [{ "name": "rust", "version": "${TOOLCHAIN}" }]
}
EOF

echo "==> checksumming every published file"
cd "${STAGE}"
if command -v sha256sum > /dev/null 2>&1; then
  find . -type f ! -name SHA256SUMS -print0 | sort -z \
    | xargs -0 sha256sum > SHA256SUMS
else
  find . -type f ! -name SHA256SUMS -print0 | sort -z \
    | xargs -0 shasum -a 256 > SHA256SUMS
fi

cd "${OUT}"
# A reproducible archive: sorted entries, fixed ownership and timestamps. GNU
# tar and bsdtar spell these differently, so the flavor is detected rather
# than assumed.
FILES="$(cd "${OUT}" && find "${NAME}" -type f | LC_ALL=C sort)"
if tar --version 2>/dev/null | grep -q GNU; then
  printf '%s\n' "${FILES}" | tar --format=ustar \
      --owner=0 --group=0 --numeric-owner \
      --mtime="@${SOURCE_DATE_EPOCH}" \
      -cf "${NAME}.tar" -T -
else
  printf '%s\n' "${FILES}" | tar --format=ustar \
      --uid 0 --gid 0 --numeric-owner \
      -cf "${NAME}.tar" -T -
fi
gzip -9 -n -f "${NAME}.tar"

echo "==> artifact"
echo "${OUT}/${NAME}.tar.gz"
if command -v sha256sum > /dev/null 2>&1; then
  sha256sum "${NAME}.tar.gz"
else
  shasum -a 256 "${NAME}.tar.gz"
fi
