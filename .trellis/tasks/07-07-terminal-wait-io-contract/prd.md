# 设计 terminal wait IO 交互合同

## Goal

设计 terminal / exec wait activity 的 Agent-facing IO 合同，让 `wait` 对已完成或失败的终端命令提供足够的 bounded decision surface，同时保持完整 stdout/stderr 由 `shell_exec read` 读取。该设计需要为后续 channel 系统预留干净的 source/result refs，不把 terminal output 或 channel 语义固化在自由文本里。

## Background

当前 `wait` 对 exec activity 主要返回 `running/completed` 等状态，完成后 Agent 往往还需要再调用 `shell_exec read` 才能知道命令是否成功、stderr 是什么、下一次从哪个 seq/cursor 读取。这让简单命令交互显得笨重，也让失败决策点滞后。

用户反馈期望：

- `wait` 对 exec completed item 可附带 bounded output preview，例如最后 4KB stdout/stderr 和 exit_code。
- `wait` item 可返回 `next_seq` / `after_seq` 建议值，避免 Agent 手动猜读取位置。
- 可以考虑 `shell_exec wait_read` 或 `read_on_complete=true` 的 convenience mode。
- `wait` 对失败命令应突出 exit code 和 stderr 摘要。

## First Principles

- Terminal / exec runtime state 是执行事实源。
- `wait` 是 bounded observer，不是完整 stdout/stderr 传输通道。
- `shell_exec read` 是 terminal output body 的读取 owner。
- Workspace waiting projection 与 Agent-facing `wait` 应共享状态、refs 和 bounded preview 语义。
- 后续 channel 系统可能接管跨来源消息建模；本设计需要保留 source、target、cursor、payload refs、diagnostic refs 的结构化字段，不把语义塞进一段自然语言。

## Requirements

1. `wait` 对 exec completed/failed/cancelled/lost item 返回明确 terminal status、exit_code、bounded stdout/stderr preview、truncation 信息。
2. `wait` item 返回下一步读取建议：`next.tool=shell_exec`、`operation=read`、`terminal_id`、`after_seq` / `next_seq` 或等价 cursor。
3. later wait 能打捞已经完成的 terminal state，不要求 wait 调用必须在命令运行期间挂着。
4. `wait` 不返回完整 stdout/stderr；大输出必须通过 `shell_exec read` 或未来 output channel 读取。
5. 输出 preview 必须 bounded，且 stdout/stderr 分开表达，便于 Agent 判断失败原因。
6. 设计需要明确 terminal resource state、exec command state、RuntimeSession delivery terminal state 的边界，避免 `lost/failed/completed` 互相冒充。
7. 设计需要预留 channel-friendly IO：source identity、activity ref、output cursor、payload refs、diagnostic refs、bounded projection text 分层清晰。

## Acceptance Criteria

- [ ] PRD/design 明确 terminal wait authority map：terminal/exec state owner、wait observer、read owner、workspace projection。
- [ ] 给出 `WaitActivityItem` exec completed/failed 的目标 JSON shape，包括 preview、exit_code、cursor/ref、next read。
- [ ] 明确 `shell_exec wait_read` / `read_on_complete=true` 是否进入首期实现；若暂缓，写清原因和后续接入点。
- [ ] 覆盖 race：terminal 已完成后新 wait 可返回结果；wait timeout 不消费 terminal output；read cursor 不丢。
- [ ] 明确 future channel 迁移时哪些字段可映射到 channel receipt / payload ref。

## Out Of Scope

- 本任务先做设计，不直接实现 terminal wait 改动。
- 不改变 terminal output 持久化 owner。
- 不实现完整 channel 系统。
