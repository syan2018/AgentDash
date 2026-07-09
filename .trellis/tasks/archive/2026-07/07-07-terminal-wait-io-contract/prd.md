# 收束 terminal wait IO 交互合同

## Goal

实现 terminal / exec wait activity 的首期 Agent-facing IO 合同，让 `wait` 对已完成或失败的终端命令提供足够的 bounded decision surface，同时保持完整 stdout/stderr 由 `shell_exec read` 读取。该实现需要为后续 channel 系统预留干净的 source/result refs，不把 terminal output 或 channel 语义固化在自由文本里。

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
- terminal wait 的 system message / projection 语义应与 companion mailbox delivery 对齐：结构化来源、activity/output refs、bounded summary 与 full body owner 分离；不能把终端输出或系统通知伪装成 human input。
- 后续 channel 系统可能接管跨来源消息建模；本设计需要保留 source、target、cursor、payload refs、diagnostic refs 的结构化字段，不把语义塞进一段自然语言。

## Requirements

1. `wait` 对 exec completed/failed/cancelled/lost item 返回明确 terminal status、exit_code、bounded stdout/stderr preview、truncation 信息。
2. `wait` item 返回下一步读取建议：`next.tool=shell_exec`、`operation=read`、`terminal_id`、`after_seq` / `next_seq` 或等价 cursor。
3. later wait 能打捞已经完成的 terminal state，不要求 wait 调用必须在命令运行期间挂着。
4. `wait` 不返回完整 stdout/stderr；大输出必须通过 `shell_exec read` 或未来 output channel 读取。
5. 输出 preview 必须 bounded，且 stdout/stderr 分开表达，便于 Agent 判断失败原因。
6. 设计需要明确 terminal resource state、exec command state、RuntimeSession delivery terminal state 的边界，避免 `lost/failed/completed` 互相冒充。
7. 设计需要预留 channel-friendly IO：source identity、activity ref、output cursor、payload refs、diagnostic refs、bounded projection text 分层清晰。
8. 若实现需要向 session feed/system event 暴露 terminal wait completion projection，应使用与 companion mailbox delivery 对齐的 `system_message` shape：`kind`、`origin`、`source`、`status`、`summary`、`result_refs` / `output_ref`，且不写入 `UserInputSubmitted`。

## Acceptance Criteria

- [ ] `wait(activity_refs=[terminal_id])` 对已完成 terminal 返回 `completed` / `failed`，并根据 exit code 区分成功失败。
- [ ] failed exec wait item 包含 `exit_code`、bounded `stderr_preview`、bounded `stdout_preview`、truncation 信息和 `shell_exec read` continuation refs。
- [ ] completed exec wait item 可包含 bounded stdout/stderr preview，但完整输出仍只能通过 `shell_exec read` 或现有 terminal output owner 获取。
- [ ] later wait 能打捞已经完成的 terminal state；wait timeout 不消费 terminal output。
- [ ] `WaitActivityItem.result_refs` / detail payload 保留 channel-friendly refs：`terminal_id`、`output_ref`、cursor/seq、diagnostic。
- [ ] 若有 terminal system projection，shape 与 companion mailbox `system_message` 一致，并且不进入 human input。
- [ ] `shell_exec wait_read` / `read_on_complete=true` 不进入首期实现，除非实现阶段发现已有自然扩展点且不新增第二套 wait protocol。
- [ ] Rust targeted tests 覆盖 running/completed/failed/lost/cancelled、bounded preview、later wait salvage 和 next read refs。

## Out Of Scope

- 不改变 terminal output 持久化 owner。
- 不实现完整 channel 系统。
- 不实现 `shell_exec wait_read` / `read_on_complete=true` convenience mode，除非它只是复用 wait/read owner 的薄 wrapper。
