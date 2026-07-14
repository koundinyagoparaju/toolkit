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
# This script is also saved next to the binary as toolkit-update.ps1 (see
# the end); when running as that copy, update the installation it belongs
# to — an explicit TOOLKIT_INSTALL_DIR still wins.
$InstallDir = if ($env:TOOLKIT_INSTALL_DIR) {
    $env:TOOLKIT_INSTALL_DIR
} elseif ($PSCommandPath -and (Split-Path -Leaf $PSCommandPath) -eq "toolkit-update.ps1") {
    Split-Path $PSCommandPath
} else {
    Join-Path $env:LOCALAPPDATA "toolkit\bin"
}

# x86_64 only; on Windows-on-ARM the x64 binary runs through emulation.
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq "ARM64") {
    Write-Host "note: no native ARM64 build yet; installing x86_64 (runs via emulation)"
} elseif ($arch -ne "AMD64") {
    throw "unsupported architecture: $arch (build from source: cargo install --path crates/cli)"
}
$Asset = "toolkit-windows-x86_64.zip"

Write-Host "checking latest release of $Repo..."
# The latest tag comes from this URL's redirect (.../releases/tag/<tag>),
# not from api.github.com: the API allows 60 unauthenticated requests per
# hour per IP, which shared IPs (CI runners, corporate NAT) blow through,
# turning installs into 403s. The redirect is not API-rate-limited.
$resp = Invoke-WebRequest -Method Head -UseBasicParsing "https://github.com/$Repo/releases/latest"
# Windows PowerShell 5.1 exposes the post-redirect URI as ResponseUri;
# PowerShell 7+ as RequestMessage.RequestUri.
$finalUri = if ($resp.BaseResponse.ResponseUri) {
    $resp.BaseResponse.ResponseUri
} else {
    $resp.BaseResponse.RequestMessage.RequestUri
}
$tag = $finalUri.Segments[-1].Trim('/')
if (-not $tag -or $tag -in @("latest", "releases")) { throw "could not determine the latest release" }

# Compare against the binary this run would replace — not whatever
# `toolkit` PATH happens to find, which may be a different install.
$installedExe = Join-Path $InstallDir "toolkit.exe"
if (Test-Path $installedExe) {
    $current = (& $installedExe --version) -replace '^toolkit\s+', ''
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

    # Windows locks a running executable (e.g. an MCP server running from
    # it), so copying over it fails — but renaming it aside is allowed.
    # Move the old exe to .old, copy the new one in, and restore the .old
    # if anything goes wrong in between.
    $exe = Join-Path $InstallDir "toolkit.exe"
    $oldExe = "$exe.old"
    # Sweep a leftover .old from a previous update; harmless if it's
    # still running and can't be deleted yet.
    Remove-Item -Force $oldExe -ErrorAction SilentlyContinue
    $movedAside = $false
    try {
        if (Test-Path $exe) {
            Move-Item -Force $exe $oldExe
            $movedAside = $true
        }
        Copy-Item -Force (Join-Path $tmp "toolkit.exe") $exe
    } catch {
        if ($movedAside -and -not (Test-Path $exe)) {
            Move-Item -Force $oldExe $exe
        }
        throw
    }
    Remove-Item -Force $oldExe -ErrorAction SilentlyContinue

    # Save this tag's copy of this script as toolkit-update, so the next
    # update is one command with no URL to remember (.cmd shim so it runs
    # from any shell without execution-policy friction).
    try {
        Invoke-WebRequest "https://raw.githubusercontent.com/$Repo/$tag/scripts/install.ps1" -OutFile (Join-Path $tmp "updater.ps1")
        Move-Item -Force (Join-Path $tmp "updater.ps1") (Join-Path $InstallDir "toolkit-update.ps1")
        Set-Content (Join-Path $InstallDir "toolkit-update.cmd") "@powershell -NoProfile -ExecutionPolicy Bypass -File `"%~dp0toolkit-update.ps1`" %*"
        Write-Host "to update later, just run: toolkit-update"
    } catch {
        Write-Host "note: could not save the updater; to update, re-run this script"
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host "installed $tag to $InstallDir\toolkit.exe"

# Refresh completions if previously set up (never creates new config).
# To set up once:  toolkit completions powershell > "$env:LOCALAPPDATA\toolkit\completions.ps1"
#                  Add-Content $PROFILE '. "$env:LOCALAPPDATA\toolkit\completions.ps1"'
$completionsFile = Join-Path (Split-Path $InstallDir) "completions.ps1"
if (Test-Path $completionsFile) {
    try {
        & (Join-Path $InstallDir "toolkit.exe") completions powershell | Set-Content $completionsFile
        Write-Host "refreshed PowerShell completions at $completionsFile"
    } catch {
        Write-Host "note: could not refresh completions: $_"
    }
}

# Add the install dir to the *user* PATH (no admin needed) if missing —
# unlike ~/.local/bin on Unix, nothing on Windows puts it there for you.
# (Guarded so the script is also testable on non-Windows PowerShell.)
if ($env:OS -eq "Windows_NT") {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (($userPath -split ';') -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallDir", "User")
        Write-Host "added $InstallDir to your user PATH (undo: Settings > Environment Variables)"
        Write-Host "open a new terminal, then run: toolkit tools"
    } else {
        Write-Host "run: toolkit tools"
    }
}
