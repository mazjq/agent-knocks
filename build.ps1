# Build AgentStatusLight.exe with the in-box .NET Framework compiler (csc.exe).
# No SDK install required. Output -> .\bin\AgentStatusLight.exe
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$src  = Join-Path $root "src\AgentStatusLight.cs"
$core = Join-Path $root "src\Core.cs"
$bin  = Join-Path $root "bin"
$out  = Join-Path $bin  "AgentStatusLight.exe"

if (-not (Test-Path $bin)) { New-Item -ItemType Directory -Path $bin | Out-Null }

# Locate csc.exe (prefer 64-bit)
$candidates = @(
    "$env:WINDIR\Microsoft.NET\Framework64\v4.0.30319\csc.exe",
    "$env:WINDIR\Microsoft.NET\Framework\v4.0.30319\csc.exe"
)
$csc = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $csc) { throw "csc.exe not found (in-box .NET Framework compiler missing)." }

Write-Host "Compiler: $csc"

# /codepage:65001 -> read source as UTF-8 (so Chinese string literals are correct)
# /target:winexe  -> no console window
$cscArgs = @(
    "/nologo",
    "/target:winexe",
    "/optimize+",
    "/codepage:65001",
    "/out:$out",
    "/reference:System.dll",
    "/reference:System.Drawing.dll",
    "/reference:System.Windows.Forms.dll",
    $core,
    $src
)

& $csc @cscArgs
if ($LASTEXITCODE -ne 0) { throw "Build failed (exit $LASTEXITCODE)" }

$size = [math]::Round((Get-Item $out).Length / 1KB, 1)
Write-Host ("Build OK -> {0}  ({1} KB)" -f $out, $size) -ForegroundColor Green
