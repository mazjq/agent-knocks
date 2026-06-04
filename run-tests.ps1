# Compile Core.cs + tests/Tests.cs into a console test runner and execute it.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$core = Join-Path $root "src\Core.cs"
$test = Join-Path $root "tests\Tests.cs"
$bin  = Join-Path $root "bin"
$out  = Join-Path $bin "AgentKnocks.Tests.exe"
if (-not (Test-Path $bin)) { New-Item -ItemType Directory -Path $bin | Out-Null }

$csc = @(
    "$env:WINDIR\Microsoft.NET\Framework64\v4.0.30319\csc.exe",
    "$env:WINDIR\Microsoft.NET\Framework\v4.0.30319\csc.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $csc) { throw "csc.exe not found." }

& $csc /nologo /target:exe /codepage:65001 /out:$out /reference:System.dll /reference:System.Core.dll $core $test
if ($LASTEXITCODE -ne 0) { throw "Test build failed (exit $LASTEXITCODE)" }

Write-Host "Running tests..." -ForegroundColor Cyan
& $out
$code = $LASTEXITCODE
if ($code -eq 0) { Write-Host "ALL TESTS PASSED" -ForegroundColor Green }
else { Write-Host "TESTS FAILED" -ForegroundColor Red }
exit $code
