# Agent 状态提示器 · AgentPing

后台监控 **claude / codex / pi 等 agent** 的工作状态，托盘图标变色 + 直觉化声音 + 气泡提示。
原生单 EXE，零运行时依赖，常驻内存约 **23MB**。

## 三种状态

| 状态 | 含义 | 托盘颜色 | 声音 |
|---|---|---|---|
| 🔵 处理中 | agent 正在干活 | 蓝 | 无 |
| 🟠 等待确认 | agent 在等你输入/批准 | 橙 | **上升音**（660→990Hz，像在问"在吗？"）+ 气泡 |
| 🟢 已完成 | agent 处理完成 | 绿 | **上行三连**（770→1046→1318Hz，"搞定~"）+ 气泡 |
| ⚪ 空闲 | 无活动会话 | 灰 | 无 |

托盘图标显示所有会话里**优先级最高**的状态（等待 > 处理 > 完成 > 空闲）。右键菜单可看每个会话明细（agent · 状态 · 耗时 · `项目名 #会话标签`）。

**多窗口区分**：按 `session_id` 区分每个对话窗口；同项目多开时，菜单/气泡会带短会话标签 `#XXXX` 加以分辨。

**诊断日志**：每次状态上报会追加到 `%LOCALAPPDATA%\AgentPing\events.log`（带时间戳、状态、消息，超 200KB 自动重置）。颜色/时机不对时可据此排查真实事件流。

## 设计

- **Hook 驱动，几乎零开销**：agent 在事件发生时调用一个极小的 exe（emit 模式）写一行状态文件；
  托盘用 `FileSystemWatcher` 监听，秒级响应。除了那个托盘进程，没有任何额外常驻进程或轮询。
- **原生轻量**：C# + WinForms，用 Windows **系统内置的 csc.exe** 编译（无需装任何 SDK），
  `NotifyIcon` 原生托盘，`Console.Beep` 合成提示音（零音频文件），单 EXE ~20KB。
- **声音符合直觉**：等待=上升未解决音（催你），完成=上行解决音（积极）。可在托盘菜单静音/测试。

## 安装

> 仅 Windows。**无需安装任何 SDK/运行时**——用系统自带的 .NET 编译器构建，单 EXE 常驻约 23MB。

### 方式一：双击安装（最简单）

1. 下载本仓库（绿色 **Code → Download ZIP**，或 `git clone`），解压
2. **双击 `install.cmd`**

就这两步。`install.cmd` 会自动绕过 PowerShell 执行策略并运行安装。卸载就双击 `uninstall.cmd`。

### 方式二：命令行

```powershell
git clone https://github.com/mazjq/agentping
cd agentping
powershell -ExecutionPolicy Bypass -File install.ps1
```

### 方式三：便携包（免编译）

下载 Release 里的 `AgentPing-*.zip`（已含预编译 exe）→ 解压 → 双击 `install.cmd`。
自己生成便携包：`powershell -ExecutionPolicy Bypass -File package.ps1` → 产物在 `dist\`。

### 安装做了什么

编译（若便携包已带 exe 则跳过）→ 部署到 `%LOCALAPPDATA%\AgentPing\` → **合并** Claude Code hooks 到
`~/.claude/settings.json`（先备份 `.agentping.bak`，保留你已有的 hook）→ 写 Codex 的 `~/.codex/hooks.json`
→ 注册开机自启 → 启动托盘。

装完后 **重启正在运行的 Claude / Codex 会话**（hook 在会话启动时加载）。

可选参数：`-NoStart`（只装不启动）、`-NoAutoStart`（不开机自启）、`-NoClaude`（不动 Claude 配置）、`-NoCodex`（不动 Codex 配置）。

### Claude Code（已自动接入）

安装脚本自动挂这些 hook（不覆盖你已有的 hook）：

| Hook 事件 | 映射状态 |
|---|---|
| `UserPromptSubmit` / `PreToolUse` | 处理中 🔵 |
| `PermissionRequest` | 等待确认 🟠（权限框即将弹出时**立即**触发，无延迟） |
| `PostToolUse` | 处理中 🔵（批准并跑完工具后回到蓝灯的关键） |
| `Notification` | 智能区分：空闲"等你输入"→完成 🟢（不误报）；其它通知→等待 🟠 |
| `Stop` | 已完成 🟢 |
| `SessionEnd` | 移除会话 |

> ⚠️ Claude 的 `PreToolUse` 在**权限提示之前**触发，"批准"动作本身不产生事件——所以靠 `PermissionRequest`（即时弹）和 `PostToolUse`（跑完回蓝）这对组合才能让颜色准确。改完 hook **必须重启 Claude 会话**才生效（hook 在会话启动时加载）。

### Codex（已自动接入，不碰 notify）

Codex 有独立的 hook 系统，与 `notify` 互不干扰。安装脚本写到**全局 `~/.codex/hooks.json`**
（`UserPromptSubmit`/`PreToolUse`/`PostToolUse`→处理中，`PermissionRequest`→等待，`Stop`→完成），
**完全不动**你被 computer-use 占用的 `notify`。
⚠️ **必须用 `hooks.json` 而非 `config.toml`**——Codex 桌面 app 不派发 config.toml 的 hook（[openai/codex#16430](https://github.com/openai/codex/issues/16430)）。详见
[`hooks/codex-setup.md`](hooks/codex-setup.md)。改完需**重启 Codex**生效。

### pi 及任意 agent

见 [`hooks/generic-setup.md`](hooks/generic-setup.md)。协议就一行命令：
`AgentPing.exe --emit --agent <名> --status <processing|waiting|done|end> --key <会话>`。

## 托盘菜单

- 顶部：当前聚合状态 + 各状态计数
- 每个活动会话一行：`🔵 claude · 处理中 · 1m20s [项目名]`
- 🔇 静音 / 🔊 测试声音（等待音 / 完成音）
- 📁 打开状态目录 · ⏻ 开机自启 · ❌ 退出
- 双击托盘图标 = 打开状态目录

## 卸载

```powershell
powershell -ExecutionPolicy Bypass -File uninstall.ps1
```

停进程 → 从 settings.json 移除我们加的 hook（保留你其它 hook）→ 删自启 → 删安装目录。
加 `-KeepState` 可保留状态/配置目录。Codex 若手动接过，需自己把 `config.toml` 的 `notify` 改回。

## 文件结构

```
agent-status-notifier/
├─ src/
│  ├─ Core.cs               纯状态逻辑（无 UI，可测试）：状态机/聚合/跃迁/推断
│  └─ AgentPing.cs          UI + emit 入口（tray + emit 双模式，C# 5 兼容）
├─ tests/Tests.cs           Core 的断言测试（38 项）
├─ build.ps1                用内置 csc 编译 -> bin/AgentPing.exe
├─ run-tests.ps1            编译并运行测试
├─ package.ps1             打包自包含便携 zip -> dist/
├─ install.cmd / uninstall.cmd   双击即装/卸载(绕过执行策略)
├─ install.ps1 / uninstall.ps1
├─ bin/AgentPing.exe        构建产物
└─ hooks/
   ├─ codex-setup.md         Codex [[hooks]] 接入说明
   ├─ codex-notify-chain.ps1 备选：Codex notify 转发脚本（一般用不到）
   └─ generic-setup.md       pi 及任意 agent 通用接入
```

运行时数据（不在仓库内）：`%LOCALAPPDATA%\AgentPing\`
├─ `AgentPing.exe`（已部署的可执行文件）
├─ `state\*.json`（每会话一个状态文件）
├─ `status.json`（当前聚合状态，供外部脚本查询：`{"agg":"waiting","sessions":1,"ts":...}`）
├─ `events.log`（诊断日志）
└─ `config.json`（静音等设置）

## 开发：测试 + 重新编译

```powershell
powershell -ExecutionPolicy Bypass -File run-tests.ps1   # 跑核心逻辑测试(38项)
powershell -ExecutionPolicy Bypass -File build.ps1       # 编译
powershell -ExecutionPolicy Bypass -File install.ps1     # 重新部署 + 重启托盘
```

状态机/聚合/跃迁/推断等核心逻辑都在 `src/Core.cs`（无 UI 依赖），改动请先在 `tests/Tests.cs` 加用例、跑绿再改实现（TDD）。

## 已知限制 / TODO

- Codex 的 `notify` 主要在回合结束触发，所以 Codex 侧只能粗粒度反映"完成/该你了"，
  不像 Claude 能精细区分"处理中"。这是 Codex 接口限制。
- pi 的具体 hook 机制待确认（先用通用 wrapper 兜底，确认后补 `generic-setup.md`）。
- 提示音用 `Console.Beep` 合成；如需自定义 WAV，可后续在 `SoundEngine` 加 `SoundPlayer` 分支。
