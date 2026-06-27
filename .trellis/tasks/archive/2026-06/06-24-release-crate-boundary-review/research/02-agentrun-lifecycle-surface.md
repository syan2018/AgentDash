# Research: AgentRun/Lifecycle surface

- Query: AgentRun/Lifecycle surface ownership, RuntimeSession dependency direction, and AgentFrame exposure boundary for release crate split.
- Scope: mixed internal architecture review; no external web references used.
- Date: 2026-06-24

## Findings

### Files found

- `.trellis/tasks/06-24-release-crate-boundary-review/prd.md` - parent task requirement: demote `session` to RuntimeSession delivery/trace substrate and move business surface ownership to AgentRun/Lifecycle.
- `.trellis/tasks/06-24-release-crate-boundary-review/design.md` - target boundary table for RuntimeSession, current runtime surface query/update, SessionHub, RuntimeGateway, VFS/resource surface, Lifecycle orchestration.
- `.trellis/tasks/06-24-release-crate-boundary-review/implement.md` - research execution plan and required reviewer outputs.
- `.trellis/tasks/06-24-release-crate-boundary-review/review-briefs/agentrun-lifecycle.md` - focused scope for this report.
- `.trellis/spec/backend/session/architecture.md` - declares RuntimeSession as turn/tool/event/resume/debug/projection/trace lineage substrate and AgentFrame as runtime surface source.
- `.trellis/spec/backend/session/runtime-execution-state.md` - separates SessionHub live coordination from AgentRun current surface query and defines AgentFrame exposure rules.
- `.trellis/spec/backend/workflow/architecture.md` - defines LifecycleRun, LifecycleAgent, AgentFrame, RuntimeSession and RuntimeSessionExecutionAnchor vocabulary.
- `.trellis/spec/backend/runtime-gateway.md` - requires Session MCP tools to use AgentRun/Lifecycle current runtime surface query, not SessionHub or raw AgentFrame.
- `.trellis/spec/backend/capability/architecture.md` - makes AgentRun effective capability/admission the runtime capability reading entry.
- `.trellis/spec/backend/vfs/architecture.md` - makes AgentRun resource surface derive from current AgentFrame typed VFS plus lifecycle projection.
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - defines RuntimeSession -> anchor -> run/agent/subject association query path.
- `crates/agentdash-domain/src/workflow/agent_frame.rs` - `AgentFrame` revision storage entity for capability/context/VFS/MCP/execution profile surface.
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs` - RuntimeSession to run/agent/launch frame/orchestration node backlink.
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs` - run-scoped agent identity plus current delivery binding.
- `crates/agentdash-domain/src/workflow/entity.rs` - `LifecycleRun` aggregate with topology, context, orchestrations, tasks and projection.
- `crates/agentdash-domain/src/workflow/repository.rs` - repository ports for run, agent, frame, subject association and execution anchor.
- `crates/agentdash-application/src/agent_run/runtime_surface.rs` - current runtime surface query port and DTO closure.
- `crates/agentdash-application/src/agent_run/runtime_surface_update.rs` - partial AgentRun surface update service and active runtime adoption port.
- `crates/agentdash-application/src/agent_run/effective_capability.rs` - AgentRun final visible capability/admission view and grant projection.
- `crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs` - current delivery selection by run/agent and anchor validation.
- `crates/agentdash-application/src/agent_run/frame/**` - frame builder, typed surface reader, launch envelope, hook runtime and frame/surface facade.
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs` - creation/materialization of LifecycleRun, LifecycleAgent, launch frame, RuntimeSession and execution anchor.
- `crates/agentdash-application/src/lifecycle/session_association.rs` - anchor-to-current-frame resolver used by internal delivery/runtime paths.
- `crates/agentdash-application/src/lifecycle/session_run_context_resolver.rs` - message stream trace to subject run context resolver.
- `crates/agentdash-application/src/lifecycle/surface/**` - AgentRun lifecycle/resource VFS surface projector and mount helpers.
- `crates/agentdash-api/src/session_construction.rs` - API-side current surface adapter and resource VFS projection.
- `crates/agentdash-application/src/runtime_gateway/mcp_access.rs` - RuntimeGateway MCP access implementation using the AgentRun surface query port.

### Current ownership model in code

`AgentFrame` is already modeled as the storage entity for effective runtime surface revisions: it stores capability, context, VFS, MCP, execution profile, visible canvas mounts and visible workspace module refs (`crates/agentdash-domain/src/workflow/agent_frame.rs:6`, `crates/agentdash-domain/src/workflow/agent_frame.rs:10`, `crates/agentdash-domain/src/workflow/agent_frame.rs:14`, `crates/agentdash-domain/src/workflow/agent_frame.rs:24`, `crates/agentdash-domain/src/workflow/agent_frame.rs:27`). This supports the target interpretation that AgentFrame is an AgentRun/Lifecycle surface revision, not a RuntimeSession entity.

`RuntimeSessionExecutionAnchor` is already a launch-evidence backlink rather than a current surface owner. The domain comment says delivery selection is resolved from the anchor repository so delivery commands do not consult AgentFrame persistence for runtime refs (`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:5`), and the anchor records runtime session id, run id, launch frame id, agent id and optional orchestration node coordinate (`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:25`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29`).

`LifecycleAgent` owns current delivery binding at the control-plane level. `LifecycleAgentCurrentDeliveryBinding` stores runtime session id, launch frame id, optional orchestration node refs, status and observed time (`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:127`), and `bind_current_delivery_from_anchor` copies these facts from the anchor into the agent (`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:221`).

`LifecycleRun` owns orchestration state, not RuntimeSession. It carries topology, context, orchestrations, tasks, view projection and status (`crates/agentdash-domain/src/workflow/entity.rs:153`, `crates/agentdash-domain/src/workflow/entity.rs:159`, `crates/agentdash-domain/src/workflow/entity.rs:164`, `crates/agentdash-domain/src/workflow/entity.rs:166`), and aggregate methods add/replace orchestration instances by `orchestration_id` (`crates/agentdash-domain/src/workflow/entity.rs:221`, `crates/agentdash-domain/src/workflow/entity.rs:232`).

The repository layer matches this split: frame persistence is behind `AgentFrameRepository`, anchors behind `RuntimeSessionExecutionAnchorRepository`, and subject ownership behind `LifecycleSubjectAssociationRepository` (`crates/agentdash-domain/src/workflow/repository.rs:82`, `crates/agentdash-domain/src/workflow/repository.rs:102`, `crates/agentdash-domain/src/workflow/repository.rs:134`).

### Services that are already correct boundaries

`AgentRunRuntimeSurfaceQueryPort` is the strongest current query boundary. Its public methods take `runtime_session_id + purpose` and return `AgentRunRuntimeSurface` or `AgentRunRuntimeSurfaceWithBackend`, not `AgentFrame` (`crates/agentdash-application/src/agent_run/runtime_surface.rs:57`). The implementation resolves `RuntimeSessionExecutionAnchor`, LifecycleRun, LifecycleAgent and current AgentFrame, validates control-plane consistency, reads typed capability/VFS/MCP, derives backend anchor and returns a DTO with run/project/agent ids, runtime address, surface frame id, capability state, VFS, MCP servers, backend anchor and provenance (`crates/agentdash-application/src/agent_run/runtime_surface.rs:81`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:86`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:100`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:115`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:164`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:190`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:221`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:287`).

`CurrentSurfaceRuntimeMcpAccess` is the correct RuntimeGateway consumer shape. It depends on `Arc<dyn AgentRunRuntimeSurfaceQueryPort>` and MCP discovery, asks for `current_runtime_surface_with_backend`, then builds discovery/call context from DTO fields (`crates/agentdash-application/src/runtime_gateway/mcp_access.rs:23`, `crates/agentdash-application/src/runtime_gateway/mcp_access.rs:39`, `crates/agentdash-application/src/runtime_gateway/mcp_access.rs:92`). It does not import `AgentFrame`.

`crates/agentdash-api/src/session_construction.rs` is a mostly correct API adapter boundary. It defines `ApiCurrentRuntimeSurface` and `ApiCurrentRuntimeSurfaceWithBackend` DTOs (`crates/agentdash-api/src/session_construction.rs:24`, `crates/agentdash-api/src/session_construction.rs:41`), calls the AgentRun query port and applies project permission at the API boundary (`crates/agentdash-api/src/session_construction.rs:47`, `crates/agentdash-api/src/session_construction.rs:68`). Canvas snapshot, Extension runtime, Terminal spawn and VFS surface resolver already consume this adapter (`crates/agentdash-api/src/routes/canvases.rs:578`, `crates/agentdash-api/src/routes/extension_runtime.rs:133`, `crates/agentdash-api/src/routes/terminals.rs:259`, `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:280`).

`LifecycleDispatchService` is the correct materialization boundary for control-plane identity. It owns the sequence of resolving/creating run, agent, subject association, RuntimeSession, launch frame, anchor and current delivery binding (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:91`). RuntimeSession creation is through `RuntimeSessionCreator`, a port implemented by session persistence (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:35`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:42`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:61`). Plain dispatch writes a dispatch anchor and binds current delivery (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:597`); graph dispatch writes an orchestration anchor, binds current delivery, then applies `NodeStarted` through the reducer (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:498`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:522`). Scheduler/workflow AgentCall materialization uses the same service to create agent/frame/session/anchor evidence (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:356`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:396`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:411`).

`DeliveryRuntimeSelectionService` is the right AgentRun command-side selector. It receives run/agent, validates `LifecycleAgent.current_delivery`, verifies the execution anchor and resolves current frame through the lifecycle resolver (`crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs:100`, `crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs:144`, `crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs:155`, `crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs:164`). Workspace query and subject execution cancel already use it instead of manually selecting a RuntimeSession (`crates/agentdash-application/src/agent_run/workspace/query.rs:323`, `crates/agentdash-application/src/lifecycle/subject_execution_control.rs:116`).

`AgentRunLifecycleSurfaceProjector` is the right resource-surface boundary. Inputs are an AgentRun runtime address, optional message stream ref, optional orchestration node evidence/projection and skill policy (`crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:20`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:27`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:40`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:133`). It exposes workspace read, launch evidence, companion child and workflow node execution projection methods (`crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:216`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:227`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:238`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:249`). It installs exactly one lifecycle mount and returns a closed VFS/projection set (`crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:336`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:378`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:394`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:404`).

`AgentRunFrameSurfaceService` is the intended write facade. The module explicitly says business domains submit typed construction/update intent and do not own `AgentFrameBuilder`, full `CapabilityState` projection or live-runtime adoption timing (`crates/agentdash-application/src/agent_run/frame/surface_service.rs:1`). `RuntimeSurfaceUpdateRequest` variants carry stable changed-resource identities, not prebuilt frames or capability state (`crates/agentdash-application/src/agent_run/frame/surface_service.rs:85`). The facade routes construction/update commands to typed adapters and enforces write role (`crates/agentdash-application/src/agent_run/frame/surface_service.rs:342`, `crates/agentdash-application/src/agent_run/frame/surface_service.rs:376`, `crates/agentdash-application/src/agent_run/frame/surface_service.rs:385`, `crates/agentdash-application/src/agent_run/frame/surface_service.rs:402`, `crates/agentdash-application/src/agent_run/frame/surface_service.rs:439`). It also has an allowlist excluding Canvas, WorkspaceModule, Permission service and API from direct AgentFrame write ownership (`crates/agentdash-application/src/agent_run/frame/surface_service.rs:251`, `crates/agentdash-application/src/agent_run/frame/surface_service.rs:636`).

`AgentRunEffectiveCapabilityService` is the right conceptual capability/admission boundary. It projects final visible capability, VFS, MCP and visible workspace module refs from frame/state into `AgentRunEffectiveCapabilityView` (`crates/agentdash-application/src/agent_run/effective_capability.rs:142`, `crates/agentdash-application/src/agent_run/effective_capability.rs:196`, `crates/agentdash-application/src/agent_run/effective_capability.rs:210`, `crates/agentdash-application/src/agent_run/effective_capability.rs:237`), and classifies tool-internal grants separately from model-visible AgentFrame surface revisions (`crates/agentdash-application/src/agent_run/effective_capability.rs:25`, `crates/agentdash-application/src/agent_run/effective_capability.rs:57`, `crates/agentdash-application/src/agent_run/effective_capability.rs:65`).

`SubjectRunContextResolver` correctly treats RuntimeSession as a trace backlink. Its documented chain is message stream trace -> RuntimeSessionExecutionAnchor -> LifecycleAgent -> LifecycleRun -> SubjectAssociations -> context (`crates/agentdash-application/src/lifecycle/session_run_context_resolver.rs:37`), and it falls back from agent-scoped associations to run-scoped associations (`crates/agentdash-application/src/lifecycle/session_run_context_resolver.rs:69`).

### Missing facade / port work

The query facade exists but is not centralized as an application service in API/AppState. `session_construction.rs` constructs `AgentRunRuntimeSurfaceQuery` from repositories on demand (`crates/agentdash-api/src/session_construction.rs:146`), while API also directly instantiates `AgentRunLifecycleSurfaceProjector` with `RepositorySet` for resource VFS projection (`crates/agentdash-api/src/session_construction.rs:104`, `crates/agentdash-api/src/session_construction.rs:123`). Target: expose an application-owned `AgentRunCurrentRuntimeSurfacePort` and `AgentRunResourceSurfacePort` from AppState/bootstrap so API routes consume DTOs only and do not assemble repository-backed query/projector internals.

The resource-surface DTO should carry both launch evidence frame id and current surface frame id. `AgentRunRuntimeSurface` has `surface_frame_id` and provenance `launch_frame_id` (`crates/agentdash-application/src/agent_run/runtime_surface.rs:231`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:312`), but `ApiCurrentRuntimeSurface` stores only `launch_frame_id` and drops `surface_frame_id` (`crates/agentdash-api/src/session_construction.rs:29`, `crates/agentdash-api/src/session_construction.rs:201`). The API resource projector then passes `launch_frame_id` as `AgentRunRuntimeAddress.frame_id` (`crates/agentdash-api/src/session_construction.rs:123`). Split work should rename/carry both fields so current surface consumers cannot confuse launch evidence with current surface revision.

`AgentRunFrameSurfaceService` should become the single public update facade, but current implementation is partial. Canvas goes through `AgentRunRuntimeSurfaceUpdateService::expose_canvas_mount` directly (`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:71`) and Canvas/WorkspaceModule wrappers validate or submit `RuntimeSurfaceUpdateRequest` while calling that narrower method (`crates/agentdash-application/src/canvas/runtime_surface.rs:10`, `crates/agentdash-application/src/workspace_module/tools.rs:669`). Permission does use `AgentRunFrameSurfaceService`, but the adapter implementation lives under `permission/` and directly imports `AgentFrameBuilder`, `AgentFrameRepository`, `AgentFrame` and `AgentFrameRuntimeTarget` (`crates/agentdash-application/src/permission/runtime_surface_update.rs:15`, `crates/agentdash-application/src/permission/runtime_surface_update.rs:21`, `crates/agentdash-application/src/permission/runtime_surface_update.rs:23`). Target: keep Permission/Canvas/WorkspaceModule as command producers only; move frame-writing adapters under AgentRun runtime surface update or behind an AgentRun-owned update port.

`AgentFrameRuntimeTarget` is currently defined in `session::types` even though AgentRun surface/admission services use it as a frame/control target (`crates/agentdash-application/src/session/types.rs:62`, `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:19`, `crates/agentdash-application/src/agent_run/effective_capability.rs:12`). Target: move this type to AgentRun frame/surface port types, and let RuntimeSession live coordination implement an adoption port that receives it.

Raw frame primitives are too public for a crate split. `agent_run/mod.rs` re-exports `AgentFrameBuilder`, `AgentFrameSurfaceExt`, frame write boundary internals and `RuntimeSurfaceUpdateRequest` from the module root (`crates/agentdash-application/src/agent_run/mod.rs:41`); `agent_run/frame/mod.rs` exposes `pub mod builder`, `pub mod construction`, `pub mod runtime_launch`, `pub mod surface`, and re-exports `AgentFrameBuilder` (`crates/agentdash-application/src/agent_run/frame/mod.rs:1`, `crates/agentdash-application/src/agent_run/frame/mod.rs:9`). Target: make builder/surface ext visible only to frame construction, launch commit, query/update internals, repository adapters and presentation read models.

`lifecycle/surface/mount` should be implementation detail. Its file-level comment says lifecycle mount projection keeps lifecycle domain identity out of generic VFS (`crates/agentdash-application/src/lifecycle/surface/mount.rs:1`), but `lifecycle/surface/mod.rs` exposes `pub mod mount` (`crates/agentdash-application/src/lifecycle/surface/mod.rs:1`). Target: public surface should be `AgentRunLifecycleSurfaceProjector` or a trait port; mount helpers stay private/pub(crate).

`AgentRunEffectiveCapabilityService` needs a repository/query-backed port for consumers. The static service has a runtime-session helper that reaches into anchor and permission grant repositories (`crates/agentdash-application/src/agent_run/effective_capability.rs:276`), and SessionHub live tool refresh calls it directly (`crates/agentdash-application/src/session/hub/tool_builder.rs:285`, `crates/agentdash-application/src/session/hub/tool_builder.rs:297`). Target: expose `AgentRunEffectiveCapabilityPort` that consumes AgentRun runtime address or current surface DTO, and keep SessionHub using it only for active-turn live coordination.

### AgentFrame exposure rule

Allowed direct `AgentFrame` holders:

- Domain entity/repository and persistence adapter boundaries: `AgentFrame` and `AgentFrameRepository` define the aggregate storage port (`crates/agentdash-domain/src/workflow/agent_frame.rs:10`, `crates/agentdash-domain/src/workflow/repository.rs:82`).
- Frame construction and launch closure: `AgentFrameBuilder`, `FrameSurfaceDraft`, `FrameLaunchSurface` and `FrameLaunchEnvelope` construct launch-ready typed surface (`crates/agentdash-application/src/agent_run/frame/builder.rs:83`, `crates/agentdash-application/src/agent_run/frame/builder.rs:136`, `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs:87`, `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs:158`, `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs:231`).
- Lifecycle dispatch launch evidence: dispatch creates launch-anchor frames and anchors delivery runtime to run/agent/node (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:773`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:789`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:597`).
- Session launch commit: connector accepted boundary persists accepted/pending AgentFrame revision and marks current delivery running (`crates/agentdash-application/src/session/launch/commit.rs:151`, `crates/agentdash-application/src/session/launch/commit.rs:164`, `crates/agentdash-application/src/session/launch/commit.rs:224`, `crates/agentdash-application/src/session/launch/commit.rs:242`).
- AgentRun current surface query and update internals: query reads current frame and emits DTO (`crates/agentdash-application/src/agent_run/runtime_surface.rs:164`); update writes/adopts a new frame as an AgentRun-owned operation (`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:109`, `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:123`).
- SessionHub live adoption implementation: it may read frame/anchor/current frame to validate an already-persisted revision, then update active turn cache, connector tools and hook runtime (`crates/agentdash-application/src/session/hub/tool_builder.rs:74`, `crates/agentdash-application/src/session/hub/tool_builder.rs:99`, `crates/agentdash-application/src/session/hub/tool_builder.rs:137`, `crates/agentdash-application/src/session/hub/tool_builder.rs:191`). This is live coordination, not a current-surface query facade.
- Presentation/debug read models: `lifecycle_views` resolves current frame and maps raw JSON fields into `AgentFrameRuntimeView` (`crates/agentdash-api/src/routes/lifecycle_views.rs:275`, `crates/agentdash-api/src/routes/lifecycle_views.rs:336`), and AgentRun workspace query reads a frame to produce workspace/resource read models (`crates/agentdash-application/src/agent_run/workspace/query.rs:338`). These projections must not be reused as RuntimeGateway/API current-surface ports.

Consumers that should use query DTO/update commands instead of direct `AgentFrame`:

- RuntimeGateway MCP access: already uses `AgentRunRuntimeSurfaceQueryPort` (`crates/agentdash-application/src/runtime_gateway/mcp_access.rs:23`).
- API current-surface consumers: Canvas runtime snapshot, Extension runtime, Terminal launch and VFS surface resolver already use `session_construction` DTOs (`crates/agentdash-api/src/routes/canvases.rs:588`, `crates/agentdash-api/src/routes/extension_runtime.rs:133`, `crates/agentdash-api/src/routes/terminals.rs:271`, `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:280`).
- Canvas/WorkspaceModule/Permission surface-changing paths: they should submit typed `RuntimeSurfaceUpdateRequest` or facade-specific command; they should not write/adopt frames directly. Current tests encode this guard for several business/API paths (`crates/agentdash-application/src/agent_run/frame/surface_service.rs:679`, `crates/agentdash-application/src/agent_run/frame/surface_service.rs:695`).
- WorkspaceModule capability visibility: the tool uses `effective_capability_view_for_delivery_runtime` rather than reading frames itself (`crates/agentdash-application/src/workspace_module/tools.rs:71`, `crates/agentdash-application/src/workspace_module/tools.rs:91`).

### RuntimeSession dependency direction

Recommended direction:

```text
API / RuntimeGateway / business tools
  -> AgentRun/Lifecycle application facades (query DTOs + update commands)
  -> domain workflow ports/entities (LifecycleRun, LifecycleAgent, AgentFrame, RuntimeSessionExecutionAnchor)
  -> RuntimeSession delivery/trace ports for create/launch/steer/live adoption
```

RuntimeSession should remain a lower-level delivery/trace substrate. Lifecycle may create a RuntimeSession through `RuntimeSessionCreator`, but it must immediately bind that delivery evidence to LifecycleAgent/AgentFrame through `RuntimeSessionExecutionAnchor` and `LifecycleAgent.current_delivery` (`crates/agentdash-application/src/lifecycle/dispatch_service.rs:805`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:597`, `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:221`). AgentRun/Lifecycle should own all current surface and command decisions.

RuntimeSession must not answer "what is the current VFS/MCP/capability/backend surface?" The current implementation already has the correct path: `runtime_session_id -> RuntimeSessionExecutionAnchor -> LifecycleRun/LifecycleAgent -> current AgentFrame -> query DTO` (`crates/agentdash-application/src/agent_run/runtime_surface.rs:81`, `crates/agentdash-application/src/lifecycle/session_association.rs:34`). Keep SessionHub live helpers limited to active-turn caches and adoption of already-persisted revisions.

For a physical crate split, put RuntimeSession storage/event/turn/live coordination ports below AgentRun/Lifecycle. Higher-level AgentRun/Lifecycle declares or consumes ports such as `RuntimeSessionCreator`, `SessionTurnMessageDeliveryPort` and `AgentRunActiveRuntimeSurfaceAdopter`; RuntimeSession implements them at composition root. Avoid letting a runtime-session crate import AgentRun query/update services except through narrow traits needed for live adoption callbacks.

### Related specs

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `.trellis/spec/backend/runtime-gateway.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/repository-pattern.md`

### External references

- None. This review only used repository code, Trellis task artifacts and local specs.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task; this report used the explicit task path supplied in the user prompt.
- I did not audit every file under `agent_run/**`, `lifecycle/**` and `workflow/**`; the review focused on requested surface/query/update/dispatch files plus direct consumers and domain workflow entities/repositories.
- `AgentRunFrameSurfaceService` is present but not yet the single wired update facade. Permission's adapter still lives under `permission/` and writes frames directly behind the facade; Canvas uses a specialized `expose_canvas_mount` method rather than the generic update command.
- API `ApiCurrentRuntimeSurface` currently drops `surface_frame_id` and carries only `launch_frame_id`; split work should clarify DTO naming before using that DTO as the stable cross-crate contract.
- Several internal modules still publicly re-export frame primitives (`AgentFrameBuilder`, `AgentFrameSurfaceExt`, `surface::mount`). These are visibility problems for a future crate split, not proof that the conceptual boundary is wrong.
