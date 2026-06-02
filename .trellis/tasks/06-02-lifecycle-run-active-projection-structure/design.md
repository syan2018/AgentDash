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

## Public Exposure Convergence

本任务处理 active projection 时同步收敛公开类型暴露：业务运行态字段落在 Agent / Lifecycle 锚定的 generated contracts 上，Session 保持 runtime trace / turn / transport adapter 的语义。

目标入口关系：

```text
runtime_session_id
  -> session-indexed adapter endpoint
  -> AgentFrameRuntimeView / RuntimeSessionExecutionAnchor / ActivityAttemptRef
  -> LifecycleRunView / WorkflowGraphInstanceView / ActivityAttemptView
```

`runtime_session_id` 可以作为查询参数或路径参数帮助定位 runtime trace，但返回体应立即回到 Agent / Lifecycle read model。这样前端无需维护 session-first runtime view，也无需从 session resource 再推导 frame、agent、activity。

`ActiveActivityRef` 若进入公开 contract，应复用 Activity attempt identity：

```rust
pub struct ActiveActivityRef {
    pub run_id: Uuid,
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: Option<u32>,
    pub status: String,
}
```

这个类型属于 Lifecycle read model，而不是 Session read model。Session trace DTO 可以引用它或引用 `ActivityAttemptRefDto`，但不应成为 active workflow 的所有者。

## Naming

`LifecycleRun.lifecycle_id` 当前表达 root graph backfill 来源。目标命名建议：

- domain: `root_graph_id`
- DTO: `root_graph_id`

如果仍需兼容 task history，迁移直接 rename，不保留运行时双字段。

## Affected Areas

- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-contracts/src/workflow.rs`
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs`
- `crates/agentdash-api/src/routes/lifecycle_views.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
- `packages/app-web/src/types/lifecycle-views.ts`
- `packages/app-web/src/stores/lifecycleStore.ts`
- `packages/app-web/src/services/lifecycle.ts`
- `packages/app-web/src/types/session.ts`

## Validation

- Unit: two graph instances with same activity key produce two distinct refs。
- Contract: generated TS includes structured refs。
- Frontend: active display uses graph instance id, not split string。
- Exposure: session-indexed runtime query returns Agent / Lifecycle anchored generated contract types。
