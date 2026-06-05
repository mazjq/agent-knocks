# Build a self-contained portable distributable -> dist\AgentKnocks-<date>.zip
# The zip ships the prebuilt Rust exe + install scripts, so a recipient WITHOUT the
# Rust toolchain can just unzip and double-click install.cmd.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

# 1. build release
Push-Location $root
try { & cargo build --release; if ($LASTEXITCODE -ne 0) { throw "cargo build failed" } }
finally { Pop-Location }

# 2. stage (in TEMP - the repo dir is held by the IDE/agent file watcher)
$stamp = Get-Date -Format "yyyyMMdd"
$stage = Join-Path $env:TEMP ("AgentKnocks-pkg-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Path $stage | Out-Null

foreach ($f in @("install.cmd", "uninstall.cmd", "install.ps1", "uninstall.ps1", "README.md", "README.zh-CN.md")) {
    Copy-Item (Join-Path $root $f) (Join-Path $stage $f) -Force
}
New-Item -ItemType Directory -Path (Join-Path $stage "bin") | Out-Null
Copy-Item (Join-Path $root "target\release\agentknocks.exe") (Join-Path $stage "bin\agentknocks.exe") -Force
Copy-Item (Join-Path $root "hooks") (Join-Path $stage "hooks") -Recurse -Force
Copy-Item (Join-Path $root "pic")   (Join-Path $stage "pic")   -Recurse -Force

# 3. zip into dist\
$dist = Join-Path $root "dist"
if (-not (Test-Path $dist)) { New-Item -ItemType Directory -Path $dist | Out-Null }
$zip = Join-Path $dist ("AgentKnocks-" + $stamp + ".zip")
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -Force
Remove-Item $stage -Recurse -Force

$kb = [math]::Round((Get-Item $zip).Length / 1KB, 1)
Write-Host ("Packaged -> {0}  ({1} KB)" -f $zip, $kb) -ForegroundColor Green
Write-Host "Recipient: unzip -> double-click install.cmd (no Rust toolchain needed)." -ForegroundColor Cyan
