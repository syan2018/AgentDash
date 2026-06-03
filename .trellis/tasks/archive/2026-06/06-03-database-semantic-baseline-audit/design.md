# 数据库语义基线正式评估设计

## Scope

本任务评估当前 PostgreSQL baseline 的语义正确性，范围包括：

- `crates/agentdash-infrastructure/migrations/0001_init.sql`
- PostgreSQL repository SQL / row mapper / readiness
- domain repository trait、实体和值对象
- application use case 对持久化事实的使用方式
- API route 和 generated/frontend contract 中对字段语义的暴露
- `.trellis/spec/backend` 与 `.trellis/spec/cross-layer` 中的当前架构约束

## Evaluation Model

每张表和关键字段按事实类型分类：

| 类型 | 含义 | 处理原则 |
| --- | --- | --- |
| Business fact | Project/Story/Workspace/Asset 等业务事实 | 保留在业务聚合表，避免混入 runtime 状态 |
| Runtime fact | Session event、runtime command、lease、lifecycle execution 等运行事实 | 保留在 runtime/control-plane 表，避免回灌业务实体 |
| Projection/cache | 可由事实派生的读模型或缓存 | 标注派生来源；评估是否应该保留物化 |
| Outbox/audit | effect outbox、permission grant、state change 等审计/投递事实 | 保留时说明生命周期和清理策略 |
| Seed/config | builtin asset、settings、provider config 等启动/用户配置 | schema 只建表，数据由 seed/use case 写入 |
| Historical residue | 旧模型字段、旧命名、旧回填默认值 | 新 baseline 中删除或改名 |

## Output Shape

正式报告写入 `report.md`，包含：

- Executive summary
- High-confidence cleanup candidates
- Requires-code-change candidates
- Keep-but-normalize candidates
- Keep-as-is rationale
- Table-by-table audit appendix
- Proposed implementation slices
- Validation plan

模块研究材料写入 `research/`，由子代理负责不同分区。

## Non-goals

- 本任务不直接修改生产 schema/code，除非只是在任务文档内记录评估。
- 本任务不为旧开发库提供兼容 migration。
- 本任务不把 dump 格式美化当作唯一目标；真正目标是 schema 语义归属正确。
