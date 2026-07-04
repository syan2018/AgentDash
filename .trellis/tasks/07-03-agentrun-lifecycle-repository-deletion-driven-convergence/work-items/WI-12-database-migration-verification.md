# WI-12 Database Migration Verification

## Objective

统筹本轮破坏式 schema 变更，确保表重命名、字段删除、ownership 调整、FK/cascade、索引和数据迁移与正式决策一致。

## Decisions

D-003, D-005, D-010, D-011, D-013, D-016, D-017, D-019

## Research Inputs

- `research/database-physical-design.md`
- `research/command-mailbox-delivery.md`
- `research/wi-04-command-mailbox-current-state.md`
- `research/runtime-session-internal-model.md`
- `research/agentframe-context-surface.md`
- `research/fork-lineage-baseline.md`

## Scope

- 维护 schema change ledger。
- 为每个工作项记录：新增表、删除表、字段迁移、FK/cascade、唯一约束、索引、backfill。
- 维护 redundant table ledger：每个疑似冗余表必须给出删除、合并、降级或保留结论。
- 验证 mailbox ownership 从 RuntimeSession 转向 AgentRun。
- 验证 runtime session trace 表命名和 FK/cascade。
- 验证 AgentFrame revision append-only 约束。
- 验证 fork lineage baseline 约束。
- 验证 current delivery binding / projection 的恢复和约束。

## Out Of Scope

- 不独立决定领域边界；只执行已被对应工作项和 `decisions.md` 接受的 schema 方案。
- 不保留旧 API/schema 兼容路径。

## Dependencies

依赖 WI-00 inventory。实际 migration 随 WI-02、WI-04、WI-06、WI-07、WI-08、WI-10 分批进入。

## Implementation Notes

- 项目未上线，migration 可以破坏式删除旧字段和旧表。
- 每个 migration 应让 schema 更接近事实所有权，而不是留下长期双写。
- 对 child table 保留的事实，需要在代码入口上隐藏为父聚合能力。

## Acceptance

- 每个 schema change 都能映射到 D-016 / D-017 的分类理由。
- 每个保留物理表都能映射到 D-016 / D-017 / D-019 的正向资格。
- 每个删除或合并的物理表都有 canonical replacement、数据迁移或可重建说明。
- 删除 RuntimeSession 不会 cascade 删除 AgentRun-owned durable facts。
- AgentRun delete 的 cascade 或显式 cleanup 覆盖 mailbox、receipts、frames、anchors、lineage、gates、subjects 等 run-owned rows。
- migration 后 repository tests 和关键用例 tests 通过。

## Validation

- 迁移应用和回放验证。
- FK/cascade 查询审计。
- Postgres repository roundtrip tests。
- AgentRun start、submit、accepted turn、fork、delete 的数据库级集成验证。
