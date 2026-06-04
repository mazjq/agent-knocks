# Agent Knocks (Rust build) uninstaller
#   - stops the tray
#   - removes only the hooks it added (keeps your other hooks)
#   - removes the Start Menu shortcut + autostart entry
#   - deletes the install dir (and optionally the state/config)
param([switch]$KeepState)
$ErrorActionPreference = "Stop"

$Utf8NoBom = New-Object System.Text.UTF8Encoding $false
function Read-Utf8($p)  { return [System.IO.File]::ReadAllText($p, [System.Text.Encoding]::UTF8) }
function Write-Utf8($p, $text) { [System.IO.File]::WriteAllText($p, $text, $Utf8NoBom) }

# 1. stop
Get-Process AgentKnocks -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

# 2. remove our Claude hooks
$claudeSettings = Join-Path $env:USERPROFILE ".claude\settings.json"
if (Test-Path $claudeSettings) {
    Copy-Item $claudeSettings "$claudeSettings.agentknocks.bak" -Force
    $json = Read-Utf8 $claudeSettings | ConvertFrom-Json
    if ($json.hooks) {
        foreach ($evt in @($json.hooks.PSObject.Properties.Name)) {
            $kept = @($json.hooks.$evt | Where-Object {
                -not ($_.hooks | Where-Object { $_.command -like "*AgentKnocks*" })
            })
            if ($kept.Count -eq 0) {
                $json.hooks.PSObject.Properties.Remove($evt)
            } else {
                $json.hooks | Add-Member -NotePropertyName $evt -NotePropertyValue ([object[]]$kept) -Force
            }
        }
        Write-Utf8 $claudeSettings ($json | ConvertTo-Json -Depth 50)
        Write-Host "Removed AgentKnocks hooks from Claude settings.json" -ForegroundColor Green
    }
}

# 2b. remove Codex hooks.json if it only contains AgentKnocks
$hooksJson = Join-Path $env:USERPROFILE ".codex\hooks.json"
if (Test-Path $hooksJson) {
    $hj = Read-Utf8 $hooksJson
    if ($hj -like "*AgentKnocks*") {
        Copy-Item $hooksJson "$hooksJson.agentknocks.bak" -Force
        try { $obj = $hj | ConvertFrom-Json } catch { $obj = $null }
        $onlyOurs = $true
        if ($obj -and $obj.hooks) {
            foreach ($evt in $obj.hooks.PSObject.Properties.Name) {
                foreach ($grp in @($obj.hooks.$evt)) {
                    foreach ($h in @($grp.hooks)) {
                        if ($h.command -notlike "*AgentKnocks*") { $onlyOurs = $false }
                    }
                }
            }
        }
        if ($onlyOurs) {
            Remove-Item $hooksJson -Force
            Write-Host "Removed ~/.codex/hooks.json (AgentKnocks-only)" -ForegroundColor Green
        } else {
            Write-Host "hooks.json has non-AgentKnocks entries; backed up - edit it manually." -ForegroundColor Yellow
        }
    }
}

# 3. Start Menu shortcut
$lnk = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Agent Knocks.lnk"
if (Test-Path $lnk) { Remove-Item $lnk -Force; Write-Host "Removed Start Menu shortcut" -ForegroundColor Green }

# 4. autostart
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
if (Get-ItemProperty -Path $runKey -Name "AgentKnocks" -ErrorAction SilentlyContinue) {
    Remove-ItemProperty -Path $runKey -Name "AgentKnocks"
    Write-Host "Removed autostart entry" -ForegroundColor Green
}

# 5. install dir
$installDir = Join-Path $env:LOCALAPPDATA "AgentKnocks"
if (Test-Path $installDir) {
    if ($KeepState) {
        Remove-Item (Join-Path $installDir "AgentKnocks.exe") -Force -ErrorAction SilentlyContinue
        Write-Host "Removed exe, kept state/config ($installDir)" -ForegroundColor Green
    } else {
        Remove-Item $installDir -Recurse -Force
        Write-Host "Removed $installDir" -ForegroundColor Green
    }
}

Write-Host ""
Write-Host "Uninstalled. NOTE: if you wired Codex manually, revert config.toml 'notify' yourself." -ForegroundColor Yellow
