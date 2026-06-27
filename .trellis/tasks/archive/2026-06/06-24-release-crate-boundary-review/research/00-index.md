# Review Index

## Current Status

Task created on 2026-06-24. Parallel review is being dispatched through direct `multi_agent_v1` research workers.

## Central Finding To Validate

`session` should be treated as RuntimeSession delivery/trace/runtime coordination substrate. AgentRun/Lifecycle should own the business control plane and current runtime surface query/update. RuntimeGateway/API/VFS/Canvas/Permission/WorkspaceModule should consume AgentRun/Lifecycle facades instead of using session as a horizontal service locator.

## Workstream Reports

| Report | Owner | Status |
| --- | --- | --- |
| `01-session-runtime-inventory.md` | Newton / review-session / `019ef55f-a45a-7570-b900-407004c6a4e8` | complete |
| `02-agentrun-lifecycle-surface.md` | Godel / review-agentrun / `019ef55f-b992-7ec3-9fd8-24852a2ec929` | complete |
| `03-api-runtime-gateway-consumers.md` | Locke / review-api-gateway / `019ef55f-d0ca-7003-95ba-a446bf764087` | complete |
| `04-business-surface-update-paths.md` | Dewey / review-business / `019ef55f-f4d1-74f2-886b-8d416f331668` | complete |
| `05-crate-split-coupling-map.md` | Pauli / review-crates / `019ef560-13d3-71b3-a9dd-a17c6214815a` | complete |

## Evidence Seeds

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/runtime-gateway.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/tasks/06-23-session-hub-boundary-cleanup/prd.md`
- `.trellis/tasks/06-23-agentrun-runtime-surface-projection-convergence/prd.md`

## Main-Thread Evidence Notes

- `crates/agentdash-application/src/agent_run/runtime_surface.rs` already defines `AgentRunRuntimeSurfaceQueryPort` and `AgentRunRuntimeSurfaceQuery`, including backend-required `current_runtime_surface_with_backend`.
- `crates/agentdash-application/src/runtime_gateway/mcp_access.rs` already implements `RuntimeSessionMcpAccess` through `CurrentSurfaceRuntimeMcpAccess`, consuming `AgentRunRuntimeSurfaceQueryPort` and `McpToolDiscovery`.
- `crates/agentdash-api/src/session_construction.rs` already exposes API adapters `resolve_current_runtime_surface_for_api`, `resolve_current_runtime_surface_with_backend_for_api` and `resolve_runtime_session_resource_vfs_for_api`.
- `crates/agentdash-application/src/session/mod.rs` still publicly re-exports many runtime/capability/frame-adjacent types, including `AgentFrameRuntimeTarget`, `CapabilityState`, `RuntimeCapabilityTransition`, runtime commands, launch services and runtime services. The review should distinguish necessary RuntimeSession substrate exports from AgentRun/Lifecycle surface leakage.

## First-Round Synthesis

### Positioning

`session` 的真实位置是 RuntimeSession delivery/trace substrate。它保留 RuntimeSession metadata/store/event/projection、live runtime/turn delivery、launch substrate、transition outbox、hook delivery adapter 和 live adoption adapter。它不拥有 AgentRun current surface、business ownership、permission scope、Lifecycle progress、RuntimeGateway backing access 或 API current-surface helper。

### Already Converged

- RuntimeGateway MCP session actions 已经通过 `CurrentSurfaceRuntimeMcpAccess -> AgentRunRuntimeSurfaceQueryPort` 读取 closed current surface。
- API Canvas/Extension/Terminal/VFS 的主要 current-surface consumer 已经开始走 `session_construction.rs` 中的 AgentRun query adapter。
- `AgentRunRuntimeSurfaceQuery`、`AgentRunRuntimeSurfaceUpdateService`、`AgentRunEffectiveCapabilityService`、`DeliveryRuntimeSelectionService`、`AgentRunLifecycleSurfaceProjector` 已经是目标边界的核心雏形。

### Remaining Coupling

- `session/mod.rs` public facade 过宽，并反向 re-export AgentRun/Lifecycle 类型；`AgentFrameRuntimeTarget` 仍定义在 `session::types`。
- `SessionRuntimeInner` 仍持有 AgentFrame/Lifecycle/Permission/Mailbox repositories，作为 live adoption adapter 可以保留，但不应继续成为 surface/helper 聚合对象。
- `session/launch/orchestrator.rs` 和 `session/launch/commit.rs` 仍直接处理 AgentFrame/Lifecycle bootstrap、frame write、delivery binding 与 hook runtime target 同步。
- `AgentRunFrameSurfaceService` 存在但还不是所有 surface update 的唯一 facade；Canvas 仍走专用 `expose_canvas_mount`，Permission adapter 位于 permission 模块且直接使用 `AgentFrameBuilder`。
- API 层仍有 route-local anchor/current frame/read-model 拼装：VFS AgentRun source 选择 latest anchor，sessions/lifecycle views 直接读 current frame resolver。
- `ApiCurrentRuntimeSurface` 丢失 `surface_frame_id`，只保留 `launch_frame_id`，后续作为稳定 facade DTO 前需要修正命名和字段。
- Canvas runtime invoke/bridge 需要显式校验 Canvas Project 与 runtime session current surface Project 一致；Extension route 也应显式拒绝 path Project 与 session Project 不一致。
- 当前 Cargo crate 依赖方向大体干净；真正阻塞 physical split 的是 application 内部 `agent_run <-> session`、`agent_run <-> lifecycle` 等双向 import 和 broad `pub use`。

### Release Split Order

1. Boundary facade first：收束 AgentRun current surface query/update/effective capability、RuntimeSession substrate facade、gateway-facing ports、resource surface facade。
2. Visibility/import cleanup second：压缩 `session/mod.rs`、`agent_run/frame/mod.rs`、`vfs/mod.rs`、application root 的 public exports，消除 API route 对 session planner/current-frame internals 的依赖。
3. Physical crate extraction third：先扩展 `agentdash-application-ports`，再抽 RuntimeSession / RuntimeGateway，最后抽 AgentRun / Lifecycle；VFS physical split 延后到 resource surface 和 provider 依赖收束后。
