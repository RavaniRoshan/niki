# NIKI startup-time benchmark.
#
# Usage:  pwsh scripts/bench-startup.ps1 [-Runs 50] [-Exe .\target\release\niki.exe]
#
# Prefers hyperfine (statistically robust, -N avoids shell-spawn noise); falls
# back to a plain Measure-Command loop when hyperfine is not installed.
# The <100 ms launch bar comes from the research report (Nielsen perceptibility).
param(
    [int]$Runs = 50,
    [string]$Exe = ""
)

if (-not $Exe) {
    $suffix = if ($IsWindows -or $env:OS -eq 'Windows_NT') { '.exe' } else { '' }
    $Exe = "./target/release/niki$suffix"
}
if (-not (Test-Path $Exe)) {
    Write-Error "binary not found at $Exe — build with cargo build --release first"
    exit 1
}

Write-Host "== NIKI startup benchmark: $Exe =="
& $Exe --version

if (Get-Command hyperfine -ErrorAction SilentlyContinue) {
    Write-Host "`n[hyperfine]"
    & hyperfine -N --warmup 10 --min-runs $Runs "$Exe --version"
    exit $LASTEXITCODE
}

Write-Host "`n[fallback loop] install hyperfine for distribution stats"
$samples = New-Object System.Collections.Generic[List double]
for ($i = 0; $i -lt $Runs; $i++) {
    & $Exe --version *>$null
    $t = (Measure-Command { & $Exe --version *>$null }).TotalMilliseconds
    $samples.Add($t)
}
$samples.Sort()
$median = $samples[[int]($Runs / 2)]
$min = $samples[0]
$p95 = $samples[[int][Math]::Floor($Runs * 0.95) - 1]
Write-Host ("  min={0:N1}ms median={1:N1}ms p95={2:N1}ms" -f $min, $median, $p95)
Write-Host ("  budget check (<100 ms launch): {0}" -f $(if ($median -lt 100) { 'PASS' } else { 'FAIL' }))
