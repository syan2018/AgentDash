# W1 Lifecycle Task Facts

## 状态

done

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

- 在 `crates/agentdash-domain/src/workflow/value_objects/task_plan.rs` 新增 `LifecycleTaskPlanItem`、`LifecycleTaskPlanItemDraft`、`LifecycleTaskPlanItemPatch`、`TaskPlanStatus`、`TaskPriority`。
- 在 `LifecycleRun` aggregate 上新增 `tasks: Vec<LifecycleTaskPlanItem>`，构造函数默认空集合。
- 在 `LifecycleRun` aggregate mutation 中新增 `create_task`、`update_task`、`archive_task`、`transition_task_status`、`task_by_id`，并校验 task id 唯一、title 非空、`story_ref.kind == "story"`。
- `TaskPlanStatus` 收敛为 `open / active / review / blocked / done / dropped`，并按计划状态机限制 transition；`archive_task` 负责软归档并标记 `dropped`。
- focused domain tests 覆盖默认空集合、create、update、archive、status transition，并断言新 Task facts 序列化不含 `dispatch_preference`、`artifacts`、execution/runtime status 字段。

## 风险与交接

- W2 需要把 `lifecycle_runs.tasks` migration、repository row mapping、INSERT/UPDATE/SELECT 序列化接到本次新增的 `LifecycleTaskPlanItem` shape；本节点未做 migration/repository。
- W2 接入 repository 前，workspace 级 `cargo check --workspace` 预计会因为 `LifecycleRun` 新增字段但 repository row mapping 尚未补齐而不可作为本节点验收依据。
- W3 可直接消费 `TaskPlanStatus` 和 `LifecycleTaskPlanItem` 作为 DTO 字段来源；旧 `agentdash_domain::task::TaskStatus` 仍存在，留给后续 contract/API/MCP/frontend 节点拆除旧 surface。
