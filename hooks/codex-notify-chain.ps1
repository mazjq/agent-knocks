# AgentKnocks <-> Codex notify chain
# Codex allows only ONE 'notify' program. If you already use 'notify' for another
# plugin (e.g. computer-use), point Codex at THIS script instead: it forwards the
# event to AgentKnocks AND to your original notify program, so nothing breaks.
#
# Codex calls:  <notify program> <event-json>
# This script receives that JSON as $args (usually a single JSON string arg).
#
# Configure the original program below, then set in ~/.codex/config.toml:
#   notify = ["powershell","-NoProfile","-ExecutionPolicy","Bypass","-File",
#             "C:\\path\\to\\agent-knocks\\hooks\\codex-notify-chain.ps1"]

# ---- EDIT THIS: your previous notify program + its fixed leading args ----
# Leave as @() if you had no previous notify program. Example:
#   $Original = @("C:\Users\<you>\.codex\plugins\...\codex-computer-use.exe", "turn-ended")
$Original = @()
# -------------------------------------------------------------------------

$exe = Join-Path $env:LOCALAPPDATA "AgentKnocks\AgentKnocks.exe"
$eventJson = ""
if ($args.Count -gt 0) { $eventJson = ($args -join " ") }

# 1) feed AgentKnocks (status auto-inferred from the event json)
try {
    if (Test-Path $exe) {
        $eventJson | & $exe --emit --agent codex --status auto | Out-Null
    }
} catch { }

# 2) forward to the original notify program (preserve its behavior)
try {
    if ($Original.Count -gt 0 -and (Test-Path $Original[0])) {
        $prog = $Original[0]
        $rest = @()
        if ($Original.Count -gt 1) { $rest += $Original[1..($Original.Count-1)] }
        $rest += $args
        & $prog @rest | Out-Null
    }
} catch { }
