#!/bin/sh
# toolkit installer/updater. Downloads the latest release binary for your
# platform from GitHub, verifies its SHA-256 checksum, and installs it.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.sh | sh
#   wget -qO-  https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.sh | sh
#
# Piping a script into your shell means trusting it — and this project's
# whole point is that you shouldn't have to. These ~90 lines are the entire
# install path: download the file and read it first, or skip it entirely
# with `cargo install --path crates/cli` from the audited source.
#
# This script is deliberately SEPARATE from the toolkit binary: the binary
# that touches your data contains no network code at all; the thing that
# talks to the network never touches your data.
#
# Environment overrides:
#   TOOLKIT_REPO=owner/name        (default: koundinyagoparaju/toolkit)
#   TOOLKIT_INSTALL_DIR=/some/bin  (default: ~/.local/bin)
set -eu

REPO="${TOOLKIT_REPO:-koundinyagoparaju/toolkit}"
INSTALL_DIR="${TOOLKIT_INSTALL_DIR:-$HOME/.local/bin}"
API="https://api.github.com/repos/$REPO/releases/latest"

# --- fetch helper: curl or wget, whichever exists ---
if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1"; }
    fetch_to() { curl -fsSL -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO- "$1"; }
    fetch_to() { wget -qO "$2" "$1"; }
else
    echo "error: need curl or wget" >&2
    exit 1
fi

# --- sha256 helper: GNU coreutils or BSD/macOS shasum ---
if command -v sha256sum >/dev/null 2>&1; then
    sha_check() { sha256sum -c -; }
elif command -v shasum >/dev/null 2>&1; then
    sha_check() { shasum -a 256 -c -; }
else
    echo "error: need sha256sum or shasum to verify the download" >&2
    exit 1
fi

case "$(uname -s)" in
    Linux)  os="linux" ;;
    Darwin) os="macos" ;;
    *) echo "error: unsupported OS: $(uname -s) — on Windows use scripts/install.ps1 (PowerShell), or build from source: cargo install --path crates/cli" >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64|amd64)  arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) echo "error: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac
asset="toolkit-${os}-${arch}.tar.gz"

echo "checking latest release of $REPO..."
release_json="$(fetch "$API")"
tag="$(printf '%s' "$release_json" | grep -m1 '"tag_name"' | cut -d'"' -f4)"
if [ -z "$tag" ]; then
    echo "error: could not determine the latest release" >&2
    exit 1
fi

if command -v toolkit >/dev/null 2>&1; then
    current="$(toolkit --version 2>/dev/null | awk '{print $2}')"
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
trap 'rm -rf "$tmp"' EXIT INT TERM

echo "downloading $asset..."
fetch_to "$base/$asset" "$tmp/$asset"
fetch_to "$base/SHA256SUMS" "$tmp/SHA256SUMS"

echo "verifying checksum..."
(cd "$tmp" && grep " $asset\$" SHA256SUMS | sha_check)

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
cp "$tmp/toolkit" "$INSTALL_DIR/toolkit"
chmod 0755 "$INSTALL_DIR/toolkit"

echo "installed $tag to $INSTALL_DIR/toolkit"
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "note: add $INSTALL_DIR to your PATH" ;;
esac
