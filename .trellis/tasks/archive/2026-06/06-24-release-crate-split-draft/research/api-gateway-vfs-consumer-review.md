# Research: API Gateway VFS Consumer Review

- Query: 复核 API / RuntimeGateway / VFS / Canvas / Extension / Terminal consumers 对 application crate split 的影响，特别是 current surface assembly 哪些必须从 API/session helper 收口到 AgentRun facade。
- Scope: internal
- Date: 2026-06-25

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-api/src/agent_run_runtime_surface.rs` | 现行 API current-surface adapter；`session_construction.rs` 已不存在，对应能力迁到此文件。 |
| `crates/agentdash-api/src/app_state.rs` | API composition root；装配 AgentRun runtime/resource surface query、RuntimeGateway MCP access 和 Gateway。 |
| `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` | RuntimeGateway provider 装配；只接收 `RuntimeSessionMcpAccess`、setup transport 和 extension transport/repo。 |
| `crates/agentdash-api/src/routes/canvases.rs` | Canvas runtime invoke/snapshot/bridge manifest consumer。 |
| `crates/agentdash-api/src/routes/extension_runtime.rs` | Extension action/channel HTTP host；从 AgentRun current surface + backend anchor 组装 invocation context。 |
| `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs` | VFS browser source resolver；runtime/AgentRun resource surface 已走 AgentRun facade，preview surface 仍 route-locally build VFS。 |
| `crates/agentdash-api/src/routes/terminals.rs` | Terminal launch consumer；读取 backend-required current surface 后派生 terminal target。 |
| `crates/agentdash-api/src/vfs_surface_runtime.rs` | API 实现 VFS summary 的 runtime projection adapter。 |
| `crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs` | RuntimeGateway MCP current-surface 端口/DTO，已从 RuntimeGateway 中抽出。 |
| `crates/agentdash-application/src/agent_run/runtime_surface.rs` | AgentRun current runtime surface、resource surface、terminal target 和 RuntimeGateway MCP surface adapter。 |
| `crates/agentdash-application/src/agent_run/runtime_surface_update.rs` | AgentRun surface update/adoption facade；当前含 Canvas exposure 和 effective capability view helper。 |
| `crates/agentdash-application/src/agent_run/permission_runtime_surface_update.rs` | AgentRun-owned Permission grant surface update adapter。 |
| `crates/agentdash-application/src/runtime_gateway/**` | Gateway registry/provider/MCP/extension action boundary。 |
| `crates/agentdash-application/src/vfs/**` | Generic VFS provider/service/surface summary/mutation/tool infrastructure。 |
| `crates/agentdash-application/src/canvas/**` | Canvas runtime snapshot/resource/tools/surface update adapter。 |
| `crates/agentdash-application/src/workspace_module/**` | WorkspaceModule declaration/projection/runtime tools and Canvas/extension operation paths。 |
| `crates/agentdash-application/src/permission/**` | PermissionGrant lifecycle service，调用 AgentRun-owned surface update adapter。 |
| `crates/agentdash-application/src/capability/**` | Capability resolver/effective state input producer，不是 current surface owner。 |
| `crates/agentdash-application/src/hooks/**` | Hook snapshot/evaluation facade，读取 workflow/frame context，不写 current runtime surface。 |

### Related Specs

- `.trellis/spec/backend/architecture.md`: API 负责鉴权、DTO、错误映射；业务编排进入 application 层。
- `.trellis/spec/backend/runtime-gateway.md`: RuntimeGateway provider 不解析 current `AgentFrame`；Canvas/Extension invocation 必须先校验 Session 与 path Project/current surface Project 一致。
- `.trellis/spec/backend/session/runtime-execution-state.md`: RuntimeGateway/API/VFS/Canvas/Extension/Terminal current-surface consumer 使用 query/update DTO，不 import `AgentFrame` / current-frame resolver / session hub idle branch。
- `.trellis/spec/backend/vfs/architecture.md`: AgentRun resource surface 从 current AgentFrame typed VFS 加 lifecycle projection 闭包生成。
- `.trellis/spec/backend/capability/architecture.md`: AgentRun effective capability/admission 是 runtime 能力读取入口。
- `.trellis/spec/backend/permission/architecture.md`: surface-changing Grant 写 AgentFrame revision，active-runtime adoption 只同步已持久化 revision。
- `.trellis/spec/cross-layer/architecture.md`: HTTP API 仍是 Dashboard 权威入口，跨层 DTO/协议事实不归属于单一端。

### Code Patterns

#### 1. API route/bootstrap 应依赖的 facade/ports 与不能再依赖的 internals

Current positive direction:

- API current-surface helper 只向 route 返回 `ApiCurrentRuntimeSurface { project_id, vfs }` 和 `ApiCurrentRuntimeSurfaceWithBackend { runtime_backend_anchor }`，并在 API 边界做 Project View 权限校验；见 `crates/agentdash-api/src/agent_run_runtime_surface.rs:17`-`31`, `:33`-`:72`。
- Canvas/Extension 已通过 project-checked helper 校验 path Project 与 current surface Project 一致；Canvas invoke 见 `crates/agentdash-api/src/routes/canvases.rs:532`-`540`，Extension action/channel 见 `crates/agentdash-api/src/routes/extension_runtime.rs:133`-`141`, `:194`-`:202`。
- Runtime/AgentRun VFS resource surface 已经从 route-local anchor selection 移到 `AgentRunResourceSurfaceQuery`；API 只调用 `state.services.resource_surface_query` 后做权限映射，见 `crates/agentdash-api/src/agent_run_runtime_surface.rs:122`-`164`。
- RuntimeGateway bootstrap 依赖 `Arc<dyn RuntimeSessionMcpAccess>`，不构造 current frame/current VFS；见 `crates/agentdash-api/src/bootstrap/runtime_gateway.rs:12`-`20`, `:39`-`:46`。
- Gateway-facing current surface 已有 port crate：`RuntimeGatewayMcpSurfaceQueryPort` 定义在 `agentdash-application-ports`，见 `crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs:60`-`67`。

Still needs cleanup for crate split:

- `agent_run_runtime_surface.rs` 仍在 API helper 内每次用 repos 构造 `AgentRunRuntimeSurfaceQuery`，见 `crates/agentdash-api/src/agent_run_runtime_surface.rs:40`-`:43`, `:61`-`:64`, `:167`-`:174`。Draft 应要求把 `Arc<dyn AgentRunRuntimeSurfaceQueryPort>` 放进 `ServiceSet` 或一个 AgentRun facade handle，API helper 只消费该 facade 并做 auth/error mapping。
- `AppState` 已经在 composition root 构造共享 `runtime_surface_query` 给 MCP/resource/presentation，但没有把它作为 service 暴露给 API current-surface helper；见 `crates/agentdash-api/src/app_state.rs:249`-`:273` 和 `:58`-`:118`。
- VFS preview resolver 仍直接调用 `agent_run::resolve_project_workspace` 和 `VfsService::build_vfs(... SessionMountTarget ...)`，见 `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:3`-`:9`, `:42`-`:61`, `:79`-`:94`, `:126`-`:141`。这不是 RuntimeSession current-surface assembly，但物理 VFS split 前应移到 Project/VFS preview facade，避免 API routes 继续知道 session-era mount target construction details。
- Terminal route 仍直接从 `vfs + runtime_backend_anchor` 调 `terminal_launch_target_from_vfs`，见 `crates/agentdash-api/src/routes/terminals.rs:265`-`:276`。Application 已有 `terminal_launch_target_from_current_surface`，见 `crates/agentdash-application/src/agent_run/runtime_surface.rs:456`-`:460`；route 应消费 facade 返回的 terminal launch target。
- API terminal route 还 import `agentdash_application::session::terminal_cache::TerminalState`，见 static grep result `crates/agentdash-api/src/routes/terminals.rs:11`。这属于 RuntimeSession substrate/public DTO split 风险，不是 current surface fact source。

API route/bootstrap target dependencies:

- Depend on `agentdash_application::agent_run` facade/port: `AgentRunRuntimeSurfaceQueryPort`, `AgentRunResourceSurfaceQuery`, `AgentRunRuntimeSurfaceUpdateService` or narrower `AgentRunRuntimeSurfaceUpdatePort`, `AgentRunEffectiveCapabilityService`, `AgentRunTerminalLaunchTarget`.
- Depend on `agentdash_application_ports::runtime_gateway_mcp_surface` from RuntimeGateway crate side; AgentRun implements the port, Gateway consumes only the port.
- Depend on `agentdash_application::runtime_gateway::{RuntimeGateway, RuntimeSessionMcpAccess, RuntimeProvider DTOs}` for action invocation/admission.
- Depend on VFS summary facade and, before extraction, move `VfsSurfaceRuntimeProjection` to `agentdash-application-ports` because API implements it and application consumes it.

API route/bootstrap must not depend on current-surface internals:

- No `AgentFrame`, `AgentFrameSurfaceExt`, `resolve_current_frame_from_delivery_trace_ref`, `SessionHub`, `SessionRuntimeInner`, or route-local `RuntimeSessionExecutionAnchorRepository` selection for current surface paths. Static grep over requested API consumers and `runtime_gateway/**` only found a user-facing error string mentioning `RuntimeSessionExecutionAnchor`; no production import of those current-frame internals was found.
- No `session_construction.rs` helper remains; the draft should rename references to current file `agent_run_runtime_surface.rs` and keep the migration direction explicit.

#### 2. RuntimeGateway crate responsibility boundary and provider/current-surface port relation

- `RuntimeGateway` owns registry/admission/invocation only: provider map/dynamic providers, `surface_for_actor`, `invoke`, and actor/context validation; see `crates/agentdash-application/src/runtime_gateway/gateway.rs:11`-`:27`, `:65`-`:86`, `:137`-`:193`。
- `surface_for_actor` only reports action visibility, not concrete MCP tool surface; see `crates/agentdash-application/src/runtime_gateway/gateway.rs:65`-`:80`。
- `mcp.list_tools` / `mcp.call_tool` providers extract `session_id` from `RuntimeContext::Session` and delegate to `RuntimeSessionMcpAccess`; see `crates/agentdash-application/src/runtime_gateway/session_actions.rs:65`-`:77`, `:115`-`:157`, `:196`-`:227`。
- `CurrentSurfaceRuntimeMcpAccess` consumes `RuntimeGatewayMcpSurfaceQueryPort` plus `McpToolDiscovery`, then builds MCP discovery/call context from the closed surface/backend context; see `crates/agentdash-application/src/runtime_gateway/mcp_access.rs:22`-`:55`, `:91`-`:108`。
- AgentRun implements the gateway-facing port by converting `AgentRunRuntimeSurfaceWithBackend` to the reduced RuntimeGateway DTO; see `crates/agentdash-application/src/agent_run/runtime_surface.rs:707`-`:756`。
- Extension dynamic provider is a RuntimeGateway provider, but its executable authority is Project installation + Backend target + schema/permission validation. It requires Session context with `project_id` and `RuntimeTarget::Backend`; see `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:119`-`:135`, `:233`-`:265`。

Boundary verdict:

- RuntimeGateway should be extractable after its dependency on AgentRun remains only as `RuntimeGatewayMcpSurfaceQueryPort` / extension transport ports. It should not depend on AgentRun implementation, VFS provider internals, session hub, current-frame resolver, or API route state.
- Provider side owns action input/output validation, actor/context admission, schema/permission checks, and relay/local transport request creation. Provider side does not own AgentRun current surface assembly.
- AgentRun side owns current surface closure: `runtime_session_id -> RuntimeSessionExecutionAnchor -> run/agent/current frame -> capability/VFS/MCP/backend/provenance` and exposes reduced DTOs/ports to RuntimeGateway/API.

#### 3. VFS/resource surface crate split timing and correct reference direction

- `AgentRunResourceSurfaceQuery` now owns both runtime-session and run/agent resource surface resolution. For `AgentRun`, latest delivery anchor selection and mismatch checks are inside application facade; see `crates/agentdash-application/src/agent_run/runtime_surface.rs:84`-`:104`, `:106`-`:158`。
- Resource surface projection uses current runtime surface plus `AgentRunLifecycleSurfaceProjector`, preserving base VFS, message stream and optional node evidence; see `crates/agentdash-application/src/agent_run/runtime_surface.rs:160`-`:199`。
- API route uses the facade for `SessionRuntime` and `AgentRun` sources; see `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:208`-`:215`。
- Generic VFS summary has a runtime projection trait in application, and API implements it using `BackendRegistry` and `MountProviderRegistry`; see `crates/agentdash-application/src/vfs/surface_query.rs:10`-`:22`, `crates/agentdash-api/src/vfs_surface_runtime.rs:10`-`:43`。

Split recommendation:

- Do not physically extract VFS provider/service before AgentRun resource surface is stable as the public resource-surface facade. The current correct direction is: API route -> AgentRunResourceSurfaceQuery for runtime/AgentRun sources -> lifecycle projector/generic VFS summary. Generic VFS should not call back into API/session/runtime gateway to discover current AgentRun state.
- Move `VfsSurfaceRuntimeProjection` or an equivalent pure port to `agentdash-application-ports` before extracting any VFS crate, because API implements the port and application consumes it.
- Keep `ResolvedVfsSurfaceSource`/summary DTOs in a stable application/contract boundary, but move Project/Story/Task preview VFS construction behind a VFS preview facade before physical split. The current route-level `resolve_project_workspace + SessionMountTarget + build_vfs` calls are construction details, not HTTP DTO mapping.

#### 4. Canvas / Extension / Terminal / Permission / WorkspaceModule surface update paths

Canvas:

- Canvas runtime invoke now checks current surface project binding before Gateway invoke; see `crates/agentdash-api/src/routes/canvases.rs:532`-`:558`。
- Snapshot path resolves current surface for the canvas project before building runtime snapshot; see `crates/agentdash-api/src/routes/canvases.rs:500`-`:512`, `:773`-`:793`。
- Bridge manifest helper itself only calls `RuntimeGateway::surface_for_actor` and relies on caller to have performed the project guard; see `crates/agentdash-api/src/routes/canvases.rs:682`-`:699`。Move this into a Canvas runtime bridge facade that atomically performs project guard + manifest build.
- Canvas surface-changing tool path still depends on `SharedSessionToolServicesHandle` to reach `runtime_surface_update.expose_canvas_mount`; see `crates/agentdash-application/src/canvas/runtime_surface.rs:10`-`:40`。Target is a narrow `AgentRunRuntimeSurfaceUpdatePort` / `AgentRunFrameSurfaceService::Update(RuntimeSurfaceUpdateRequest)` facade, not a session service locator.

Extension:

- API action/channel path correctly uses backend-required current surface for the path Project, then uses backend access and workspace selection; see `crates/agentdash-api/src/routes/extension_runtime.rs:133`-`:168`, `:194`-`:245`。
- Workspace selection still lives in API route helper `select_extension_invocation_workspace`; see `crates/agentdash-api/src/routes/extension_runtime.rs:323`-`:355`。Move it into an application runtime placement facade shared by Extension API and WorkspaceModule.
- `ExtensionRuntimeChannelInvoker` is constructed per request in the API route; see `crates/agentdash-api/src/routes/extension_runtime.rs:228`-`:245`。For split, prefer AppState-owned invoker/facade or an `ExtensionRuntimeInvocationContextResolver` that returns prevalidated `{ project_id, session_id, backend_id, workspace }` from AgentRun current surface.

Terminal:

- Terminal launch currently checks session permission, resolves backend-required current surface, derives target, then checks backend online; see `crates/agentdash-api/src/routes/terminals.rs:253`-`:288`。
- Application already owns terminal target derivation in AgentRun runtime surface module; see `crates/agentdash-application/src/agent_run/runtime_surface.rs:450`-`:521`。Route should call a facade returning `AgentRunTerminalLaunchTarget` from current surface, not manually pair VFS and backend anchor.

Permission:

- Permission service owns Grant lifecycle and calls AgentRun-owned surface update adapter for apply/revoke/expire; see `crates/agentdash-application/src/permission/service.rs:52`-`:77`, `:128`-`:149`, `:175`-`:198`, `:232`-`:255`, `:280`-`:303`。
- Frame revision writer/adoption lives under `agent_run/permission_runtime_surface_update.rs`, not permission module; see `crates/agentdash-application/src/agent_run/permission_runtime_surface_update.rs:1`-`:5`, `:47`-`:97`, `:99`-`:184`, `:186`-`:207`。This is the target pattern for crate split.

WorkspaceModule:

- WorkspaceModule visibility reads `AgentRunEffectiveCapabilityView`, and effective view currently reaches it through `session_services.runtime_surface_update.effective_capability_view_for_delivery_runtime`; see `crates/agentdash-application/src/workspace_module/tools.rs:86`-`:111`。
- Runtime tool provider gates tool injection with `CapabilityState` and injects WorkspaceModule tools through runtime context; see `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:143`-`:257`。
- `workspace_module_create`, canvas present, and HostCanvas bind_data submit Canvas surface update requests; see `crates/agentdash-application/src/workspace_module/tools.rs:710`-`:720`, `:1234`-`:1277`, `:1437`-`:1447`。
- RuntimeAction and ProtocolChannel branches invoke RuntimeGateway/channel invoker and do not write AgentFrame surface; see `crates/agentdash-application/src/workspace_module/tools.rs:1180`-`:1233`。

Capability and Hooks:

- Capability resolver is a pure input producer; output should be written to AgentFrame revision by AgentRun construction/update path, see `crates/agentdash-application/src/capability/resolver.rs:232`-`:244`, `:269`-`:381`。
- AgentRun effective capability/admission owns final visible capability and grant classification; see `crates/agentdash-application/src/agent_run/effective_capability.rs:25`-`:83`, `:142`-`:151`, `:198`-`:293`。
- Hooks provider builds/evaluates frame/workflow snapshots and appends execution logs; it is a read/evaluate/logging facade, not a runtime surface update path. See `crates/agentdash-application/src/hooks/provider.rs:32`-`:80`, `:263`-`:315` and `crates/agentdash-application/src/hooks/active_workflow_snapshot.rs:56`-`:94`。

#### 5. Draft compile/test gates to add

Compile gates:

- `cargo metadata --no-deps --format-version 1`
- `cargo check -p agentdash-application-ports`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-local -p agentdash-mcp` after public facade/import cleanup.
- After physical crate extraction begins, run `cargo check --workspace` after each crate move.

RuntimeGateway/AgentRun tests:

- `cargo test -p agentdash-application runtime_gateway::session_actions`
- `cargo test -p agentdash-application runtime_gateway::mcp_access`
- `cargo test -p agentdash-application runtime_gateway::extension_actions`
- `cargo test -p agentdash-application agent_run::runtime_surface`
- `cargo test -p agentdash-application agent_run::runtime_surface_update`
- `cargo test -p agentdash-application agent_run::permission_runtime_surface_update`
- `cargo test -p agentdash-application workspace_module::tools`

API route/helper tests:

- `cargo test -p agentdash-api agent_run_runtime_surface`
- `cargo test -p agentdash-api routes::canvases`
- `cargo test -p agentdash-api routes::extension_runtime`
- `cargo test -p agentdash-api routes::terminals`
- `cargo test -p agentdash-api routes::vfs_surfaces`

Static boundary gates:

- `rg -n "AgentFrame|AgentFrameSurfaceExt|resolve_current_frame_from_delivery_trace_ref|SessionHub|SessionRuntimeInner" crates/agentdash-application/src/runtime_gateway crates/agentdash-api/src/agent_run_runtime_surface.rs crates/agentdash-api/src/bootstrap/runtime_gateway.rs crates/agentdash-api/src/routes/canvases.rs crates/agentdash-api/src/routes/extension_runtime.rs crates/agentdash-api/src/routes/vfs_surfaces crates/agentdash-api/src/routes/terminals.rs` should have no production imports.
- `rg -n "AgentRunRuntimeSurfaceQuery::new|AgentRunRuntimeSurfaceQueryDeps|runtime_surface_query\\(" crates/agentdash-api/src` should eventually only match composition root/bootstrap, not route/helper modules.
- `rg -n "agentdash_application::session::construction_planner|agentdash_application::session::.*TerminalState|SessionMountTarget" crates/agentdash-api/src` should trend to zero for production route code after runtime-session/VFS preview facade cleanup.
- `rg -n "McpPresetChanged|ProjectVfsMountChanged|WorkspaceModuleVisibilityChanged|SkillInventoryChanged|AgentProcedureContractChanged" crates/agentdash-application/src` should identify real production submitters before claiming those update paths are release-ready; current production submitters found only Canvas and Permission variants.

### External References

- None. This review used repository code and Trellis specs only.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task. The user supplied the exact task path, so this file was written under `.trellis/tasks/06-24-release-crate-split-draft/research/`.
- `crates/agentdash-api/src/session_construction.rs` requested by the prompt was not found. Current equivalent is `crates/agentdash-api/src/agent_run_runtime_surface.rs`.
- No production code, specs, scripts, or git state were modified; only this research file was created.
- No compile/test commands were run. Gates above are recommended for the draft and based on source inspection.
- Existing code appears to have already addressed two risks from earlier sibling reports: Canvas/Extension path Project checks now call project-checked current-surface helpers, and VFS AgentRun/resource surface now uses `AgentRunResourceSurfaceQuery` instead of route-local latest-anchor selection.
