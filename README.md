# Agent Knocks

**English** | [中文](README.zh-CN.md)

A tiny Windows system-tray status light for **AI coding agents** (Claude Code / Codex / pi).
It "knocks" — color + sound + toast — when an agent needs you or finishes, and one click jumps to
that agent's window. Native **Rust** single EXE (~0.45 MB, ~10 MB resident), no runtime dependencies.

<p align="center">
  <img src="pic/colors.png" width="260" alt="tri-state tray dots"><br>
  <sub>🟢 Done ｜ 🔵 Working ｜ 🟠 Waiting</sub>
</p>
<p align="center">
  <img src="pic/details.png" width="320" alt="tray menu — per-session detail"><br>
  <sub>Right-click: each session's status / elapsed / project · ↗ jump · mute · test sound · language · start at login</sub>
</p>

> The original C# / .NET build is archived under [`legacy/csharp/`](legacy/csharp/) and the
> [`csharp-final`](https://github.com/mazjq/agent-knocks/releases/tag/csharp-final) release.

## Features

| State | Color | Sound |
|---|---|---|
| 🔵 Working — agent is doing work | blue | none |
| 🟠 Waiting — needs your input/approval | orange | rising 660→990 Hz + toast |
| 🟢 Done — turn finished | green | ascending 770→1046→1318 Hz + toast |
| ⚪ Idle — no active session | grey | none |

- **Glanceable tri-state dot** aggregating all sessions (Waiting > Working > Done > Idle); right-click for per-session detail.
- **Intuitive earcons + Windows toast** on waiting/done (mute-able).
- **Click-to-focus** — left-click the dot (or a menu session line) raises that agent's window, matched by project/cwd folder name so it works across multiple VSCode windows.
- **Multi-session** — each window tracked by `session_id` with a short `#tag`.
- **EN / 中文** — switch in the tray menu (default English).
- **Done persists** until you close the terminal (`SessionEnd`), with a 30-min cap + "Clear completed".
- **Non-blocking & safe** — the hook writes a state file and exits (no stdout, exit 0), never altering the agent; install/uninstall touch only their own hooks.

## Install

**Easiest (no toolchain):** download the [latest release](https://github.com/mazjq/agent-knocks/releases/latest) zip → unzip → double-click **`install.cmd`**. Uninstall: `uninstall.cmd`.

**From source** (needs the [Rust toolchain](https://rustup.rs)):
```
git clone https://github.com/mazjq/agent-knocks && cd agent-knocks && install.cmd
```

Install deploys to `%LOCALAPPDATA%\AgentKnocks\`, merges Claude hooks into `~/.claude/settings.json`
(backup kept) + writes Codex `~/.codex/hooks.json` (your `notify` untouched), adds a Start-Menu
shortcut + autostart, and starts the tray. **Restart running Claude/Codex sessions** so the hooks load.
Flags: `-NoStart` / `-NoAutoStart` / `-NoClaude` / `-NoCodex`.

Hook → state: `UserPromptSubmit`/`PreToolUse`/`PostToolUse` → working · `PermissionRequest` → waiting ·
`Stop` → done · `SessionEnd` → remove.

## Development

```powershell
cargo test               # core state-machine tests (16)
cargo build --release    # target\release\agentknocks.exe (GUI subsystem, no console)
.\install.ps1            # build + redeploy + restart the tray
.\package.ps1            # portable zip -> dist\
```
Pure logic lives in `src/core.rs` (no UI deps, fully unit-tested). TDD: add a case to its
`#[cfg(test)] mod tests`, go green, then change the implementation. UI strings are the `t_*` helpers
in `src/tray.rs`.

## Architecture

Hook-driven, three stages, no polling:

```
 ① agent hook                 ② state file = event bus              ③ resident tray
 claude/codex ─emit─► %LOCALAPPDATA%\AgentKnocks\state\<agent>__<session>.json
              (observer: no stdout / exit 0)          ─► notify watcher → aggregate → color + sound + toast
```

- **`src/core.rs`** — pure state machine: status priority, JSON parse, aggregate, TTL prune, transition cues, and `select_window` (focus targeting). Platform-agnostic, unit-tested.
- **`src/app.rs`** — engine: `notify` file-watcher + ≤2 s prune loop; writes `status.json` (aggregate, for external consumers).
- **`src/main.rs`** — `--emit` (observer; captures the agent's cwd + window handle) / tray / `--once`.
- **`src/tray.rs`** — Windows tray UI: dot + menu + sound + toast + i18n + autostart + click-to-focus (Win32).

**Drive it from anything** (the hook contract):
```
AgentKnocks.exe --emit --agent <name> --status <processing|waiting|done|end> [--key <id>] [--title <label>]
```
or pipe the hook JSON via stdin. External tools can read `state\*.json` / `status.json` directly.

## Limitations

- Windows only — `core.rs`/`app.rs` are cross-platform; the macOS/Linux tray is on the [roadmap (#1)](https://github.com/mazjq/agent-knocks/issues/1).
- Codex **desktop** app doesn't dispatch local hooks yet ([openai/codex#16430](https://github.com/openai/codex/issues/16430)) → `hooks.json` is ready, or use the Codex CLI.
- Several agents in **one** VSCode window's terminals → focus can raise the window, not a specific terminal tab.
