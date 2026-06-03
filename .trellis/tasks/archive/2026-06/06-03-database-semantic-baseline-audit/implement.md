# 实施计划

## Checklist

- [x] 启动任务并派发模块化 schema 语义研究。
- [x] 汇总研究材料，生成正式评估报告。
- [x] 对报告中的高风险建议做主会话复核。
- [x] 给出下一步 schema/code 收敛实施切片。
- [x] 将业务语义/职责归属问题拆入 `06-03-database-business-semantic-convergence`。
- [x] 实施 Slice 1：P0 baseline correctness。
- [x] 实施 Slice 2：hand-curated `0001_init.sql`。
- [x] 运行必要验证，确认空库 init、readiness、repository contract 与 baseline 一致。

## Research Slices

- Core business：projects / stories / workspaces / project_agents / project_vfs_mounts / canvases
- Session runtime：sessions / session_events / session runtime commands / compactions / lineage / projection tables
- Workflow lifecycle：agent_procedures / workflow_graphs / lifecycle_runs / workflow instances / agents / frames / assignments / anchors / gates
- Platform assets and config：library_assets / skill_assets / inline_fs_files / mcp_presets / extension artifacts / settings / auth / LLM providers
- Backend/local runtime：backends / runtime_health / backend leases / project backend access / inventory / views / user_preferences

## Validation Commands

- `rg -n "<table_or_field>" crates packages .trellis/spec`
- Programmatic comparison of `0001_init.sql` table/column definitions against repository SQL constants and row structs.
- `cargo check -p agentdash-infrastructure`
- `cargo check -p agentdash-api`

## Deliverable

- `.trellis/tasks/06-03-database-semantic-baseline-audit/report.md`
- `crates/agentdash-infrastructure/migrations/0001_init.sql` 与 Slice 1/2 对齐。
- Repository/domain 的最小同步修复。
