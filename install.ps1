# AgentPing installer
#   - builds the exe (if needed)
#   - deploys to %LOCALAPPDATA%\AgentPing
#   - merges Claude Code hooks into ~/.claude/settings.json (backup first)
#   - prints Codex setup note (manual, to avoid breaking existing notify)
#   - optional: start now + register autostart
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File install.ps1            # full install + start + autostart
#   powershell -ExecutionPolicy Bypass -File install.ps1 -NoStart   # install only
#   powershell -ExecutionPolicy Bypass -File install.ps1 -NoAutoStart
param(
    [switch]$NoStart,
    [switch]$NoAutoStart,
    [switch]$NoClaude,
    [switch]$NoCodex
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

# PS 5.1 needs explicit UTF-8 (no BOM) so we don't corrupt non-ASCII content
# (e.g. Chinese project paths in config.toml) or break TOML parsers with a BOM.
$Utf8NoBom = New-Object System.Text.UTF8Encoding $false
function Read-Utf8($p)  { return [System.IO.File]::ReadAllText($p, [System.Text.Encoding]::UTF8) }
function Write-Utf8($p, $text) { [System.IO.File]::WriteAllText($p, $text, $Utf8NoBom) }

# ---------- 1. build ----------
$exeSrc = Join-Path $root "bin\AgentPing.exe"
if (-not (Test-Path $exeSrc)) {
    Write-Host "Building AgentPing.exe ..."
    & (Join-Path $root "build.ps1")
}

# ---------- 2. deploy ----------
$installDir = Join-Path $env:LOCALAPPDATA "AgentPing"
if (-not (Test-Path $installDir)) { New-Item -ItemType Directory -Path $installDir | Out-Null }
$exe = Join-Path $installDir "AgentPing.exe"

# stop a running instance so we can overwrite the exe
Get-Process AgentPing -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300
Copy-Item $exeSrc $exe -Force
Write-Host "Deployed -> $exe" -ForegroundColor Green

# ---------- 3. Claude Code hooks ----------
function Add-ClaudeHook {
    param($settings, [string]$eventName, [string]$command)
    if (-not $settings.hooks) {
        $settings | Add-Member -NotePropertyName hooks -NotePropertyValue ([pscustomobject]@{}) -Force
    }
    $entry = [pscustomobject]@{ hooks = @([pscustomobject]@{ type = "command"; command = $command }) }
    # remove any pre-existing AgentPing entry for this event, keep others
    $existing = $null
    if ($settings.hooks.PSObject.Properties.Name -contains $eventName) {
        $existing = @($settings.hooks.$eventName | Where-Object {
            -not ($_.hooks | Where-Object { $_.command -like "*AgentPing*" })
        })
    }
    $merged = @()
    if ($existing) { $merged += $existing }
    $merged += $entry
    $settings.hooks | Add-Member -NotePropertyName $eventName -NotePropertyValue ([object[]]$merged) -Force
}

if (-not $NoClaude) {
    $claudeSettings = Join-Path $env:USERPROFILE ".claude\settings.json"
    if (Test-Path $claudeSettings) {
        $backup = "$claudeSettings.agentping.bak"
        Copy-Item $claudeSettings $backup -Force
        Write-Host "Backed up Claude settings -> $backup" -ForegroundColor DarkGray

        $json = Read-Utf8 $claudeSettings | ConvertFrom-Json
        $q = '"' + $exe + '"'
        Add-ClaudeHook $json "UserPromptSubmit"  "$q --emit --agent claude --status processing"
        Add-ClaudeHook $json "PreToolUse"        "$q --emit --agent claude --status processing"
        Add-ClaudeHook $json "PermissionRequest" "$q --emit --agent claude --status waiting"
        Add-ClaudeHook $json "PostToolUse"       "$q --emit --agent claude --status processing"
        Add-ClaudeHook $json "Notification"      "$q --emit --agent claude --status notify"
        Add-ClaudeHook $json "Stop"              "$q --emit --agent claude --status done"
        Add-ClaudeHook $json "SessionEnd"        "$q --emit --agent claude --status end"

        Write-Utf8 $claudeSettings ($json | ConvertTo-Json -Depth 50)
        Write-Host "Claude Code hooks installed (UserPromptSubmit/PreToolUse/PermissionRequest/PostToolUse/Notification/Stop/SessionEnd)" -ForegroundColor Green
    } else {
        Write-Host "Claude settings.json not found, skipped." -ForegroundColor Yellow
    }
}

# ---------- 3b. Codex hooks (via global ~/.codex/hooks.json) ----------
# Codex DESKTOP does NOT dispatch config.toml [[hooks]] (openai/codex#16430), but
# BOTH Desktop and CLI read the global ~/.codex/hooks.json. So we use hooks.json,
# and strip any old [[hooks]] block we previously wrote into config.toml.
if (-not $NoCodex) {
    $codexDir = Join-Path $env:USERPROFILE ".codex"
    if (Test-Path $codexDir) {
        # 1. remove our old managed block from config.toml (avoid double-fire in CLI; keep notify)
        $codexCfg = Join-Path $codexDir "config.toml"
        if (Test-Path $codexCfg) {
            $raw = Read-Utf8 $codexCfg
            $bm = "# >>> AgentPing codex hooks (managed) >>>"
            $em = "# <<< AgentPing codex hooks (managed) <<<"
            if ($raw -like "*$bm*") {
                Copy-Item $codexCfg "$codexCfg.agentping.bak" -Force
                $pat = [regex]::Escape($bm) + "[\s\S]*?" + [regex]::Escape($em)
                Write-Utf8 $codexCfg ([regex]::Replace($raw, $pat, "").TrimEnd() + "`r`n")
            }
        }

        # 2. write ~/.codex/hooks.json (hand-built so single-element arrays stay arrays)
        $hooksJson = Join-Path $codexDir "hooks.json"
        if ((Test-Path $hooksJson) -and ((Read-Utf8 $hooksJson) -notlike "*AgentPing*")) {
            Copy-Item $hooksJson "$hooksJson.agentping.bak" -Force
            Write-Host "NOTE: existing hooks.json backed up to .agentping.bak and replaced." -ForegroundColor Yellow
        }
        $exeJ = $exe -replace '\\', '\\'
        function JCmd([string]$st) { return '"\"' + $exeJ + '\" --emit --agent codex --status ' + $st + '"' }
        $evs = @(
            @('UserPromptSubmit','processing'),
            @('PreToolUse','processing'),
            @('PermissionRequest','waiting'),
            @('PostToolUse','processing'),
            @('Stop','done')
        )
        $parts = @()
        foreach ($e in $evs) {
            $c = JCmd $e[1]
            $parts += '    "' + $e[0] + '": [ { "hooks": [ { "type": "command", "command": ' + $c + ', "commandWindows": ' + $c + ' } ] } ]'
        }
        $hjContent = "{`r`n  ""hooks"": {`r`n" + ($parts -join ",`r`n") + "`r`n  }`r`n}`r`n"
        Write-Utf8 $hooksJson $hjContent
        Write-Host "Codex hooks written to ~/.codex/hooks.json (Desktop + CLI)" -ForegroundColor Green
    } else {
        Write-Host "Codex dir not found, skipped." -ForegroundColor Yellow
    }
}

# ---------- 4. autostart ----------
if (-not $NoAutoStart) {
    $runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
    Set-ItemProperty -Path $runKey -Name "AgentPing" -Value ('"' + $exe + '"')
    Write-Host "Autostart registered (HKCU Run)" -ForegroundColor Green
}

# ---------- 5. start ----------
if (-not $NoStart) {
    Start-Process -FilePath $exe
    Write-Host "AgentPing started (check the system tray)" -ForegroundColor Green
}

# ---------- 6. done ----------
Write-Host ""
Write-Host "Done. Right-click the tray icon for the menu." -ForegroundColor Green
Write-Host "Restart any running Claude/Codex sessions so the new hooks load." -ForegroundColor Cyan
Write-Host "Event log for debugging: %LOCALAPPDATA%\AgentPing\events.log" -ForegroundColor DarkGray
