# W1 Lifecycle Task Facts

## 状态

pending

## 依赖

- W0 done

## 目标

在 domain 层建立 `LifecycleRun.tasks` 计划项事实模型，包括 Task value object、计划状态枚举和 aggregate mutation。

## 输入

- `design.md` 的 Task / 状态机 / Assignment 章节。
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `crates/agentdash-domain/src/workflow/lifecycle_run.rs`
- `crates/agentdash-domain/src/task/value_objects.rs`
- `crates/agentdash-domain/src/task/entity.rs`

## 范围

- 在 `LifecycleRun` aggregate 上表达 `tasks` 集合。
- 定义 Task plan item 字段：`id`、`title`、`body`、`status`、`priority`、`created_by_agent_id`、`owner_agent_id`、`assigned_agent_id`、`source_task_id`、`created_at`、`updated_at`、`archived_at`、`context_refs`、`story_ref`。
- 将 Task plan status 收敛为 `open / active / review / blocked / done / dropped`。
- 提供 create / update / archive / status transition mutation。
- 保证 Task facts 不包含 runtime status、artifacts 或 `dispatch_preference`。

## 范围边界

- 该节点只落 domain aggregate 和 value object，原因是 migration、API 和前端都依赖这里稳定后的字段与状态语义。
- PostgreSQL、route 和 UI 变更分别进入 W2、W4 和 W5，原因是它们需要消费已经稳定的 domain contract。

## 验收

- `LifecycleRun.tasks` 默认空集合。
- aggregate tests 覆盖 create / update / archive / status transition。
- 旧 runtime TaskStatus 不再作为 plan status 的 domain contract。
- Story domain 不再持有 Task durable facts 的目标接口。

## 产出记录

- 待填写。

## 风险与交接

- W2 需要稳定的 serialization shape 和 default value。
- W3 需要稳定的 Task plan status 与 DTO 字段。
