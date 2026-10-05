# NIKI Windows uninstaller (PowerShell)
# Removes installed niki binaries. Leaves configuration and data intact
# unless -Purge is specified.
#
# Usage:
#   .\scripts\uninstall.ps1           # remove binaries, keep data
#   .\scripts\uninstall.ps1 -Purge    # remove binaries and data
#
param(
    [switch]$Purge,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

# Determine install directory
$Dest = $env:NIKI_INSTALL_DIR
if (-not $Dest -and $env:LOCALAPPDATA) {
    $Dest = Join-Path $env:LOCALAPPDATA "niki\bin"
}
if (-not $Dest) {
    $Dest = Join-Path $HOME ".niki\bin"
}

$ConfigDir = if ($env:APPDATA) { Join-Path $env:APPDATA "niki" } else { Join-Path $HOME ".config\niki" }
$StateDir = Join-Path $HOME ".niki"

Write-Host "niki Windows uninstaller"
Write-Host ""

$Binaries = @("niki.exe", "niki-shell.exe", "niki-update.exe")

Write-Host "Installed files"
foreach ($bin in $Binaries) {
    $target = Join-Path $Dest $bin
    if (Test-Path $target) {
        if ($DryRun) {
            Write-Host "  would remove: $target"
        } else {
            Remove-Item -Path $target -Force
            Write-Host "  removed:      $target"
        }
    } else {
        Write-Host "  absent:       $target"
    }
}

Write-Host ""
Write-Host "Your data"
if ($Purge) {
    foreach ($data in @($ConfigDir, $StateDir)) {
        if (Test-Path $data) {
            if ($DryRun) {
                Write-Host "  would purge: $data"
            } else {
                Remove-Item -Path $data -Recurse -Force
                Write-Host "  purged:      $data"
            }
        }
    }
} else {
    Write-Host "  kept config: $ConfigDir"
    Write-Host "  kept state:  $StateDir"
    Write-Host ""
    Write-Host "  Pass -Purge to remove these too. Until then they are left as they are."
}

if ($DryRun) {
    Write-Host "Dry run: nothing was deleted."
} else {
    Write-Host "Done."
}
