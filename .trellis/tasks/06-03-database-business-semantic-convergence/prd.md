# 数据库业务语义收敛

## Goal

承接 `06-03-database-semantic-baseline-audit` 报告中不适合与 baseline 正确性修复混在一起的业务语义问题，系统性收敛数据库表/字段的事实归属、projection 边界、runtime audit 边界和 UI/config 边界。

用户价值：

- 避免当前 `0001_init.sql` 把可运行的旧模型误固化为长期领域模型。
- 将 Session、Lifecycle、Backend、UI preference 等跨层职责拆清楚，减少后续功能继续依赖错误字段。
- 在项目未上线阶段直接收敛到正确模型，不为旧库兼容妥协。

## Background

`database-semantic-baseline-audit/report.md` 已完成 55 张表分区审计，并建议当前任务先处理 Slice 1/2：

- P0 baseline correctness。
- hand-curated `0001_init.sql`。

以下问题需要单独任务承接，因为它们涉及 repository/domain/API/frontend contracts 或产品语义决策，不应混进第一轮 baseline 机械修复：

- `sessions` 作为 RuntimeSession head 的职责收敛。
- `lifecycle_runs.active_node_keys` 与 `execution_log` 的 projection/audit 拆分。
- `views` / `user_preferences` 的删除或迁移。
- `backends` local identity/share/claim 字段边界整理。
- `stories.task_count`、`project_agents.is_default_for_task` 等业务字段去冗余。
- `canvas_bindings`、LLM credential 字段、permission grant JSONB 查询等跨层语义整理。

## Requirements

- 逐项评估报告中 “Requires-Code-Change Candidates” 和 “业务语义/职责归属问题”。
- 每个改动必须先确认目标事实归属：business fact、runtime fact、projection/cache、outbox/audit、seed/config 或 UI state。
- 删除字段前必须同步 repository、domain、API contracts、frontend usage 和相关 spec。
- 不做旧开发库兼容迁移；直接修改 init baseline 与代码到目标形态。
- 对仍需保留的 projection/cache 字段，必须给出事实源和重建路径。

## Acceptance Criteria

- [ ] `sessions` 不再承载业务归属、provider/executor 行为和 UI layout state，或保留项已明确标注为 projection。
- [ ] `lifecycle_runs` 不再把 activity projection 与 runtime execution audit 当作 ledger row 的事实源。
- [ ] `views` / `user_preferences` 删除或迁移到 scoped UI/settings 模型。
- [ ] `backends` local identity/share/claim 状态与 backend registration config 的边界被整理。
- [ ] `stories.task_count`、`project_agents.is_default_for_task` 等冗余业务字段完成删除、迁移或明确保留理由。
- [ ] API/generated/frontend contract 与数据库目标模型一致。
- [ ] 相关 Trellis spec 更新为目标语义，而不是记录旧实现问题。

