# Agent Knocks

**English** | [中文](README.zh-CN.md)

A tiny system-tray status light for **AI coding agents** (Claude Code / Codex / pi …).
The tray dot changes color + plays an intuitive earcon when an agent finishes or needs you.
Native **Rust** single EXE (~**0.4 MB**), no runtime dependencies. Windows today; the codebase
is cross-platform and macOS/Linux builds are on the roadmap.

> The original C# / .NET-Framework build is archived under [`legacy/csharp/`](legacy/csharp/) and as
> the [`csharp-final`](https://github.com/mazjq/agent-knocks/releases/tag/csharp-final) release.

| State | Meaning | Color | Sound |
|---|---|---|---|
| 🔵 Working | the agent is doing work | blue | none |
| 🟠 Waiting | the agent is waiting for your input/approval | orange | rising 660→990 Hz (like "you there?") |
| 🟢 Done | the agent finished | green | ascending 770→1046→1318 Hz ("all set") |
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
| orange + rising earcon | green + ascending triad |

> Screenshots are from the C# edition (中文 UI). The Rust build signals via color + sound;
> balloon toasts are pending (see TODO).

| Working 🔵 | Session detail (right-click) |
|:---:|:---:|
| <img src="pic/processing.png" width="320" alt="working tray"> | <img src="pic/details.png" width="320" alt="per-session detail menu"> |
| blue dot, silent while busy | each session's state / elapsed / project tag |

## How it works

No polling, no guessing — it **subscribes to the agent's lifecycle events (hooks)**. Three stages:

```
 ① agent event (hook)         ② state file = event bus           ③ resident tray
 claude/codex ─emit─► writes state\<agent>__<session>.json ─► notify watcher
              (observer: no output / exit 0)                     ↓ aggregate → color + sound
```

- **emit** — each agent hook calls `AgentKnocks.exe --emit ...`, writes one state file, exits.
  It is a **pure observer**: writes nothing to stdout, always exits 0, so it **never blocks or
  alters the agent's decisions**.
- **state file = event bus** — one JSON per session; producer (emit) and consumer (tray) fully decoupled.
- **tray** — watches that folder with the `notify` crate (the OS file-change notification, ~120 ms),
  aggregates all sessions, recolors + synthesizes earcons via Win32 `Beep`.
- **lightweight** — a native Rust single EXE (~0.4 MB, GUI subsystem so no console window);
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
- `install.cmd` builds with **cargo** (needs the [Rust toolchain](https://rustup.rs)). No toolchain? Use the
  [`csharp-final`](https://github.com/mazjq/agent-knocks/releases/tag/csharp-final) portable zip (in-box compiler, no install). A prebuilt Rust release is TODO.
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
Cargo.toml · Cargo.lock
src/core.rs              pure state logic (no UI, testable): state machine / aggregate / transitions / inference
src/app.rs               engine: notify watcher + aggregate loop + status.json
src/main.rs              entry: --emit (observer) / tray / --once / autostart flags
src/tray.rs              Windows tray-icon UI: dot + menu + sound + i18n + autostart
install.cmd · uninstall.cmd   double-click install/uninstall (bypasses execution policy)
install.ps1 · uninstall.ps1
hooks/  codex-setup.md · generic-setup.md · codex-notify-chain.ps1 (optional)
legacy/csharp/           the original C# build (archived; also the csharp-final release)
```
Runtime data (not in the repo): `%LOCALAPPDATA%\AgentKnocks\` = `AgentKnocks.exe` · `state\*.json` · `status.json` · `events.log` · `config.json`

## Development

```powershell
cargo test               # core state-machine tests
cargo build --release    # build target\release\agentknocks.exe (GUI subsystem, no console)
powershell -ExecutionPolicy Bypass -File install.ps1   # build + redeploy + restart tray
```
Core logic lives in `src/core.rs` (no UI deps); add a case to its `#[cfg(test)] mod tests` and go green
before changing the implementation (TDD). All user-facing strings live in the `I18n`/`t_*` helpers in `src/tray.rs`.

## Known limitations / TODO

- **Codex desktop app** doesn't dispatch local hooks yet (upstream bug [openai/codex#16430](https://github.com/openai/codex/issues/16430))
  → `hooks.json` is in place and activates automatically once fixed; use the **Codex CLI** for immediate use.
- pi hook mechanism unconfirmed (generic protocol provided as fallback).
- **macOS / Linux**: the core + engine are cross-platform; the tray (`tray.rs`) and autostart are Windows-only
  so far — native event loop + `auto-launch` for the other platforms is on the roadmap ([#1](https://github.com/mazjq/agent-knocks/issues/1)).
- **Balloon toasts** (the C# build had them) are pending in the Rust tray.
- **click-to-focus** (jump to the agent's window) planned ([#2](https://github.com/mazjq/agent-knocks/issues/2)).
