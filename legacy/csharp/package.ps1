# Build a self-contained portable distributable -> dist\AgentKnocks-<date>.zip
# The zip includes the prebuilt exe AND the source + scripts, so the recipient can
# either double-click install.cmd (uses the prebuilt exe, no build needed) or rebuild.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

# 1. build fresh
& (Join-Path $root "build.ps1")

# 2. stage files
$stamp = Get-Date -Format "yyyyMMdd"
$dist  = Join-Path $root "dist"
$stage = Join-Path $dist "AgentKnocks"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage | Out-Null

$include = @(
    "install.cmd", "uninstall.cmd",
    "install.ps1", "uninstall.ps1", "build.ps1",
    "README.md"
)
foreach ($f in $include) { Copy-Item (Join-Path $root $f) (Join-Path $stage $f) -Force }

# prebuilt exe (so install can skip building)
New-Item -ItemType Directory -Path (Join-Path $stage "bin") | Out-Null
Copy-Item (Join-Path $root "bin\AgentKnocks.exe") (Join-Path $stage "bin\AgentKnocks.exe") -Force

# source (so recipient can rebuild if they want)
Copy-Item (Join-Path $root "src")   (Join-Path $stage "src")   -Recurse -Force
Copy-Item (Join-Path $root "hooks") (Join-Path $stage "hooks") -Recurse -Force

# 3. zip it
$zip = Join-Path $dist ("AgentKnocks-" + $stamp + ".zip")
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $stage -DestinationPath $zip -Force
Remove-Item $stage -Recurse -Force

$mb = [math]::Round((Get-Item $zip).Length / 1KB, 1)
Write-Host ("Packaged -> {0}  ({1} KB)" -f $zip, $mb) -ForegroundColor Green
Write-Host "Attach this zip to a GitHub Release, or share it directly." -ForegroundColor Cyan
Write-Host "Recipient: unzip -> double-click install.cmd" -ForegroundColor Cyan
