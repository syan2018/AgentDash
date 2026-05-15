# Session Refactor Batch 1：Owner Resolver 与 Construction Plan

## Goal

建立 session 构建的第一份共享事实源：统一 owner 解析，并引入 `SessionConstructionPlan` 作为 launch 与 context query 的共同上游。Batch 1 的目标不是拆完整 launch pipeline，而是先收掉 owner priority 分裂和 route 层重复重建 context/VFS/capability 的主线。

## Requirements

- 新增 `session/ownership`，定义 `ResolvedSessionOwner` 与 `SessionOwnerResolver`。
- launch augment、context query、权限/展示投影必须使用同一 owner priority 与同一解释逻辑。
- 新增 `session/construction`，定义最小 `SessionConstructionPlan`、trace 与 context endpoint projection。
- `SessionConstructionPlan` 先覆盖 Batch 1 必需事实：session id、project id、owner、workspace root、context bundle/source、VFS、MCP、capability overlay、identity、execution profile、working dir policy trace。
- `/sessions/{id}/context` 改为从 construction projection 生成 response，不再在 route 层独立排序或解释 primary owner。
- 保留 `PromptSessionRequest` 作为旧 pipeline 边界，不在本批删除。
- 保留 `start_prompt_with_follow_up` 主执行流程，不在本批拆 `LaunchExecution`。

## Acceptance Criteria

- [x] `ResolvedSessionOwner` 有覆盖 Task / Story / Project priority 的单元测试。
- [x] context query 与 HTTP launch augment 使用同一 owner resolver；Batch 0 中 context owner priority characterization 被有意更新。
- [x] `SessionConstructionPlan` 能生成 context endpoint 所需 projection，并附带 trace 说明 owner/context 来源。
- [x] route 层不再包含独立 `Project -> Story -> Task` primary binding 排序逻辑。
- [x] `/sessions/{id}/context` 不再直接拼 response，而是通过 `SessionConstructionPlan` projection 输出。
- [x] 不引入只转发旧 request 的 `SessionLaunchService` 或等价空壳。

## Completion Notes

- 已新增 `session/ownership.rs`，将 owner priority 固定为 `Task -> Story -> Project`，并被 context query 与 HTTP launch augment 共用。
- 已新增 `session/construction.rs`，让 context endpoint 先形成 `SessionConstructionPlan` 再投影 API response。
- project/story/task 现有 context builder 仍暂留原位置；它们还被 project/story/vfs/canvas routes 复用，且依赖 API `AppState` 与 project-agent helpers。直接搬迁会扩大 Batch 1 风险，后续应单独做 construction use-case 下沉，而不是在本批强迁。

## Out of Scope

- 不删除 `PromptSessionRequest`。
- 不迁移所有入口到 `LaunchCommand`。
- 不拆 `LaunchExecution`。
- 不拆 terminal effects / pending runtime command / runtime registry。
- 不收紧 `working_dir` 安全策略；本批只让 construction trace 暴露当前策略来源。
