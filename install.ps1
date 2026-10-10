# NIKI installer for Windows (PowerShell).
# Native Windows is labelled UNSUPPORTED in v1 (see README / niki doctor);
# this script exists so the I1 Windows path is a *verified* manual install
# (checksum before execution) instead of an unchecked download. WSL users:
# use install.sh inside WSL instead.
#
#   .\install.ps1                install to %LOCALAPPDATA%\nikicode\bin
#   .\install.ps1 -Uninstall     remove the binary
#   .\install.ps1 -BaseURL ...   override the release asset base (tests)
param(
    [switch]$Uninstall,
    [string]$BaseURL = "https://github.com/RavaniRoshan/niki/releases/latest/download",
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "nikicode\bin")
)
$ErrorActionPreference = 'Stop'

$arch = $env:PROCESSOR_ARCHITECTURE
switch ($arch) {
    "AMD64" { $goarch = "amd64" }
    "ARM64" { $goarch = "arm64" }
    default {
        Write-Error "Unsupported architecture: $arch. Fix: x64 or arm64 Windows required."
        exit 1
    }
}
# Release assets: windows_amd64.zip / windows_arm64.zip (goreleaser name_template)
if ($goarch -eq "amd64") { $asset = "windows_amd64.zip" } else { $asset = "windows_arm64.zip" }
$binary = "nikicode.exe"

if ($Uninstall) {
    $removed = 0
    foreach ($name in @("nikicode.exe", "nc.exe", "niki.exe")) {
        $path = Join-Path $InstallDir $name
        if (Test-Path $path) {
            Remove-Item -Force $path
            Write-Host "removed $path"
            $removed++
        }
    }
    Write-Host "Uninstalled $removed entries from $InstallDir."
    exit 0
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("niki-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $zipUrl = "$BaseURL/$asset"
    $sumUrl = "$BaseURL/checksums.txt"
    $zipPath = Join-Path $tmp $asset
    Write-Host "Installing NikiCode for windows/$goarch..."
    Invoke-WebRequest -Uri $sumUrl -OutFile (Join-Path $tmp "checksums.txt")
    Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath

    # Verify the checksum BEFORE the archive is expanded or anything runs.
    $manifest = Get-Content (Join-Path $tmp "checksums.txt") |
        ForEach-Object { ($_ -split "\s+") } | Where-Object { $_ -ne "" }
    $expected = $null
    for ($i = 0; $i -lt $manifest.Count - 1; $i += 2) {
        if ($manifest[$i + 1] -eq $asset) { $expected = $manifest[$i]; break }
    }
    if (-not $expected) {
        Write-Error "$asset is not listed in checksums.txt — refusing to install an unverifiable binary."
        exit 1
    }
    $actual = (Get-FileHash -Algorithm SHA256 -Path $zipPath).Hash.ToLower()
    if ($expected.ToLower() -ne $actual) {
        Write-Error "checksum mismatch for $asset — download corrupted or tampered. Expected $expected got $actual. Fix: retry the download."
        exit 1
    }
    Write-Host "checksum OK ($asset)"

    Expand-Archive -Path $zipPath -DestinationPath $tmp -Force
    $src = Join-Path $tmp $binary
    if (-not (Test-Path $src)) {
        Write-Error "Archive did not contain $binary."
        exit 1
    }
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Move-Item -Force -Path $src -Destination (Join-Path $InstallDir $binary)
    Write-Host "INSTALLED_DIR=$InstallDir"
    Write-Host "NikiCode installed to $(Join-Path $InstallDir $binary)."
    $machinePath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($machinePath -notlike "*$InstallDir*") {
        Write-Host "Note: $InstallDir is not in your user PATH."
        Write-Host "Add it with:"
        Write-Host "  [Environment]::SetEnvironmentVariable('Path', `$env:Path + ';$InstallDir', 'User')"
    }
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
