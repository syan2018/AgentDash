# Release 前模块与 crate 拆分边界调研设计

## Architecture Hypothesis

`session` 当前目录名承载了过多历史语义。目标模型中，产品层面的“Agent 会话/工作台”应由 AgentRun / Lifecycle 表达；`RuntimeSession` 只保存 delivery、trace、turn、event、connector continuation 和 runtime coordination。它可以作为底层运行 substrate 存在，但不应作为 API、Canvas、VFS、Permission、WorkspaceModule、RuntimeGateway MCP provider 的业务事实源。

调研采用以下判定：

- AgentRun/Lifecycle owns control-plane identity：run、agent、frame、orchestration/node、workspace command surface。
- AgentFrame owns effective runtime surface：VFS、MCP、capability、context、workspace module visibility、hook/runtime launch facts。
- RuntimeSession owns delivery/trace substrate：session_id、turn、tool/event stream、connector live cache、active turn coordination、runtime command delivery。
- RuntimeGateway owns invocation protocol：actor/context admission、action input/output、provider dispatch。
- API owns interface adaptation：auth、HTTP DTO、actor/context assembly、error mapping。

## Target Boundary

| Area | Target ownership | Public surface |
| --- | --- | --- |
| Runtime delivery/session events | RuntimeSession substrate under AgentRun/Lifecycle runtime area | launch/steer/turn/event ports consumed by AgentRun services |
| Current runtime surface query | AgentRun/Lifecycle application facade | query DTO returning VFS/MCP/capability/backend anchor/provenance, without exposing `AgentFrame` to API consumers |
| Runtime surface update/adoption | AgentRun frame/surface command boundary | typed update requests from Canvas/Permission/WorkspaceModule/MCP/VFS/Skill |
| Session hub | live runtime coordination | active turn registry/cache, connector session lifecycle, live tool refresh from closed execution context, hook runtime delivery cache |
| RuntimeGateway MCP session action | RuntimeGateway provider + AgentRun surface query port | `mcp.list_tools` / `mcp.call_tool` with session actor/context admission |
| VFS/resource surface | AgentRun resource surface facade + VFS providers | surface refs and runtime mounts derived from the same current AgentRun surface |
| Lifecycle orchestration | Lifecycle/Workflow control plane | node/run/agent/frame/session evidence and reducer-owned state transition |

## Review Record Contract

Each `research/*.md` file must include:

- scope and files inspected
- confirmed facts with file paths
- incoming dependencies and outgoing dependencies
- boundary verdict: keep, move, split, or hide behind facade
- target module/crate candidate
- risks for release split
- follow-up child task candidates

## Module Review Matrix

| Workstream | Primary paths | Main question |
| --- | --- | --- |
| Session runtime inventory | `crates/agentdash-application/src/session/**` | Which files are true RuntimeSession substrate, and which are AgentRun/Lifecycle/capability/VFS/business surface logic living in session? |
| AgentRun/Lifecycle surface | `crates/agentdash-application/src/agent_run/**`, `lifecycle/**`, `workflow/**`, domain workflow entities | Which facade should own current surface query/update and hide `AgentFrame` from consumers? |
| API/RuntimeGateway consumers | `agentdash-api/src/session_construction.rs`, `routes/canvases.rs`, `routes/extension_runtime.rs`, `routes/vfs_surfaces/**`, `routes/terminals.rs`, `bootstrap/runtime_gateway.rs` | Which API paths currently treat session as a business lookup facade? |
| Business update paths | `canvas/**`, `workspace_module/**`, `permission/**`, `capability/**`, `hooks/**`, `vfs/**`, `mcp_preset/**` | Which domain/application modules mutate AgentFrame/runtime surface or active runtime through session-side primitives? |
| Crate split map | `Cargo.toml`, `crates/*/Cargo.toml`, module imports/exports | What crate/module split enforces the target dependency direction with the smallest release-risk sequence? |

## Candidate Split Axes

The review should evaluate these split candidates, then accept/reject them with evidence:

1. `agentdash-application-agentrun` or an internal AgentRun module facade owning command, workspace, surface query/update, effective capability and resource surface.
2. `agentdash-application-lifecycle` owning lifecycle/workflow orchestration, node runtime reducer, subject association and AgentRun materialization use cases.
3. `agentdash-runtime-session` or `agentdash-application-runtime-session` as a lower-level delivery substrate with no public business surface ownership.
4. `agentdash-application-runtime-gateway` as application invocation protocol/provider registry, consuming AgentRun surface ports.
5. Keep VFS providers in application VFS, but consume AgentRun resource surface facade for runtime/current AgentRun surfaces.

## Decision Questions To Resolve

- Whether `RuntimeSession` should physically move under `agent_run` or remain a lower-level crate consumed only through AgentRun/Lifecycle facades.
- Which public `session` exports can be removed, privatized, or renamed to `runtime_session`.
- Which existing `AgentFrame` direct consumers are legitimate construction/read-model internals versus current-surface consumers that require DTO/query ports.
- Which split sequence gives the strongest boundary enforcement before release while keeping reviewable diffs.

## Initial Evidence

- `.trellis/spec/backend/session/architecture.md` states current `Session` target semantics are `RuntimeSession` and not owner of business scope or Agent effective surface.
- `.trellis/spec/backend/runtime-gateway.md` states Session MCP Action must use AgentRun/Lifecycle current runtime surface query and keep `RuntimeSessionMcpAccess` out of `SessionHub`.
- `.trellis/spec/backend/capability/architecture.md` states AgentRun effective capability/admission is the runtime capability reading entry.
- `.trellis/spec/backend/vfs/architecture.md` states AgentRun resource surface derives from current AgentFrame typed VFS and AgentRun surface resolver.
- `crates/agentdash-application/src/session` currently contains 76 files, while `agent_run` already contains `runtime_surface.rs`, `runtime_surface_update.rs`, `delivery_runtime_selection.rs`, `effective_capability.rs`, and frame construction/surface services.

## First-Round Boundary Verdict

### Session Placement

`session` should not be a first-class business module. The stable product/control-plane names are AgentRun and Lifecycle. The session code that remains should be renamed/scoped mentally as RuntimeSession substrate: delivery session creation, turn/event stream, connector live runtime, RuntimeSession persistence/projection, launch substrate after `FrameLaunchEnvelope`, active turn cache, runtime command delivery, hook delivery binding and adoption of already-persisted AgentFrame revisions into live connector state.

This means `RuntimeSession` may become a lower-level crate/module later, but only as a delivery/trace implementation dependency. Public business calls should go through AgentRun/Lifecycle facades; RuntimeGateway/API/Canvas/VFS/Permission/WorkspaceModule should not call session as a horizontal service locator.

### Accepted Target Owners

| Concern | Owner | Evidence |
| --- | --- | --- |
| Current runtime surface query | `agent_run::runtime_surface` | `AgentRunRuntimeSurfaceQueryPort` already returns DTO surface from `runtime_session_id` via anchor/run/agent/current frame. |
| MCP session action backing access | `runtime_gateway::mcp_access` consuming AgentRun query port | `CurrentSurfaceRuntimeMcpAccess` already uses `current_runtime_surface_with_backend`. |
| Surface-changing business updates | AgentRun frame/surface command/update facade | `AgentRunFrameSurfaceService` and `RuntimeSurfaceUpdateRequest` exist; wiring remains incomplete. |
| Effective capability/admission | AgentRun effective capability facade | `AgentRunEffectiveCapabilityService` classifies grant effects and projects final view. |
| Resource/browser surface | AgentRun/Lifecycle resource surface facade | `AgentRunLifecycleSurfaceProjector` exists; API currently assembles it in `session_construction.rs`. |
| Runtime delivery/live adoption | RuntimeSession substrate implementing an AgentRun-owned adopter port | `SessionRuntimeInner` currently implements `AgentRunActiveRuntimeSurfaceAdopter`. |

### Remaining Boundary Problems

- `session/mod.rs` exports too much and leaks AgentRun/Lifecycle concepts through `AgentFrameRuntimeTarget`, `AgentFrameHookRuntime`, `WorkflowApplicationError`, capability types and runtime transition types.
- `AgentFrameRuntimeTarget` belongs to AgentRun surface/adoption semantics, not to `session::types`.
- `SessionRuntimeInner` stores AgentFrame/Lifecycle/Permission/Mailbox repositories directly. Its live adoption behavior is valid, but repository ownership should move behind AgentRun/Lifecycle ports.
- `session/launch/commit.rs` and `session/launch/orchestrator.rs` still perform AgentFrame/Lifecycle writes and bootstrap status decisions. These should be launch/commit adapters owned by AgentRun/Lifecycle, with RuntimeSession keeping accepted turn/trace commits.
- `AgentRunFrameSurfaceService` is not yet the single public update facade. Canvas and Permission have partial adapters, while several `RuntimeSurfaceUpdateRequest` variants are contract-only and not wired from production callers.
- API route helpers still do too much read-model assembly. `session_construction.rs` has the right query direction but should become an AgentRun runtime surface API adapter/facade and preserve both launch and current surface frame ids.
- Canvas and Extension runtime routes need explicit session project binding validation before Gateway/provider execution.

### Crate Split Decision

Physical crate extraction should wait until imports express the target graph. The current Cargo graph is not the main issue; `agentdash-application` internal module imports are. The release-oriented order is:

1. Expand/settle ports and facades while still in one application crate.
2. Tighten public visibility and remove broad `pub use` surfaces.
3. Extract lower-level RuntimeSession and RuntimeGateway crates first.
4. Extract AgentRun and Lifecycle once their interaction is port-mediated.
5. Defer VFS extraction until AgentRun resource surface and VFS provider responsibilities are separated.
