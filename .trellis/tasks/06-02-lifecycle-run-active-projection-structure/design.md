# LifecycleRun Active Projection Structure Design

## ActiveActivityRef

```rust
pub struct ActiveActivityRef {
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: Option<u32>,
    pub status: String,
}
```

Domain 可保存为 `active_activity_refs`，或只在 read builder 中从 `WorkflowGraphInstance.activity_state` 派生。推荐优先 read-builder 派生，减少 run aggregate 双写。

## Projection Rule

```text
WorkflowGraphInstance.activity_state.attempts
  -> Ready / Claiming / Running attempts
  -> ActiveActivityRef[]
  -> LifecycleRunView.active_activity_refs
```

`LifecycleRun.status` 仍可由 graph instance states 聚合。

## Naming

`LifecycleRun.lifecycle_id` 当前表达 root graph backfill 来源。目标命名建议：

- domain: `root_graph_id`
- DTO: `root_graph_id`

如果仍需兼容 task history，迁移直接 rename，不保留运行时双字段。

## Affected Areas

- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-contracts/src/workflow.rs`
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
- `packages/app-web/src/types/lifecycle-views.ts`
- `packages/app-web/src/stores/lifecycleStore.ts`

## Validation

- Unit: two graph instances with same activity key produce two distinct refs。
- Contract: generated TS includes structured refs。
- Frontend: active display uses graph instance id, not split string。
