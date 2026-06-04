# 接入任意 agent（pi 及其它）

AgentKnocks 的状态采集协议极简：**任何东西**只要在状态变化时调用一次 exe，即可被托盘聚合显示。

## 命令

```
AgentKnocks.exe --emit --agent <名称> --status <processing|waiting|done|end> [--key <会话ID>] [--title <显示名>]
```

- `--agent`  agent 名称，如 `pi` / `claude` / `codex`，决定托盘里那一行的标识。
- `--status`
  - `processing` 正在处理（蓝）
  - `waiting`    等待确认（橙，响"等待音" + 气泡）
  - `done`       处理完成（绿，响"完成音" + 气泡）
  - `end`        会话结束（删除该状态，从列表移除）
  - `auto`       从 stdin/参数里的事件文本自动推断（含 complete/finished→done，approval/permission/input→waiting）
- `--key`    会话唯一 ID（同一会话多次上报用同一个 key 才会更新同一行）。不传则尝试从 stdin JSON 的 `session_id`/`session` 提取，再不行用 `<agent>-default`。
- `--title`  托盘里显示的项目名/描述。不传则从 stdin JSON 的 `cwd`/`workdir` 取末级目录名；仍无则沿用上次的 title。

也可以把一段 JSON 通过 **stdin** 管道喂进去（hook 场景常见），exe 会自动解析 `session_id` / `cwd` 等字段。

## 例子

```powershell
$exe = "$env:LOCALAPPDATA\AgentKnocks\AgentKnocks.exe"

# pi 开始干活
& $exe --emit --agent pi --status processing --key job-42 --title "数据清洗"

# pi 需要你确认
& $exe --emit --agent pi --status waiting --key job-42

# pi 干完了
& $exe --emit --agent pi --status done --key job-42

# 会话结束，移除
& $exe --emit --agent pi --status end --key job-42
```

## 接入思路

- **有 hook/回调机制的 agent**：在"开始/等待输入/完成"三个时机各挂一条上面的命令。
- **只有完成回调的 agent**：至少挂 `done`，完成时响一声。
- **纯命令行包装**：写个 wrapper，`processing` 在调用前，`done`/`end` 在调用后。

> 关于 **pi**：若 pi 暴露了生命周期 hook，按"三时机"接入即可；否则用 wrapper 兜底。
> 把 pi 的具体接法补充到这里，方便复用。
