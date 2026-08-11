#!/bin/sh
# Tachyon installer for macOS and Linux.
#   curl -fsSL https://tachyon.del.ma/install.sh | sh
# Downloads the right `ty` binary from the latest GitHub release, verifies its
# checksum, then installs it to a directory on your PATH.
set -eu

REPO="d31ma/Tachyon"
BASE="${TAC_BASE_URL:-https://github.com/${REPO}/releases/latest/download}"
TACHYON_STEPS=5
tachyon_step=0

repeat_char() {
    char=$1
    count=$2
    out=""
    while [ "$count" -gt 0 ]; do
        out="${out}${char}"
        count=$((count - 1))
    done
    printf "%s" "$out"
}

tachyon_progress() {
    tachyon_step=$((tachyon_step + 1))
    percent=$((tachyon_step * 100 / TACHYON_STEPS))
    filled=$((tachyon_step * 24 / TACHYON_STEPS))
    empty=$((24 - filled))
    bar=$(repeat_char "#" "$filled")
    gap=$(repeat_char "-" "$empty")
    printf "TACHYON [%s%s] %3d%%  %s\n" "$bar" "$gap" "$percent" "$1"
}

printf "TACHYON installer\n"
printf "Bringing the ty binary online...\n\n"

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Darwin) os_tag="macos" ;;
    Linux) os_tag="linux" ;;
    *) echo "Unsupported OS: $os (use install.ps1 on Windows)" >&2; exit 1 ;;
esac

case "$arch" in
    x86_64 | amd64) arch_tag="x64" ;;
    arm64 | aarch64) arch_tag="arm64" ;;
    *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
esac

tachyon_progress "Detected ${os_tag}/${arch_tag}"

asset="ty-${os_tag}-${arch_tag}"
url="${BASE}/${asset}"

# Pick an install dir on PATH we can write to; fall back to ~/.local/bin.
if [ -n "${TAC_INSTALL_DIR:-}" ]; then
    dest="${TAC_INSTALL_DIR}"
    mkdir -p "$dest"
elif [ -w /usr/local/bin ]; then
    dest="/usr/local/bin"
else
    dest="${HOME}/.local/bin"
    mkdir -p "$dest"
fi
tachyon_progress "Selected install directory: ${dest}"

tmp=$(mktemp "${dest}/.ty.download.XXXXXX")
checksums=$(mktemp)
trap 'rm -f "$tmp" "$checksums"' EXIT

tachyon_progress "Downloading ${asset}"
curl -fsSL "$url" -o "$tmp"

# Verification is fail-closed: an unavailable checksum, a missing asset entry,
# or a digest mismatch aborts the install.
tachyon_progress "Verifying release checksum"
if command -v sha256sum >/dev/null 2>&1; then hash_cmd="sha256sum"; \
    elif command -v shasum >/dev/null 2>&1; then hash_cmd="shasum -a 256"; \
    else echo "No SHA-256 utility found. Aborting." >&2; exit 1; fi
curl -fsSL "${BASE}/SHA256SUMS" -o "$checksums"
expected=$(awk -v asset="$asset" '$2 == asset { print $1; exit }' "$checksums")
[ -n "$expected" ] || { echo "No checksum published for ${asset}. Aborting." >&2; exit 1; }
actual=$($hash_cmd "$tmp" | awk '{print $1}')
[ "$expected" = "$actual" ] || { echo "Checksum mismatch for ${asset}. Aborting." >&2; exit 1; }

tachyon_progress "Installing ty"
chmod 0755 "$tmp"
mv "$tmp" "$dest/ty"
echo "Installed ty to ${dest}/ty"

case ":$PATH:" in
    *":$dest:"*) : ;;
    *) echo "Note: ${dest} is not on your PATH. Add it, e.g.:"; echo "  export PATH=\"${dest}:\$PATH\"" ;;
esac
echo "Run 'ty --help' to get started."
