# Session Refactor Batch 0：Characterization 与安全护栏

## Goal

为 session 系统性重构建立现状行为护栏。Batch 0 不改变生产架构，不引入新的 launch/construction 主线，只通过测试和最小 fixture 固定当前关键行为，保证后续 Batch 1+ 的重构能明确区分“预期迁移”与“误伤回归”。

## Parent Context

父任务：`.trellis/tasks/05-14-session-launch-refactor-assessment`

父任务已确认目标主链路：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution
```

Batch 0 是正式实现前的安全网，范围来自父任务 `implement.md` 的 Batch 0。

## Requirements

- 固化现有入口矩阵的关键行为：HTTP、Task、Workflow、Routine、Companion dispatch、Companion parent resume、Hook auto-resume、Local relay。
- 固化当前 fallback 行为：owner priority、VFS、MCP、capability、executor config、working_dir、follow-up、hook reload、repository restore。
- 固化关键失败路径：connector.prompt failure、owner bootstrap commit、pending capability transition 消费、terminal effects 触发。
- 优先补充低耦合单元测试；必要时添加小型 characterization helper，不重写生产逻辑。
- 不引入新的 `SessionLaunchService` / `SessionConstructionPlan` / `LaunchExecution` 生产实现。
- 不删除 `PromptSessionRequest`、`SessionLaunchIntent`、`SessionHub` wrapper。

## Acceptance Criteria

- [x] 新增或补强测试覆盖 `PromptSessionRequest` / `PreparedSessionInputs` / `finalize_request` 的当前合并语义。
- [x] 新增或补强测试覆盖 owner priority 现状差异：launch augment 使用 Task -> Story -> Project，context query 使用 Project -> Story -> Task。
- [x] 新增或补强测试覆盖 `start_prompt_with_follow_up` 的关键 fallback 或失败路径，至少包括 pending transition apply-once 或 connector failure 之一。
- [x] 新增或补强测试覆盖 `resolve_working_dir` 当前允许绝对路径 / `..` 的现状，作为后续安全收紧的行为变更护栏。
- [x] 验证命令至少覆盖 touched package 的 focused tests。
- [x] 不修改生产架构边界，不引入只转发旧 request 的空壳 service。

## Completion Notes

- `assembler.rs` 新增 `post_turn_handler` prepared/base 合并语义测试，补齐 `finalize_request` 对 hook/effect 回调载体的当前行为护栏。
- `path_policy.rs` 新增绝对路径与 `..` 当前允许行为测试，供后续 Batch 7 安全收紧时有意修改。
- `acp_sessions.rs` 新增 context query primary binding priority 测试，固定当前 `Project -> Story -> Task` 行为；Batch 1 引入单一 owner resolver 后应有意更新。
- 已复用既有 `hub/tests.rs` 覆盖 pending transition apply-once 与 connector setup failure terminal 记录。

## Out of Scope

- 不实现 `SessionConstructionPlan`。
- 不实现 `LaunchExecution`。
- 不迁移入口 adapter。
- 不拆 runtime registry、effects outbox、pending command store。
- 不收紧 `working_dir` 行为；这里只记录当前不安全语义。
