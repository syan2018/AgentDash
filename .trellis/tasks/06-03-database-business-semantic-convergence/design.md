# 数据库业务语义收敛设计

## Slice 1: Session Runtime Head

Target:

`sessions` 表只表达 runtime trace shell：

- runtime session id
- title / title source projection
- event sequence head
- delivery status / last turn pointers
- runtime trace metadata needed by session page drill-down

业务归属通过 `runtime_session_execution_anchors -> AgentFrame -> LifecycleRun -> LifecycleSubjectAssociation` 查询。`project_id` 若仅用于 UI navigation，应由 anchor/read model 派生；`tab_layout_json` 属于 scoped UI state；`executor_config_json` 属于 AgentFrame execution profile 或 connector launch input。

## Slice 2: Lifecycle Run Ledger

Target:

- `WorkflowGraphInstance.activity_state_json` 是 Activity runtime state fact source。
- active projection 由 sibling task `06-02-lifecycle-run-active-projection-structure` 派生。
- execution audit 应是 append-only event / transition / trace surface，不应作为 `lifecycle_runs.execution_log` 的长期事实源。

本 slice 与 active projection task 有依赖关系：不要在两边重复实现 active ref。数据库任务负责 schema/repository 对齐，active projection task负责 read model 和业务使用退场。

## Slice 3: UI / Settings Tables

Target:

- 用户偏好进入 scoped settings：`settings(scope_kind, scope_id, category, key, value)` 或现有 settings contract 的目标形态。
- Saved views 若保留，必须明确 user/project scope 和 owner module；不应作为 backend repository 的无作用域配置。
- 删除 legacy `views` / `user_preferences` 前先确认 frontend settings / backend view consumers。

## Slice 4: Business Redundancy

Target:

- `stories.task_count` 是可从 tasks 按 story 聚合的 projection；若保留必须有 projection owner 和 refresh path。
- `project_agents.is_default_for_task` 与当前 dispatch policy / `default_lifecycle_key` 边界需要统一；若 task default agent 仍是产品配置，应迁入显式 dispatch preference / project policy，而不是混在 agent row。
- `projects.visibility/is_template` 若目前属于产品事实，可保留；若只是未来权限/模板系统占位，应移出当前 baseline。

## Slice 5: Backend Runtime Identity

Target:

Backend registration config、local machine identity、share scope、claim lifecycle 分别命名。若继续共表，字段名、CHECK 和 indexes 必须表达各自事实类型，避免 registration row 同时承担 runtime lease。

## Slice 6: Typed Cleanup

Target:

- LLM credential 字段命名与 provider/credential fact source 对齐。
- permission grant JSONB 查询改为 typed columns / typed scope index where needed.
- canvas binding source contract 与 VFS / project surface source 对齐。

## Validation

每个 slice 完成后都要跑与修改范围匹配的 backend check、contract check、frontend typecheck/test。跨 slice 修改完成后再跑 workspace 级检查。
