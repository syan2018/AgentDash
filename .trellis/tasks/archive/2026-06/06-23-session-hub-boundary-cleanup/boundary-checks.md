# Boundary Checks: Session Hub Cleanup

本清单用于最终验收边界，不替代 Rust 测试。命令默认在仓库根目录运行。

## 1. RuntimeGateway MCP Backing Access

RuntimeGateway provider 只做 action input/output 与 actor/context admission；MCP backing access 应消费
AgentRun runtime surface query port。

```powershell
rg -n "AgentFrame|AgentFrameSurfaceExt|resolve_current_frame_from_delivery_trace_ref|SessionRuntimeInner|discover_runtime_mcp_tool_entries" `
  crates/agentdash-application/src/runtime_gateway `
  crates/agentdash-api/src/bootstrap/runtime_gateway.rs

rg -n "SessionCapabilityService|RuntimeSessionMcpAccess" `
  crates/agentdash-api/src/app_state.rs `
  crates/agentdash-api/src/bootstrap/runtime_gateway.rs `
  crates/agentdash-application/src/session/capability_service.rs
```

Expected final state: RuntimeGateway files do not import frame internals or hub discovery. `RuntimeSessionMcpAccess`
is implemented by the resolver-backed access, not by `SessionCapabilityService`.

## 2. API Current Surface Consumers

Canvas snapshot, Extension runtime, VFS `SessionRuntime` / `AgentRun` current source, and Terminal launch target
should consume the same current/resource surface facade.

```powershell
rg -n "AgentFrame|AgentFrameSurfaceExt|resolve_current_frame_from_delivery_trace_ref|get_current_runtime_backend_anchor|SessionFrameVfsResult|resolve_session_frame_vfs" `
  crates/agentdash-api/src/session_construction.rs `
  crates/agentdash-api/src/routes/canvases.rs `
  crates/agentdash-api/src/routes/extension_runtime.rs `
  crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs `
  crates/agentdash-api/src/routes/terminals.rs
```

Expected final state: API current-surface consumers do not expose `AgentFrame` and do not call active-turn-only backend
anchor helpers. A thin API adapter name may remain only if it delegates to AgentRun runtime surface query and returns no
frame entity.

## 3. Session Hub Idle Query

Hub keeps live runtime coordination: active turn state, connector live refresh, hook runtime cache, and launch/turn
supervision. Idle/current AgentRun surface lookup belongs to AgentRun runtime surface query.

```powershell
rg -n "resolve_current_frame_from_delivery_trace_ref|AgentFrameSurfaceExt|typed_vfs|typed_mcp_servers|project_capability_state_from_frame|runtime_mcp_tool_discovery|get_current_runtime_backend_anchor|discover_runtime_mcp_tool_entries" `
  crates/agentdash-application/src/session/hub
```

Expected final state: matches are either active-turn/live refresh internals or tests. No idle MCP discovery path should
assemble VFS/MCP/capability/backend surface inside hub.

## 4. Business Update / Adoption Boundary

Canvas, WorkspaceModule, and Permission surface-changing effects should enter application update use cases; API routes
and business tools should not directly call active runtime adoption primitives.

```powershell
rg -n "adopt_persisted_agent_frame_revision|expose_canvas_mount_revision_and_adopt|resolve_runtime_session_target|get_current_runtime_backend_anchor" `
  crates/agentdash-api/src/routes/permission_grants.rs `
  crates/agentdash-application/src/canvas/tools.rs `
  crates/agentdash-application/src/workspace_module `
  crates/agentdash-application/src/permission/service.rs `
  crates/agentdash-application/src/session/capability_service.rs
```

Expected final state: business paths call typed runtime surface update/adoption use cases. Any active adoption primitive
is private/internal to that use case or live runtime coordination.

## 5. Presentation Read-Model Allowlist

Lifecycle/session presentation views may still read frame data for UI/debug projection. They must stay separate from
runtime action and current-surface consumers.

```powershell
rg -n "AgentFrame|AgentFrameSurfaceExt|AgentFrameRuntimeView|AgentFrameRefDto" `
  crates/agentdash-api/src/routes/lifecycle_views.rs `
  crates/agentdash-api/src/routes/sessions.rs `
  crates/agentdash-contracts/src/runtime/workflow.rs
```

Expected final state: these matches are read-model DTO/projection code only. Do not reuse these paths to back
RuntimeGateway MCP, Canvas/Extension/VFS/Terminal current surface, or business surface mutation.

## 6. Spec Conflict Check

After production workers finish, verify implementation terms still match the spec:

```powershell
rg -n "AgentRunRuntimeSurfaceQuery|AgentRunRuntimeSurfaceQueryPort|CurrentRuntimeSurface|RuntimeSessionMcpAccess|surface_for_actor|runtime_mcp_tool_discovery" `
  .trellis/spec/backend `
  crates/agentdash-application/src `
  crates/agentdash-api/src
```

Expected final state: naming may vary only if the implementation preserves the same ownership boundary: AgentRun /
Lifecycle owns current runtime surface query; RuntimeGateway MCP access consumes that query; session hub owns live
coordination.
