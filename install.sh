#!/bin/sh
# Tachyon installer for macOS and Linux.
#   curl -fsSL https://tachyon.del.ma/install.sh | sh
# Downloads the right `ty` binary from the latest GitHub release, verifies its
# checksum, installs it to a directory on your PATH, then installs the `fylo`
# and `chex` binaries Tachyon drives at runtime.
set -eu

REPO="d31ma/Tachyon"
BASE="https://github.com/${REPO}/releases/latest/download"
TACHYON_STEPS=7
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
if [ -w /usr/local/bin ]; then
    dest="/usr/local/bin"
else
    dest="${HOME}/.local/bin"
    mkdir -p "$dest"
fi
tachyon_progress "Selected install directory: ${dest}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

tachyon_progress "Downloading ${asset}"
curl -fsSL "$url" -o "$tmp/ty"

# Verify checksum against the release's SHA256SUMS (best-effort: skip if tools absent).
tachyon_progress "Verifying release checksum"
if command -v sha256sum >/dev/null 2>&1; then hash_cmd="sha256sum"; \
    elif command -v shasum >/dev/null 2>&1; then hash_cmd="shasum -a 256"; else hash_cmd=""; fi
if [ -n "$hash_cmd" ]; then
    curl -fsSL "${BASE}/SHA256SUMS" -o "$tmp/SHA256SUMS" || true
    if [ -f "$tmp/SHA256SUMS" ]; then
        expected=$(grep " ${asset}\$" "$tmp/SHA256SUMS" | awk '{print $1}')
        actual=$($hash_cmd "$tmp/ty" | awk '{print $1}')
        if [ -n "$expected" ] && [ "$expected" != "$actual" ]; then
            echo "Checksum mismatch for ${asset}. Aborting." >&2
            exit 1
        fi
    fi
fi

tachyon_progress "Installing ty"
chmod +x "$tmp/ty"
mv "$tmp/ty" "$dest/ty"
echo "Installed ty to ${dest}/ty"

# Tachyon drives the FYLO and CHEX binaries at runtime — install them too.
tachyon_progress "Installing FYLO runtime"
curl -fsSL https://github.com/d31ma/Fylo/releases/latest/download/install.sh | sh
tachyon_progress "Installing CHEX validator"
curl -fsSL https://github.com/d31ma/Chex/releases/latest/download/install.sh | sh

case ":$PATH:" in
    *":$dest:"*) : ;;
    *) echo "Note: ${dest} is not on your PATH. Add it, e.g.:"; echo "  export PATH=\"${dest}:\$PATH\"" ;;
esac
echo "Run 'ty --help' to get started."
