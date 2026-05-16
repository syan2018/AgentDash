# Session 重构收尾清理 Design

## Scope

本轮只推进 `TurnSupervisor` 对 turn 后台任务的可见性，作为 `LaunchExecutor / TurnSupervisor` 目标态的一步收口。

## Current Shape

`prompt_pipeline::spawn_stream_adapter` 直接 `tokio::spawn` adapter task，但返回值被丢弃。`TurnSupervisor` 只在 `TurnExecution` 中记录 `processor_tx` 和 cancel 标记，因此无法在 turn 清理时主动中止 adapter task。

## Target Shape

- `TurnExecution` 增加 `stream_adapter_abort: Option<tokio::task::AbortHandle>`。
- `prompt_pipeline::spawn_stream_adapter` 返回 `JoinHandle<()>`。
- prompt pipeline 创建 adapter 后调用 `TurnSupervisor::register_stream_adapter_handle`。
- `TurnSupervisor::clear_active_turn` 和 `clear_turn_and_hook` 在释放 active turn 前中止已登记 adapter。

## Behavioral Contract

- 正常 stream 结束时 adapter task 自然完成，processor 发 terminal，processor 调 `clear_active_turn`。此时 abort 已完成任务是幂等安全的。
- cancel 路径仍先给 processor 发送 interrupted terminal；processor 终态清理时中止 adapter，避免后续继续读 stream。
- 如果 connector.prompt 失败，adapter 尚未创建，现有 `clear_turn_and_hook` 行为不变。

## Tradeoffs

- 不把 processor task handle 一并纳入 supervisor，因为 `SessionTurnProcessor` 当前封装了 join handle 且终态由 processor 自己收束；一起改会扩大范围。
- 使用 `AbortHandle` 而不是保存 `JoinHandle`，避免 `TurnExecution` 需要承担 join 语义，也保留当前 Clone 形态。
