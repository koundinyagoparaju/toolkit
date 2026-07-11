# toolkit installer/updater for Windows. Downloads the latest release
# binary from GitHub, verifies its SHA-256 checksum, and installs it.
#
# Usage (PowerShell):
#   irm https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.ps1 | iex
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
#   TOOLKIT_REPO=owner/name            (default: koundinyagoparaju/toolkit)
#   TOOLKIT_INSTALL_DIR=C:\some\bin    (default: %LOCALAPPDATA%\toolkit\bin)

$ErrorActionPreference = "Stop"

# Windows PowerShell 5.1 defaults to TLS 1.0, which GitHub rejects.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = if ($env:TOOLKIT_REPO) { $env:TOOLKIT_REPO } else { "koundinyagoparaju/toolkit" }
$InstallDir = if ($env:TOOLKIT_INSTALL_DIR) { $env:TOOLKIT_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "toolkit\bin" }

# x86_64 only; on Windows-on-ARM the x64 binary runs through emulation.
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq "ARM64") {
    Write-Host "note: no native ARM64 build yet; installing x86_64 (runs via emulation)"
} elseif ($arch -ne "AMD64") {
    throw "unsupported architecture: $arch (build from source: cargo install --path crates/cli)"
}
$Asset = "toolkit-windows-x86_64.zip"

Write-Host "checking latest release of $Repo..."
$release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
$tag = $release.tag_name
if (-not $tag) { throw "could not determine the latest release" }

$existing = Get-Command toolkit -ErrorAction SilentlyContinue
if ($existing) {
    $current = (& toolkit --version) -replace '^toolkit\s+', ''
    if ("v$current" -eq $tag) {
        Write-Host "already up to date ($tag)"
        return
    }
    Write-Host "updating from v$current to $tag"
} else {
    Write-Host "installing toolkit $tag"
}

$base = "https://github.com/$Repo/releases/download/$tag"
$tmp = Join-Path ([IO.Path]::GetTempPath()) "toolkit-install-$PID"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    Write-Host "downloading $Asset..."
    Invoke-WebRequest "$base/$Asset" -OutFile (Join-Path $tmp $Asset)
    Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $tmp "SHA256SUMS")

    Write-Host "verifying checksum..."
    $sumsLine = Get-Content (Join-Path $tmp "SHA256SUMS") | Where-Object { $_ -match ([regex]::Escape($Asset) + '$') }
    if (-not $sumsLine) { throw "no checksum for $Asset in SHA256SUMS" }
    $expected = ($sumsLine -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Algorithm SHA256 (Join-Path $tmp $Asset)).Hash.ToLower()
    if ($actual -ne $expected) { throw "checksum mismatch: expected $expected, got $actual" }

    Expand-Archive -Force (Join-Path $tmp $Asset) -DestinationPath $tmp
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item -Force (Join-Path $tmp "toolkit.exe") (Join-Path $InstallDir "toolkit.exe")
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host "installed $tag to $InstallDir\toolkit.exe"

# Add the install dir to the *user* PATH (no admin needed) if missing —
# unlike ~/.local/bin on Unix, nothing on Windows puts it there for you.
# (Guarded so the script is also testable on non-Windows PowerShell.)
if ($env:OS -eq "Windows_NT") {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (($userPath -split ';') -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallDir", "User")
        Write-Host "added $InstallDir to your user PATH (undo: Settings > Environment Variables)"
        Write-Host "open a new terminal, then run: toolkit list"
    } else {
        Write-Host "run: toolkit list"
    }
}
