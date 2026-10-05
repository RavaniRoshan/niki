# NIKI Windows installer (PowerShell)
# Downloads the matching release archive, verifies SHA256 against sha256.sum,
# and installs the niki binary.
#
# Usage:
#   irm https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.ps1 | iex
#
param(
    [string]$Tag = "",
    [string]$InstallDir = ""
)

$ErrorActionPreference = "Stop"
$Repo = "RavaniRoshan/niki"
$Target = "x86_64-pc-windows-msvc"
$Asset = "niki-$Target.zip"
$Sums = "sha256.sum"

if (-not $Tag) {
    if ($env:NIKI_VERSION) {
        $Tag = $env:NIKI_VERSION
    } else {
        Write-Host "Resolving latest NIKI release..."
        $ReleaseUrl = "https://api.github.com/repos/$Repo/releases/latest"
        $Release = Invoke-RestMethod -Uri $ReleaseUrl -Headers @{ "User-Agent" = "niki-installer" }
        $Tag = $Release.tag_name
    }
}

if (-not $Tag) {
    Write-Error "Could not determine release tag"
    exit 1
}

Write-Host "Release: $Tag (target $Target)"

$BaseUrl = "https://github.com/$Repo/releases/download/$Tag"
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("niki-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

try {
    $AssetPath = Join-Path $TempDir $Asset
    $SumsPath = Join-Path $TempDir $Sums

    Write-Host "Downloading $Asset ..."
    Invoke-WebRequest -Uri "$BaseUrl/$Asset" -OutFile $AssetPath -UseBasicParsing
    Invoke-WebRequest -Uri "$BaseUrl/$Sums" -OutFile $SumsPath -UseBasicParsing

    # Verify SHA256 checksum
    $ActualHash = (Get-FileHash -Path $AssetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $SumsContent = Get-Content -Path $SumsPath
    $ExpectedHash = $null

    foreach ($line in $SumsContent) {
        if ($line -match "([0-9a-fA-F]{64})\s+\*?$([regex]::Escape($Asset))") {
            $ExpectedHash = $matches[1].ToLowerInvariant()
            break
        }
    }

    if (-not $ExpectedHash) {
        Write-Error "Could not find checksum for $Asset in $Sums"
        exit 1
    }

    if ($ActualHash -ne $ExpectedHash) {
        Write-Error "Checksum mismatch for $Asset (expected $ExpectedHash, got $ActualHash)"
        exit 1
    }

    Write-Host "Checksum verified: $ActualHash"

    # Extract
    $ExtractDir = Join-Path $TempDir "extracted"
    Expand-Archive -Path $AssetPath -DestinationPath $ExtractDir -Force

    $NikiExe = Get-ChildItem -Path $ExtractDir -Filter "niki.exe" -Recurse | Select-Object -First 1
    if (-not $NikiExe) {
        Write-Error "Extracted archive did not contain niki.exe"
        exit 1
    }

    # Determine destination directory
    $Dest = $InstallDir
    if (-not $Dest -and $env:NIKI_INSTALL_DIR) {
        $Dest = $env:NIKI_INSTALL_DIR
    }
    if (-not $Dest -and $env:LOCALAPPDATA) {
        $Dest = Join-Path $env:LOCALAPPDATA "niki\bin"
    }
    if (-not $Dest) {
        $Dest = Join-Path $HOME ".niki\bin"
    }

    New-Item -ItemType Directory -Path $Dest -Force | Out-Null
    Copy-Item -Path $NikiExe.FullName -Destination (Join-Path $Dest "niki.exe") -Force

    # Also copy niki-shell.exe if present in archive
    $ShellExe = Get-ChildItem -Path $ExtractDir -Filter "niki-shell.exe" -Recurse | Select-Object -First 1
    if ($ShellExe) {
        Copy-Item -Path $ShellExe.FullName -Destination (Join-Path $Dest "niki-shell.exe") -Force
    }

    Write-Host ""
    Write-Host "Installed niki to (Join-Path $Dest 'niki.exe')"
    & (Join-Path $Dest "niki.exe") --version

    $UserPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")
    if ($UserPath -notlike "*$Dest*") {
        Write-Host ""
        Write-Host "NOTE: $Dest is not on your PATH."
        Write-Host "To add it to your user PATH permanently in PowerShell, run:"
        Write-Host "  [Environment]::SetEnvironmentVariable('Path', `$env:Path + ';$Dest', 'User')"
    }
}
finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
