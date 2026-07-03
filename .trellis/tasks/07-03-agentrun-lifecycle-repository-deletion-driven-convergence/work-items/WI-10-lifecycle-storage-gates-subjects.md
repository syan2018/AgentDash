# WI-10 Lifecycle Storage Gates Subjects

## Objective

评估并收敛 Lifecycle control-plane 内部的 context、orchestration、tasks、view projection、gates、subjects、agent lineage 等存储形态。

## Decisions

D-002, D-014, D-016, D-017

## Research Inputs

- `research/aggregate-ownership.md`
- `research/database-physical-design.md`

## Scope

- 验证 `LifecycleRun.context` 中 permission/budget/main_agent_run_id/agent_runs/frame_refs 的生产 consumer。
- 评估 `LifecycleRun.view_projection` 是否为可重建 read model。
- 评估 gates 是否需要独立扫描、独立状态机或跨 run 查询。
- 评估 subject association 是否需要 subject -> run/agent 高频反查。
- 评估 agent lineage 是否应为 AgentRun child lineage、Lifecycle child fact 或派生 projection。
- 对每项给出 JSONB、child table、independent table、projection 的结论。

## Out Of Scope

- 不处理 product fork canonical record；交给 WI-08。
- 不处理 projection/frontend product DTO 总清理；交给 WI-09。
- 不做 RepositorySet cleanup；交给 WI-11。

## Dependencies

依赖 WI-00 inventory。schema 变更需登记到 WI-12。

## Implementation Notes

- Lifecycle control-plane 可以保留复杂 orchestration state，但它应解释编排事实，不重复 AgentRun 工作区事实。
- 不可重建且参与决策的字段应被命名为 state/binding。
- 可重建 view projection 应有明确 rebuild input。

## Acceptance

- `LifecycleContext` 中每个字段都有保留或删除理由。
- gates、subjects、lineage 的独立表资格被逐项验证。
- Lifecycle materialization 不再写入会让 node started 误成立的 projection/state。
- 所有 schema 选择都满足 D-017。

## Validation

- `rg "LifecycleContext|permission_scope|budget|LifecycleGate|SubjectAssociation|AgentLineage"` 使用点清点。
- lifecycle repository roundtrip 测试覆盖删除/保留字段。
- migration 验证 JSONB/child table/table 删除或约束调整。
