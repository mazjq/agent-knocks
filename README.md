# Agent Knocks

**English** | [中文](README.zh-CN.md)

A tiny system-tray status light for **AI coding agents** (Claude Code / Codex / pi …).
The tray dot changes color + plays an intuitive earcon + shows a balloon when an agent
finishes or needs you. Native single EXE, zero runtime dependencies, ~**23 MB** resident.
Windows only (a cross-platform Rust rewrite is on the roadmap).

| State | Meaning | Color | Sound |
|---|---|---|---|
| 🔵 Working | the agent is doing work | blue | none |
| 🟠 Waiting | the agent is waiting for your input/approval | orange | rising 660→990 Hz (like "you there?") + balloon |
| 🟢 Done | the agent finished | green | ascending 770→1046→1318 Hz ("all set") + balloon |
| ⚪ Idle | no active session | grey | none |

The tray shows the **highest-priority** state across all sessions (Waiting > Working > Done > Idle);
right-click for per-session detail. Windows are told apart by `session_id`; multiple windows of the
same project get a short tag `#XXXX`.

> **Language:** the tray UI defaults to **English**; switch to **中文** anytime from the
> tray menu → 🌐 Language. (Screenshots below show the 中文 UI.)

## Screenshots

<p align="center">
  <img src="pic/colors.png" width="280" alt="Three-state tray dots: done / working / waiting"><br>
  <sub>Three-state tray light: 🟢 Done ｜ 🔵 Working ｜ 🟠 Waiting</sub>
</p>

| Waiting 🟠 | Done 🟢 |
|:---:|:---:|
| <img src="pic/waiting.png" width="320" alt="waiting balloon"> | <img src="pic/done.png" width="320" alt="done balloon"> |
| orange + rising earcon + balloon | green + ascending triad + balloon |

| Working 🔵 | Session detail (right-click) |
|:---:|:---:|
| <img src="pic/processing.png" width="320" alt="working tray"> | <img src="pic/details.png" width="320" alt="per-session detail menu"> |
| blue dot, silent while busy | each session's state / elapsed / project tag |

## How it works

No polling, no guessing — it **subscribes to the agent's lifecycle events (hooks)**. Three stages:

```
 ① agent event (hook)         ② state file = event bus           ③ resident tray
 claude/codex ─emit─► writes state\<agent>__<session>.json ─► FileSystemWatcher
              (observer: no output / exit 0)                     ↓ aggregate → color + sound + balloon
```

- **emit** — each agent hook calls `AgentKnocks.exe --emit ...`, writes one state file, exits.
  It is a **pure observer**: writes nothing to stdout, always exits 0, so it **never blocks or
  alters the agent's decisions**.
- **state file = event bus** — one JSON per session; producer (emit) and consumer (tray) fully decoupled.
- **tray** — watches that folder with `FileSystemWatcher` (the OS file-change notification, ~120 ms),
  aggregates all sessions, recolors + synthesizes sound via `Console.Beep` + shows a balloon.
- **lightweight** — a native single EXE compiled by Windows' built-in `csc.exe` (~25 KB file / ~23 MB RAM);
  apart from the tray there is **no extra resident process and no polling**.

## Integration protocol

For **any agent or script**, and the integration point for **external consumers** (e.g. a multi-agent dashboard).

**① Report status** (from an agent hook or any script):
```
AgentKnocks.exe --emit --agent <name> --status <processing|waiting|done|end> [--key <session-id>] [--title <label>]
```
You can also pipe the event JSON via **stdin**; it auto-parses `session_id` / `cwd`.

**② State file**: `%LOCALAPPDATA%\AgentKnocks\state\<agent>__<session>.json`
```json
{"agent":"claude","session":"...","status":"waiting","title":"my-project","ts":1780000000}
```

**③ Aggregate status** (for external queries): `%LOCALAPPDATA%\AgentKnocks\status.json`
```json
{"agg":"waiting","sessions":1,"ts":1780000000}
```
> To reuse this tool's status from elsewhere, **just read `state\*.json` or `status.json`** — no need
> to understand the internals.

**Diagnostics**: `events.log` records every report (ms timestamp / status / message; auto-resets past 200 KB).

## Install

**Simplest**: download this repo (**Code → Download ZIP** or `git clone`), unzip, **double-click `install.cmd`**.
Uninstall by double-clicking `uninstall.cmd`.

<details><summary>Other methods / flags</summary>

- Command line: `git clone https://github.com/mazjq/agent-knocks && cd agent-knocks && powershell -ExecutionPolicy Bypass -File install.ps1`
- Portable (no build): download the Release `AgentKnocks-*.zip`, unzip → double-click `install.cmd`; build your own with `package.ps1` (output in `dist\`).
- Flags: `-NoStart` / `-NoAutoStart` / `-NoClaude` / `-NoCodex`.
</details>

**What install does**: build (skipped if the portable zip already ships the exe) → deploy to
`%LOCALAPPDATA%\AgentKnocks\` → merge Claude hooks into `~/.claude/settings.json` (backs up
`.agentknocks.bak` first, keeps your existing hooks) → write Codex `~/.codex/hooks.json`
(leaves your `notify` alone) → register autostart → start the tray.
**Restart any running Claude / Codex session afterwards** (hooks load at session start).

**Hook mapping** (auto-wired, never overwrites your existing hooks):

| Event | State |
|---|---|
| `UserPromptSubmit` / `PreToolUse` / `PostToolUse` | Working 🔵 |
| `PermissionRequest` | Waiting 🟠 (fires the instant the prompt appears, no delay) |
| `Stop` | Done 🟢 ｜ `SessionEnd` removes the session |
| `Notification` (Claude only) | smart: idle "waiting for your input" → ignored (Stop already reported Done); permission → Waiting |

- Claude: works out of the box.
- Codex: writes global `~/.codex/hooks.json` (**not config.toml** — the desktop app doesn't dispatch it,
  [openai/codex#16430](https://github.com/openai/codex/issues/16430)). See [`hooks/codex-setup.md`](hooks/codex-setup.md).
- pi / any agent: see [`hooks/generic-setup.md`](hooks/generic-setup.md) — wire the three states per the protocol above.

## Tray menu / uninstall

- Menu: aggregate status + per-session detail ｜ 🔇 Mute ｜ 🔊 Test sound ｜ 📁 Open state folder ｜
  🌐 Language (English / 中文) ｜ ⏻ Start at login ｜ ❌ Quit. Double-click the icon = open state folder.
- Uninstall: double-click `uninstall.cmd` (or `uninstall.ps1`). Stops the process → removes **only the
  hooks it added** (your other config stays) → removes autostart → deletes the install dir. `-KeepState`
  keeps state/config. **Verified not to touch your original agent config** (notify / other hooks untouched).

## File structure

```
src/Core.cs              pure state logic (no UI, testable): state machine / aggregate / transitions / inference
src/AgentKnocks.cs  UI + emit entry (tray + emit dual mode, C# 5, i18n EN/中文)
tests/Tests.cs           Core assertions (38)
build.ps1 / run-tests.ps1 / package.ps1
install.cmd · uninstall.cmd   double-click install/uninstall (bypasses execution policy)
install.ps1 · uninstall.ps1
hooks/  codex-setup.md · generic-setup.md · codex-notify-chain.ps1 (optional)
```
Runtime data (not in the repo): `%LOCALAPPDATA%\AgentKnocks\` = `AgentKnocks.exe` · `state\*.json` · `status.json` · `events.log` · `config.json`

## Development

```powershell
powershell -ExecutionPolicy Bypass -File run-tests.ps1   # run 38 tests
powershell -ExecutionPolicy Bypass -File build.ps1       # compile
powershell -ExecutionPolicy Bypass -File install.ps1     # redeploy + restart tray
```
Core logic lives in `src/Core.cs` (no UI deps); add a case to `tests/Tests.cs` and go green before
changing the implementation (TDD). All user-facing strings live in the `I18n` class in `src/AgentKnocks.cs`.

## Known limitations / TODO

- **Codex desktop app** doesn't dispatch local hooks yet (upstream bug [openai/codex#16430](https://github.com/openai/codex/issues/16430))
  → `hooks.json` is in place and activates automatically once fixed; use the **Codex CLI** for immediate use.
- pi hook mechanism unconfirmed (generic protocol provided as fallback).
- Sounds are synthesized with `Console.Beep`; for custom WAVs add a `SoundPlayer` branch in `SoundEngine`.
- **Cross-platform Rust rewrite** planned (Windows-only today; also cuts RAM 23 MB → 3–5 MB) — see issues.
- **click-to-focus** (jump to the agent's window from the balloon) planned — see issues.
