# 接入 Codex

Codex 有完整的生命周期 hook 系统，和 `notify` 是两套独立机制——AgentPing 用 hook 接入，
**完全不碰**你被 computer-use 占用的 `notify`。

## ⚠️ 关键：用全局 `~/.codex/hooks.json`，不要用 config.toml

**Codex 桌面 app 当前不派发 `config.toml` 里的 `[[hooks]]`**（已知上游 bug
[openai/codex#16430](https://github.com/openai/codex/issues/16430)）——会出现"app 列出了 hook、
你也点了信任，却一条都不触发"的现象。

而**桌面 app 和 CLI 都会读全局 `~/.codex/hooks.json`**。所以 AgentPing 把 hook 写到
`~/.codex/hooks.json`（`install.ps1` 自动完成），同时清掉早期版本写进 config.toml 的块，避免
CLI 下重复触发。

> 这个排查线索来自 [agentmemory](https://github.com/rohitg00/agentmemory)（同样用 hook 构建记忆系统的项目），它踩过一模一样的坑。

## 自动接入（install.ps1 已做）

生成的 `~/.codex/hooks.json`：

```json
{
  "hooks": {
    "UserPromptSubmit": [ { "hooks": [ { "type": "command", "command": "\"...AgentPing.exe\" --emit --agent codex --status processing", "commandWindows": "..." } ] } ],
    "PreToolUse":        [ ... processing ... ],
    "PermissionRequest": [ ... waiting ... ],
    "PostToolUse":       [ ... processing ... ],
    "Stop":              [ ... done ... ]
  }
}
```

事件→状态映射：

| Codex Hook | 状态 |
|---|---|
| `UserPromptSubmit` / `PreToolUse` / `PostToolUse` | 处理中 🔵 |
| `PermissionRequest` | 等待确认 🟠 |
| `Stop` | 已完成 🟢 |

注意 `command` 与 `commandWindows` 都设成真实 Windows 调用——**桌面 app 读的是 `command`**，
占位 `"true"` 在 Windows 上不是有效命令会导致 hook 静默失败。

## 生效条件

- `config.toml` 里 `[features] hooks = true`（你已开启）
- 如果 app 有"自动审查 / 自定义(config.toml)"之类的 hook 模式开关，选**自定义/启用自定义 hook**
- 改完 hooks.json **必须完全重启 Codex app**（hook 在会话启动时加载）
- 重启后若 app 弹"信任 hook"提示 → 允许

## 验证

让 Codex 干一轮活，然后：
```powershell
Get-Content "$env:LOCALAPPDATA\AgentPing\events.log" -Tail 15
```
出现 `codex … hook=PreToolUse` 等行 = 接通 ✅

## 卸载

`uninstall.ps1` 会删掉 `~/.codex/hooks.json`（若只含 AgentPing 的 hook）并清理 config.toml 里的旧块，
你的 `notify` 和其它配置保持不动。
