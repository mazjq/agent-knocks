# Agent Knocks (Rust build) installer
#   - builds the Rust release exe (cargo) if needed
#   - deploys to %LOCALAPPDATA%\AgentKnocks (same path as the C# build, so hooks are unchanged)
#   - merges Claude Code hooks into ~/.claude/settings.json (backup first)
#   - writes Codex ~/.codex/hooks.json
#   - creates a Start Menu shortcut (so you can launch it when autostart is off)
#   - registers autostart + starts the tray
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File install.ps1            # full install + start + autostart
#   powershell -ExecutionPolicy Bypass -File install.ps1 -NoStart -NoAutoStart -NoClaude -NoCodex
param(
    [switch]$NoStart,
    [switch]$NoAutoStart,
    [switch]$NoClaude,
    [switch]$NoCodex
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

$Utf8NoBom = New-Object System.Text.UTF8Encoding $false
function Read-Utf8($p)  { return [System.IO.File]::ReadAllText($p, [System.Text.Encoding]::UTF8) }
function Write-Utf8($p, $text) { [System.IO.File]::WriteAllText($p, $text, $Utf8NoBom) }

# ---------- 1. build ----------
$exeSrc = Join-Path $root "target\release\agentknocks.exe"
if (-not (Test-Path $exeSrc)) {
    Write-Host "Building agentknocks.exe (cargo build --release) ..."
    Push-Location $root
    try { & cargo build --release; if ($LASTEXITCODE -ne 0) { throw "cargo build failed" } }
    finally { Pop-Location }
}

# ---------- 2. deploy ----------
$installDir = Join-Path $env:LOCALAPPDATA "AgentKnocks"
if (-not (Test-Path $installDir)) { New-Item -ItemType Directory -Path $installDir | Out-Null }
$exe = Join-Path $installDir "AgentKnocks.exe"

Get-Process AgentKnocks -ErrorAction SilentlyContinue | Stop-Process -Force
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
    $existing = $null
    if ($settings.hooks.PSObject.Properties.Name -contains $eventName) {
        $existing = @($settings.hooks.$eventName | Where-Object {
            -not ($_.hooks | Where-Object { $_.command -like "*AgentKnocks*" })
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
        Copy-Item $claudeSettings "$claudeSettings.agentknocks.bak" -Force
        Write-Host "Backed up Claude settings -> $claudeSettings.agentknocks.bak" -ForegroundColor DarkGray
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
        Write-Host "Claude Code hooks installed" -ForegroundColor Green
    } else {
        Write-Host "Claude settings.json not found, skipped." -ForegroundColor Yellow
    }
}

# ---------- 3b. Codex hooks (~/.codex/hooks.json) ----------
if (-not $NoCodex) {
    $codexDir = Join-Path $env:USERPROFILE ".codex"
    if (Test-Path $codexDir) {
        $hooksJson = Join-Path $codexDir "hooks.json"
        if ((Test-Path $hooksJson) -and ((Read-Utf8 $hooksJson) -notlike "*AgentKnocks*")) {
            Copy-Item $hooksJson "$hooksJson.agentknocks.bak" -Force
            Write-Host "NOTE: existing hooks.json backed up to .agentknocks.bak and replaced." -ForegroundColor Yellow
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
        Write-Host "Codex hooks written to ~/.codex/hooks.json" -ForegroundColor Green
    } else {
        Write-Host "Codex dir not found, skipped." -ForegroundColor Yellow
    }
}

# ---------- 4. Start Menu shortcut ----------
$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$lnk = Join-Path $startMenu "Agent Knocks.lnk"
try {
    $ws = New-Object -ComObject WScript.Shell
    $sc = $ws.CreateShortcut($lnk)
    $sc.TargetPath = $exe
    $sc.WorkingDirectory = $installDir
    $sc.Description = "Agent Knocks - AI agent status tray"
    $sc.Save()
    Write-Host "Start Menu shortcut created (search 'Agent Knocks')" -ForegroundColor Green
} catch {
    Write-Host "Could not create Start Menu shortcut: $($_.Exception.Message)" -ForegroundColor Yellow
}

# ---------- 5. autostart ----------
if (-not $NoAutoStart) {
    $runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
    Set-ItemProperty -Path $runKey -Name "AgentKnocks" -Value ('"' + $exe + '"')
    Write-Host "Autostart registered (HKCU Run)" -ForegroundColor Green
}

# ---------- 6. start ----------
if (-not $NoStart) {
    Start-Process -FilePath $exe
    Write-Host "Agent Knocks started (check the system tray)" -ForegroundColor Green
}

Write-Host ""
Write-Host "Done. Right-click the tray icon for the menu." -ForegroundColor Green
Write-Host "Restart any running Claude/Codex sessions so the new hooks load." -ForegroundColor Cyan
Write-Host "Event log for debugging: %LOCALAPPDATA%\AgentKnocks\events.log" -ForegroundColor DarkGray
