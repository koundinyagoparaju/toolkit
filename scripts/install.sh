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
# This script is also saved next to the binary as `toolkit-update` (see
# the end); when running as that copy, update the installation it
# belongs to — an explicit TOOLKIT_INSTALL_DIR still wins.
if [ -z "${TOOLKIT_INSTALL_DIR:-}" ] && [ "$(basename "$0")" = "toolkit-update" ]; then
    INSTALL_DIR="$(cd "$(dirname "$0")" && pwd)"
else
    INSTALL_DIR="${TOOLKIT_INSTALL_DIR:-$HOME/.local/bin}"
fi
# Redirects to …/releases/tag/<tag>; unlike api.github.com it has no
# unauthenticated rate limit, which 403s on shared IPs (CI, NAT).
LATEST="https://github.com/$REPO/releases/latest"

# --- fetch helper: curl or wget, whichever exists ---
if command -v curl >/dev/null 2>&1; then
    fetch_to() { curl -fsSL -o "$2" "$1"; }
    final_url() { curl -fsSLI -o /dev/null -w '%{url_effective}' "$1"; }
elif command -v wget >/dev/null 2>&1; then
    fetch_to() { wget -qO "$2" "$1"; }
    final_url() {
        wget -q --spider --server-response "$1" 2>&1 |
            sed -n 's/^ *[Ll]ocation: *//p' | tail -n1 | tr -d '\r'
    }
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
tag="$(final_url "$LATEST")"
tag="${tag##*/}"
# A repo with no releases redirects elsewhere; don't treat that as a tag.
case "$tag" in
    ""|latest|releases)
        echo "error: could not determine the latest release" >&2
        exit 1 ;;
esac

# Compare against the binary this run would replace — not whatever
# `toolkit` PATH happens to find, which may be a different install.
if [ -x "$INSTALL_DIR/toolkit" ]; then
    current="$("$INSTALL_DIR/toolkit" --version 2>/dev/null | awk '{print $2}')"
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
new="$INSTALL_DIR/.toolkit.new.$$"
trap 'rm -rf "$tmp"; rm -f "$new"' EXIT INT TERM

echo "downloading $asset..."
fetch_to "$base/$asset" "$tmp/$asset"
fetch_to "$base/SHA256SUMS" "$tmp/SHA256SUMS"

echo "verifying checksum..."
(cd "$tmp" && grep " $asset\$" SHA256SUMS | sha_check)

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
# Replace via rename, never copy-over: writing into a running executable
# fails on Linux (ETXTBSY) — e.g. while an MCP server runs from it. The
# rename is atomic and the old binary is untouched until it succeeds, so
# a failure at any earlier step leaves the installed version working.
cp "$tmp/toolkit" "$new"
chmod 0755 "$new"
mv -f "$new" "$INSTALL_DIR/toolkit"

echo "installed $tag to $INSTALL_DIR/toolkit"

# Save this tag's copy of this script as `toolkit-update`, so the next
# update is one command with no URL to remember. Written via rename so
# a currently running toolkit-update never reads a half-written self.
if fetch_to "https://raw.githubusercontent.com/$REPO/$tag/scripts/install.sh" "$new" 2>/dev/null; then
    chmod 0755 "$new"
    mv -f "$new" "$INSTALL_DIR/toolkit-update"
    echo "to update later, just run: toolkit-update"
else
    echo "note: could not save the updater; to update, re-run this script"
fi

# Refresh shell completions that were previously set up, so they always
# match the installed version. Never creates new config: only files that
# already exist are regenerated.
refresh_completions() {
    [ -f "$2" ] || return 0
    if "$INSTALL_DIR/toolkit" completions "$1" > "$2" 2>/dev/null; then
        echo "refreshed $1 completions at $2"
    else
        echo "note: could not refresh $1 completions at $2"
    fi
}
refresh_completions zsh "$HOME/.zsh/completions/_toolkit"
refresh_completions fish "$HOME/.config/fish/completions/toolkit.fish"
refresh_completions bash "$HOME/.local/share/bash-completion/completions/toolkit"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "note: add $INSTALL_DIR to your PATH" ;;
esac
