# WI-08 Fork Lineage Baseline

## Objective

以 AgentRunForkRecord 收束 product fork，消除 session projection、runtime lineage、receipt result cache 共同构造 baseline 的多源结构。

## Decisions

D-001, D-013, D-015, D-017

## Research Inputs

- `research/fork-lineage-baseline.md`
- `research/aggregate-ownership.md`
- `research/runtime-session-internal-model.md`

## Scope

- 定义 canonical `AgentRunForkRecord`。
- fork baseline 精确锚定 parent AgentRun、fixed turn/message boundary、child AgentRun、child baseline、fork owner。
- product fork transaction 先写 AgentRun fork fact，再创建或绑定 RuntimeSession trace。
- RuntimeSession lineage 降级为 internal trace provenance 或可重建派生。
- fork receipt `result_json` 只保存 idempotent outcome ref，不保存 canonical child refs/lineage。
- 清理 DTO 中同 run control tree、product fork、runtime trace lineage 混用的命名。

## Out Of Scope

- 不实现 admission 基础创建；交给 WI-03。
- 不处理 current delivery selection；交给 WI-06。
- 不处理前端产品 identity 总清理；交给 WI-09。

## Dependencies

依赖 WI-03 admission 边界和 WI-06 delivery binding 语义。

## Implementation Notes

- fork fixed boundary 应能定位到 parent 的稳定 accepted turn/message，而不是最新 runtime projection。
- `agent_run_lineages` 是否继续独立成表由查询和审计需求决定；若保留，它是 AgentRun child lineage table，不是 session lineage 的并列产品事实。
- RuntimeSession fork 可以作为内部 trace 创建方式，但不是产品 fork 的第一事实。

## Acceptance

- fork replay 读取 canonical fork record。
- UI/permission 不依赖 raw Session lineage。
- parent/child baseline 能解释 child initial frame 和 context input。
- RuntimeSession lineage relation kinds 不再表达 product-level companion/spawned/rollback semantics。

## Validation

- fork from fixed turn 的事务测试。
- duplicate fork command replay 测试。
- lineage query 测试覆盖 product lineage 与 runtime trace provenance 的分离。

## Acceptance Record 2026-07-05 / Worker D2

### Implemented

- Duplicate fork replay now resolves the canonical product fork record through `AgentRunLineageRepository::find_parent(child_run_id, child_agent_id)`, using receipt `accepted_refs` only as the idempotent child outcome pointer.
- Fork receipt `result_json` now stores the idempotent outcome shape plus `fork_record_id` and mailbox outcome ref. Parent refs, child refs, redirect refs, and lineage payload are sourced from `agent_run_lineages`.
- `AgentRunLineage` now carries parent frame baseline and child frame baseline fields. Fork materialization writes these fields from the parent frame input and created child frame so product fork audit can explain parent AgentRun, fixed fork point, child AgentRun, child baseline, and fork owner from one canonical record.
- `agent_run_lineages` remains the AgentRun child lineage table because duplicate replay, child lookup, parent audit, and fork provenance need a durable indexed product fact independent of RuntimeSession trace storage.

### RuntimeSession Trace Provenance

RuntimeSession fork creation still supplies internal trace provenance during materialization. Product replay no longer reads RuntimeSession lineage or receipt cached product refs; the remaining convergence risk is the existing runtime-session fork creation order and compensation path, which should be reviewed when the product-first fork transaction is tightened.
