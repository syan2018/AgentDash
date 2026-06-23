# AgentRun / RuntimeSession 边界解耦设计

## Target Architecture

```text
API / RuntimeGateway / Canvas / WorkspaceModule / Permission / VFS / Terminal / Extension
  -> AgentRun / Lifecycle application facades
       - current runtime surface query
       - resource surface query
       - effective capability / admission
       - runtime surface update command
       - presentation/read-model query
  -> RuntimeSession delivery ports
       - create delivery session
       - launch/steer/cancel turns
       - stream events and trace
       - adopt persisted surface revision into live connector state
```

`RuntimeSession` 只负责 delivery/trace/runtime coordination。它可以实现 AgentRun 需要的 live adapter，但不再作为 current surface、business ownership、permission scope、Lifecycle progress 或 API helper 的事实源。

## Module Ownership

| Concern | Owner | Notes |
| --- | --- | --- |
| `runtime_session_id -> current closed surface` | AgentRun runtime surface query | Starts from `RuntimeSessionExecutionAnchor`, returns DTO without exposing `AgentFrame`. |
| Resource browser / lifecycle mount VFS | AgentRun/Lifecycle resource surface facade | Uses current surface + `AgentRunLifecycleSurfaceProjector`; API does not assemble projector. |
| Surface-changing business updates | AgentRun frame/surface update facade | Business modules submit typed requests only. |
| Permission grant admission | AgentRun effective capability/admission | Tool-internal grants stay admission-only; toolset expansion writes frame revision. |
| RuntimeGateway MCP session actions | RuntimeGateway provider + AgentRun surface port | Provider owns action input/output and admission, not current frame lookup. |
| Launch accepted turn delivery | RuntimeSession launch substrate | Consumes `FrameLaunchEnvelope`; does not own frame construction facts. |
| AgentFrame write / Lifecycle binding | AgentRun/Lifecycle launch/commit adapters | Writes durable control-plane facts after connector acceptance. |
| Live tool/cache/hook refresh | RuntimeSession live adapter | Only adopts already-persisted AgentFrame revision. |

## Implementation Phases

### Phase A: Facade Contracts

- Introduce or stabilize narrow AgentRun ports:
  - `AgentRunCurrentRuntimeSurfaceQuery`
  - `AgentRunResourceSurfaceQuery`
  - `AgentRunRuntimeSurfaceUpdateCommand`
  - `AgentRunEffectiveCapabilityQuery`
  - `RuntimeSessionLiveSurfaceAdopter`
- Move `AgentFrameRuntimeTarget` ownership to AgentRun.
- Preserve compatibility only inside this branch while refactoring; project is pre-release, so old public routes should be deleted or privatized rather than kept as fallback.

### Phase B: Session Facade Tightening

- Reduce `session/mod.rs` exports to RuntimeSession substrate services.
- Make session hub/tool/launch internals private or crate-private.
- Remove re-exports of AgentRun/Lifecycle types from `session`.

### Phase C: Launch / Commit Boundary

- Split RuntimeSession accepted turn commit from AgentRun control-plane commit.
- Move AgentFrame revision write and LifecycleAgent delivery binding into AgentRun/Lifecycle adapter.
- Ensure RuntimeSession launch only consumes final `FrameLaunchEnvelope` and emits delivery facts.

### Phase D: Consumer Migration

- RuntimeGateway MCP access stays on AgentRun query port.
- Canvas, Extension, Terminal, VFS and API read-model consumers use AgentRun current/resource surface facade.
- Permission/Canvas/WorkspaceModule surface updates submit typed requests to AgentRun update facade.
- API routes keep auth/DTO/error mapping only.

### Phase E: Visibility / Import Cleanup

- Tighten `pub mod` / `pub use`.
- Run `rg` checks for forbidden imports.
- Only after this task should physical crate extraction begin.

## Key Risks

- `AgentRun <-> session` and `AgentRun <-> lifecycle` imports are currently bidirectional; moving files before ports exist will create cycles.
- SessionHub still implements live adoption. Keep the behavior, but hide it behind AgentRun-owned port.
- API helpers currently have correct direction but wrong naming and too much projector logic. Rename and move after facade exists.
- `ApiCurrentRuntimeSurface` must preserve current surface frame id separately from launch frame id.

## Validation Strategy

- Compile after each phase.
- Add regression tests around Project/session binding, current surface query with backend, resource surface projection, and surface update/adoption.
- Use `rg` checks to enforce dependency cleanup before any crate split task starts.
