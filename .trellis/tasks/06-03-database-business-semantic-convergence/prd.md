# 数据库业务语义收敛

## Goal

承接已归档的 `06-03-database-semantic-baseline-audit`，把当前 `0001_init.sql` 中仍会误导后续开发的业务语义字段收敛到正确事实源。当前 baseline correctness 和 migration reset 已完成；本任务只处理仍需 repository / domain / contracts / frontend 同步修改的业务语义问题。

目标状态：数据库 schema 表达长期目标模型，而不是把可运行的旧 projection、UI state、runtime trace helper 或冗余缓存固化为领域事实。

## Current Baseline

- migration 目录已收敛为 hand-curated `0001_init.sql`。
- Session runtime control source 已通过 RuntimeSessionExecutionAnchor / AgentFrameRuntimeView 收束，相关 task 已归档。
- `sessions` 仍保留 `executor_config_json`、`tab_layout_json`、`project_id` 等混合字段。
- `lifecycle_runs` 仍保留 `active_node_keys` 和 `execution_log`。
- `views`、`user_preferences` 仍以 legacy UI/settings 表存在。
- `stories.task_count`、`project_agents.is_default_for_task` 等冗余业务字段仍在 schema / contracts / frontend 中存在。
- `backends` local identity/share/claim 字段、LLM credential 命名和 permission JSONB 查询仍需要归属确认。

## Requirements

- 每个字段必须先归类为 business fact、runtime fact、projection/cache、outbox/audit、seed/config 或 UI state。
- 删除或迁移字段时必须同步 domain、repository、API contract、generated TS、frontend usage 和 spec。
- 对仍保留的 projection/cache 字段，必须写清事实源和重建路径。
- 不为旧开发库保留运行时兼容分支；migration 直接进入目标结构。
- 优先收敛与当前生命周期控制面 PR 合并最相关的 Session / Lifecycle / UI settings / business redundancy 字段。

## Acceptance Criteria

- [ ] `sessions` 只承载 RuntimeSession identity、event sequence head、display title projection、delivery/runtime trace metadata；业务归属、executor behavior、UI layout state 不再落在 session row。
- [ ] `lifecycle_runs.active_node_keys` / `execution_log` 不再作为 ledger row 的 runtime fact source；active projection 与 execution audit 有明确 owner。
- [ ] `views` / `user_preferences` 被删除或迁移到 scoped settings / UI state 模型。
- [ ] `stories.task_count`、`project_agents.is_default_for_task` 完成删除、迁移或被明确降级为可重建 projection。
- [ ] `backends` local identity/share/claim 与 backend registration config 的字段边界清晰。
- [ ] API contracts、generated TS、frontend usage 与目标 schema 一致。
- [ ] 相关 Trellis spec 记录目标事实归属和原因。

## Out Of Scope

- 不重新做 baseline dump reset。
- 不处理已归档 Runtime control source、Graphless default runtime、Session-Agent channel 的实现。
- 不保留旧 schema 兼容路径。
