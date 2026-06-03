# AgentPing uninstaller
#   - stops the running instance
#   - removes Claude Code hooks we added (keeps your other hooks)
#   - removes autostart entry
#   - deletes the install dir (and optionally the state/config)
param([switch]$KeepState)
$ErrorActionPreference = "Stop"

$Utf8NoBom = New-Object System.Text.UTF8Encoding $false
function Read-Utf8($p)  { return [System.IO.File]::ReadAllText($p, [System.Text.Encoding]::UTF8) }
function Write-Utf8($p, $text) { [System.IO.File]::WriteAllText($p, $text, $Utf8NoBom) }

# 1. stop
Get-Process AgentPing -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

# 2. remove our Claude hooks
$claudeSettings = Join-Path $env:USERPROFILE ".claude\settings.json"
if (Test-Path $claudeSettings) {
    Copy-Item $claudeSettings "$claudeSettings.agentping.bak" -Force
    $json = Read-Utf8 $claudeSettings | ConvertFrom-Json
    if ($json.hooks) {
        foreach ($evt in @($json.hooks.PSObject.Properties.Name)) {
            $kept = @($json.hooks.$evt | Where-Object {
                -not ($_.hooks | Where-Object { $_.command -like "*AgentPing*" })
            })
            if ($kept.Count -eq 0) {
                $json.hooks.PSObject.Properties.Remove($evt)
            } else {
                $json.hooks | Add-Member -NotePropertyName $evt -NotePropertyValue ([object[]]$kept) -Force
            }
        }
        Write-Utf8 $claudeSettings ($json | ConvertTo-Json -Depth 50)
        Write-Host "Removed AgentPing hooks from Claude settings.json" -ForegroundColor Green
    }
}

# 2b. remove Codex hooks: ~/.codex/hooks.json (ours) + any old config.toml block
$codexDir = Join-Path $env:USERPROFILE ".codex"
# config.toml old managed block (from earlier versions)
$codexCfg = Join-Path $codexDir "config.toml"
if (Test-Path $codexCfg) {
    $raw = Read-Utf8 $codexCfg
    $beginMark = "# >>> AgentPing codex hooks (managed) >>>"
    $endMark   = "# <<< AgentPing codex hooks (managed) <<<"
    if ($raw -like "*$beginMark*") {
        Copy-Item $codexCfg "$codexCfg.agentping.bak" -Force
        $pattern = [regex]::Escape($beginMark) + "[\s\S]*?" + [regex]::Escape($endMark)
        Write-Utf8 $codexCfg ([regex]::Replace($raw, $pattern, "").TrimEnd() + "`r`n")
        Write-Host "Removed AgentPing codex hooks block from config.toml" -ForegroundColor Green
    }
}
# hooks.json: if it only contains AgentPing hooks, delete it; else leave for manual edit
$hooksJson = Join-Path $codexDir "hooks.json"
if (Test-Path $hooksJson) {
    $hj = Read-Utf8 $hooksJson
    if ($hj -like "*AgentPing*") {
        Copy-Item $hooksJson "$hooksJson.agentping.bak" -Force
        try { $obj = $hj | ConvertFrom-Json } catch { $obj = $null }
        $onlyOurs = $true
        if ($obj -and $obj.hooks) {
            foreach ($evt in $obj.hooks.PSObject.Properties.Name) {
                foreach ($grp in @($obj.hooks.$evt)) {
                    foreach ($h in @($grp.hooks)) {
                        if ($h.command -notlike "*AgentPing*") { $onlyOurs = $false }
                    }
                }
            }
        }
        if ($onlyOurs) {
            Remove-Item $hooksJson -Force
            Write-Host "Removed ~/.codex/hooks.json (AgentPing-only)" -ForegroundColor Green
        } else {
            Write-Host "hooks.json has non-AgentPing entries; backed up to .agentping.bak - edit it manually to remove AgentPing lines." -ForegroundColor Yellow
        }
    }
}

# 3. autostart
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
if (Get-ItemProperty -Path $runKey -Name "AgentPing" -ErrorAction SilentlyContinue) {
    Remove-ItemProperty -Path $runKey -Name "AgentPing"
    Write-Host "Removed autostart entry" -ForegroundColor Green
}

# 4. install dir
$installDir = Join-Path $env:LOCALAPPDATA "AgentPing"
if (Test-Path $installDir) {
    if ($KeepState) {
        Remove-Item (Join-Path $installDir "AgentPing.exe") -Force -ErrorAction SilentlyContinue
        Write-Host "Removed exe, kept state/config ($installDir)" -ForegroundColor Green
    } else {
        Remove-Item $installDir -Recurse -Force
        Write-Host "Removed $installDir" -ForegroundColor Green
    }
}

Write-Host ""
Write-Host "Uninstalled. NOTE: if you wired Codex manually, revert config.toml 'notify' yourself." -ForegroundColor Yellow
