# NIKI startup-time benchmark.
#
# Usage:  powershell -ExecutionPolicy Bypass -File scripts/bench-startup.ps1 [-Runs 50]
#
# Prefers hyperfine (statistically robust, -N avoids shell-spawn noise); falls
# back to a Measure-Command loop when hyperfine is not installed.
# The <100 ms launch bar comes from the research report (Nielsen perceptibility).
param(
    [int]$Runs = 50,
    [string]$Exe = ""
)

$ErrorActionPreference = 'Stop'

if (-not $Exe) {
    $suffix = if ($env:OS -eq 'Windows_NT') { '.exe' } else { '' }
    $Exe = "./target/release/niki$suffix"
}
if (-not (Test-Path $Exe)) {
    Write-Error "binary not found at $Exe - build with cargo build --release first"
    exit 1
}

Write-Host "== NIKI startup benchmark: $Exe =="
& $Exe --version

if (Get-Command hyperfine -ErrorAction SilentlyContinue) {
    Write-Host ""
    Write-Host "[hyperfine]"
    & hyperfine -N --warmup 10 --min-runs $Runs "$Exe --version"
    exit $LASTEXITCODE
}

Write-Host ""
Write-Host "[fallback loop] install hyperfine for distribution stats"

$samples = New-Object System.Collections.Generic.List[double]
for ($i = 0; $i -lt $Runs; $i++) {
    $t = (Measure-Command { & $Exe --version | Out-Null }).TotalMilliseconds
    $samples.Add($t)
}
$arr = $samples.ToArray()
[System.Array]::Sort($arr)
$median = $arr[[int]($Runs / 2)]
$min = $arr[0]
$p95Index = [int][Math]::Floor($Runs * 0.95)
if ($p95Index -ge $Runs) { $p95Index = $Runs - 1 }
$p95 = $arr[$p95Index]

Write-Host ("min={0:N1}ms median={1:N1}ms p95={2:N1}ms" -f $min, $median, $p95)
$verdict = 'FAIL'
if ($median -lt 100) { $verdict = 'PASS' }
Write-Host ("budget check (<100 ms launch): {0}" -f $verdict)
