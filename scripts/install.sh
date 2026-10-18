#!/bin/sh
# Install the latest neojoplin binary from the GitHub release page.
# Requires: curl, tar, and sha256sum or shasum (for verification).
#
# Usage: install.sh [-d INSTALL_DIR] [-v TAG]
#   INSTALL_DIR  defaults to $NEOJOPLIN_INSTALL_DIR or ~/.local/bin
#   TAG          release tag (e.g. build-12); defaults to the latest release

set -eu

REPO="${NEOJOPLIN_REPO:-Dronakurl/neojoplin}"
INSTALL_DIR="${NEOJOPLIN_INSTALL_DIR:-$HOME/.local/bin}"
TAG=""

err() { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '%s\n' "$*"; }

while getopts "d:v:h" opt; do
    case "$opt" in
        d) INSTALL_DIR="$OPTARG" ;;
        v) TAG="$OPTARG" ;;
        h) sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) exit 2 ;;
    esac
done

command -v curl >/dev/null 2>&1 || err "curl is required but was not found in PATH."
command -v tar >/dev/null 2>&1 || err "tar is required but was not found in PATH."

os=$(uname -s)
arch=$(uname -m)
case "$os" in
    Linux) os_name=linux ;;
    Darwin) os_name=macos ;;
    *) err "unsupported operating system '$os'. Prebuilt binaries exist for Linux and macOS only; build from source instead (see README.md)." ;;
esac
case "$arch" in
    x86_64|amd64) arch_name=x86_64 ;;
    aarch64|arm64) arch_name=aarch64 ;;
    *) err "unsupported CPU architecture '$arch'. Build from source instead (see README.md)." ;;
esac

asset="neojoplin-${os_name}-${arch_name}.tar.gz"
if [ -n "$TAG" ]; then
    base="https://github.com/$REPO/releases/download/$TAG"
else
    base="https://github.com/$REPO/releases/latest/download"
fi

tmp=$(mktemp -d) || err "could not create a temporary directory."
trap 'rm -rf "$tmp"' EXIT INT TERM

info "Downloading $asset from $REPO ${TAG:+(release $TAG)}..."
status=$(curl -sSL -w '%{http_code}' -o "$tmp/$asset" "$base/$asset") \
    || err "download failed (network problem?). URL: $base/$asset"
case "$status" in
    200) ;;
    404) err "no binary '$asset' found at $base. Either there is no release yet, the tag does not exist, or your platform ($os_name/$arch_name) is not published. Build from source instead (see README.md)." ;;
    *) err "download failed with HTTP status $status. URL: $base/$asset" ;;
esac

# Verify the checksum when it is published.
if curl -sSLf -o "$tmp/$asset.sha256" "$base/$asset.sha256" 2>/dev/null; then
    expected=$(cut -d' ' -f1 "$tmp/$asset.sha256")
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$tmp/$asset" | cut -d' ' -f1)
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$tmp/$asset" | cut -d' ' -f1)
    else
        actual=""
        info "warning: neither sha256sum nor shasum found, skipping checksum verification."
    fi
    if [ -n "$actual" ] && [ "$actual" != "$expected" ]; then
        err "checksum mismatch for $asset (expected $expected, got $actual)."
    fi
else
    info "warning: no checksum file published for this release, skipping verification."
fi

tar -xzf "$tmp/$asset" -C "$tmp" || err "could not extract $asset (corrupt download?)."
bin="$tmp/neojoplin-${os_name}-${arch_name}"
[ -f "$bin" ] || err "archive did not contain the expected binary."

mkdir -p "$INSTALL_DIR" || err "could not create $INSTALL_DIR."
[ -w "$INSTALL_DIR" ] || err "$INSTALL_DIR is not writable. Choose another directory with -d, or fix permissions."
install -m 755 "$bin" "$INSTALL_DIR/neojoplin" || err "could not install to $INSTALL_DIR/neojoplin."

info "Installed neojoplin to $INSTALL_DIR/neojoplin"
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) info "note: $INSTALL_DIR is not in your PATH. Add it, e.g. in fish: fish_add_path $INSTALL_DIR" ;;
esac
"$INSTALL_DIR/neojoplin" --version 2>/dev/null || info "warning: installed binary could not be executed on this system."
