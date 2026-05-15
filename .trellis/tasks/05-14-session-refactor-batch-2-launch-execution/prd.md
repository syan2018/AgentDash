# Session Refactor Batch 2：LaunchExecution 与 Prompt Pipeline 拆分

## Goal

把一次 launch 的执行前解析从 `start_prompt_with_follow_up` 中抽出，形成可测试的 `LaunchExecution`。Batch 2 不迁移所有入口，也不删除 `PromptSessionRequest`，只把当前 pipeline 中最容易漂移的 prompt payload、lifecycle、restore、hook reload、pending apply plan、connector input summary 前置为明确计划。

## Requirements

- 新增 `session/launch`，定义 `LaunchCommand` 的最小过渡输入与 `LaunchExecution`。
- `LaunchExecution` 消费旧 `PromptSessionRequest`、`SessionConstructionPlan`/owner fact、session meta/runtime facts，输出 connector boundary 所需字段组。
- `start_prompt_with_follow_up` connector boundary 不再直接构造 `ExecutionContext`，而是通过 `LaunchExecution` 输出 connector projection 与 summary。
- `ExecutionContext` 仍作为 connector 边界投影，不升级为主链路事实源。
- connector prompt 前可以记录或断言 launch summary：session id、turn id、lifecycle、restore mode、pending apply count、working dir、VFS/MCP/capability 来源。
- connector setup / prompt failure 的 terminal 与 pending 语义保持 Batch 0 characterization 所固定的现状。

## Acceptance Criteria

- [x] `LaunchExecution` 有覆盖 owner bootstrap 与 repository restore summary 的单元测试。
- [x] `start_prompt_with_follow_up` 通过 `LaunchExecution` 构造 connector input。
- [x] connector prompt 前可获得 launch summary，便于 Batch 3 迁移入口。
- [x] 现有 pending apply-once 与 connector setup failure 测试继续通过。
- [x] 不新增空壳 launch service；不删除 `PromptSessionRequest`。

## Completion Notes

- 已新增 `session/launch.rs`，定义 `LaunchExecution`、`LaunchExecutionInput` 与 `LaunchSummary`。
- `prompt_pipeline.rs` 已将 `ExecutionContext` 构造迁入 `LaunchExecution::build`，connector prompt 前会形成 summary trace。
- lifecycle、VFS/MCP/capability fallback 的更深层纯解析仍在 pipeline 内；直接一次性外搬会牵动 hook runtime、skill discovery、tool building 等副作用边界，后续应继续在 Batch 2 follow-up 或 Batch 3 前完成，而不是在本提交里伪装完成。

## Out of Scope

- 不迁移 HTTP/Task/Workflow/Routine/Companion/Hook/Local relay 到新 command。
- 不删除 `SessionLaunchIntent`。
- 不拆 runtime registry。
- 不做 terminal outbox。
