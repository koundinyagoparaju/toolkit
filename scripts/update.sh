#!/usr/bin/env bash
# Updates (or installs) the toolkit CLI from GitHub releases.
#
# This script is deliberately SEPARATE from the toolkit binary: the binary
# that touches your data contains no network code at all, and the thing
# that talks to the network never touches your data. These ~70 lines are
# the entire update path — read them before running.
#
# Usage:   ./update.sh            # install/update to the latest release
#          TOOLKIT_REPO=me/fork ./update.sh
#
# Verifies the SHA-256 checksum shipped with the release before installing.
set -euo pipefail

REPO="${TOOLKIT_REPO:-koundinyagoparaju/toolkit}"
INSTALL_DIR="${TOOLKIT_INSTALL_DIR:-$HOME/.local/bin}"
API="https://api.github.com/repos/$REPO/releases/latest"

case "$(uname -s)" in
    Linux)  os="linux" ;;
    Darwin) os="macos" ;;
    *) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64|amd64)  arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac
asset="toolkit-${os}-${arch}.tar.gz"

echo "checking latest release of $REPO…"
release_json="$(curl -fsSL "$API")"
tag="$(printf '%s' "$release_json" | grep -m1 '"tag_name"' | cut -d'"' -f4)"
[ -n "$tag" ] || { echo "could not determine the latest release" >&2; exit 1; }

if command -v toolkit >/dev/null 2>&1; then
    current="$(toolkit --version | awk '{print $2}')"
    if [ "v$current" = "$tag" ]; then
        echo "already up to date ($tag)"
        exit 0
    fi
    echo "updating from v$current to $tag"
else
    echo "installing toolkit $tag"
fi

base="https://github.com/$REPO/releases/download/$tag"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading $asset…"
curl -fsSL -o "$tmp/$asset" "$base/$asset"
curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS"

echo "verifying checksum…"
(cd "$tmp" && grep " $asset\$" SHA256SUMS | sha256sum -c -)

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
install -m 0755 "$tmp/toolkit" "$INSTALL_DIR/toolkit"

echo "installed $tag to $INSTALL_DIR/toolkit"
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "note: add $INSTALL_DIR to your PATH" ;;
esac
