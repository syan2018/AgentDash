# AgentRun / RuntimeSession Hard Cutover Design

## Target Model

目标模型保持：

```text
LifecycleRun
  -> LifecycleAgent
  -> AgentFrame
  -> RuntimeSession
```

含义：

- `LifecycleRun` 是 tracked life process / control ledger。
- `LifecycleAgent` 是 run-scoped Agent runtime identity。
- `AgentFrame` 是 versioned runtime surface revision，拥有 capability、context、VFS、MCP 与 execution profile。
- `RuntimeSession` 是 connector delivery、trace evidence、event stream、continuation handle 和 runtime action context。
- `RuntimeSessionExecutionAnchor` 是 RuntimeSession 反查 run / agent / frame / orchestration node 的索引，不承载 surface truth。

## Boundary Changes

### Frame Launch Surface

`FrameLaunchEnvelope` 不再保存与 typed surface 并列的 `executor_config`、`capability_state`、`vfs`、`mcp_servers` 字段。

Frame construction 输出 launch-ready typed surface：

```text
FrameConstructionService
  -> FrameLaunchEnvelope {
       surface: FrameRuntimeSurface,
       surface_draft: FrameSurfaceDraft or FrameLaunchSurface,
       intent,
       working_directory,
       context_bundle,
       continuation_context_frame,
       base_capability_state,
       resolution_trace
     }
```

`surface_draft` 进入 launch 前必须完整：

- `capability_state: CapabilityState`
- `vfs: Vfs`
- `mcp_servers: Vec<RuntimeMcpServerDeclaration>`
- `execution_profile: AgentConfig`
- optional context bundle summary

如果当前 `FrameSurfaceDraft` 的 optional 字段让代码保留 fallback，重构时应引入 launch-ready wrapper 或 validation helper，使 planner 只消费 non-optional 结果。

### Construction Projection Cleanup

`RuntimeContextInspectionPlan.projections` 只保留仍属于 inspection/read model 的字段，不再保留 MCP/capability fixture。

测试需要 surface 时直接构造：

- `FrameSurfaceDraft` / launch-ready surface
- persisted `AgentFrame`
- `FrameLaunchEnvelope` factory that requires complete typed surface

旧 fixture helper 不作为生产结构保留。

### Owner Bootstrap Composition

Owner bootstrap composition 的业务职责属于 frame construction / AgentRun surface composition，不属于 RuntimeSession 模块。

迁移目标：

```text
workflow/frame_construction
  -> owner / activity / companion composer
  -> FrameSurfaceDraft
  -> AgentFrameBuilder::with_surface_draft
  -> build_envelope_from_frame
```

`session` 模块保留职责：

- launch stages
- turn supervisor/runtime registry
- eventing and persistence adapters
- runtime command delivery outbox
- runtime action adapter and connector projection

为了最高效推进，本轮允许先在 `workflow/frame_construction` 下建立新 composer 模块并移动 owner-facing types/functions；随后删除或缩小 `session::assembler` 对 owner bootstrap 的拥有关系。

### RuntimeSession Boundaries

继续保留：

- `/sessions/{id}/trace`
- `/sessions/{id}/runtime-control`
- runtime gateway Session actions such as MCP list/call
- pending queue keyed by delivery runtime session
- event/terminal/lineage/compaction stores

原因是这些边界表达 delivery 和 trace，而不是 business control-plane ownership。

## Data Flow

### New User Message

```text
AgentRun route
  -> resolve run/agent/project permission
  -> latest delivery RuntimeSession via anchor
  -> AgentRunMessageService
  -> SessionLaunchService.launch_command(delivery_runtime_session_id, LaunchCommand)
  -> FrameConstructionService.construct_launch_envelope
  -> current AgentFrame + launch-ready surface
  -> LaunchPlanner / TurnPreparer
  -> connector ExecutionContext
```

### MCP Tool Discovery

```text
active turn:
  ExecutionSessionFrame.mcp_servers + active CapabilityState

idle runtime action:
  RuntimeSessionExecutionAnchor
    -> current AgentFrame
    -> typed MCP surface + projected CapabilityState
```

No path reads MCP preset or session projection as current executable truth after frame construction.

### Runtime Context Transition

```text
runtime command request
  -> AgentFrameTransitionRecord
  -> RuntimeDeliveryCommand(outbox)
  -> replay during next launch
  -> new AgentFrame revision / launch surface
  -> active execution snapshot
```

`CapabilityState.tool.mcp_servers` remains a capability/draft projection and is derived from frame/launch surface.

## Testing Strategy

Tests that previously used partial `RuntimeContextInspectionPlan.projections` should be deleted if they only verify compatibility plumbing. Tests that protect behavior should construct complete surface inputs.

Priority tests to preserve or rewrite:

- AgentFrame surface write/read roundtrip.
- Launch envelope requires complete surface.
- Launch planner projects ExecutionContext from typed surface.
- Runtime gateway MCP discovery reads active snapshot or current AgentFrame.
- AgentRun message/steer accepted refs still include run/agent/frame/runtime/turn refs.
- Runtime command replay updates frame/launch surface without storing full current surface in session persistence.

## Migration Notes

No database migration is expected unless implementation discovers persisted current surface fields that must be removed. If no schema changes occur, run `pnpm run migration:guard` and record no migration needed.

Generated TypeScript contracts should only change if DTO names or fields change. The intended backend hard cutover should not require public AgentRun workspace DTO changes.

## Rollback Points

- Phase 1 commit after launch surface single-source compile/test pass.
- Phase 2 commit after projection fixture removal and test rewrite/delete.
- Phase 3 commit after composer relocation/session module boundary cleanup.
- Phase 4 commit after global review/spec/check.

Each phase should keep the worktree clean before dispatching the next dependent phase.
