# Canvas runtime surface 与资源绑定收束 Design

## Architecture

复用并补齐现有应用层 Canvas runtime binding/context，而不是新增第二套绑定事实源。当前已有：

- `CanvasAgentRunContext`：`crates/agentdash-application/src/canvas/diagnostics.rs:20`，持有 AgentRun、Agent、Canvas、delivery trace、runtime session、current AgentFrame、`agent_run_canvas_ref`。
- `CanvasRuntimeStateRepository`：`crates/agentdash-domain/src/canvas/runtime_state.rs:113`，保存 AgentRun Canvas runtime observation 和 interaction snapshot 的 latest state。
- `agent_run_canvas_runtime_observations` / `agent_run_canvas_interaction_snapshots`：`crates/agentdash-infrastructure/migrations/0026_agent_run_canvas_runtime_state.sql:4`，以 `(run_id, agent_id, canvas_mount_id)` 为稳定 ownership key。

本任务要做的是把 snapshot/resource/action/backend 消费统一接到这层现有绑定上，并把 Workspace Module 内部 operate/invoke/present/canvas runtime update 共用的 runtime facts 收到一个模块接口里。Canvas context 表达“哪个 AgentRun Canvas”，Workspace Module runtime context 表达“当前工具/模块调用在哪个 runtime surface 上执行”。

现有绑定层的闭包应表达：

- AgentRun Canvas identity：`run_id`、`agent_id`、`canvas_mount_id`、`agent_run_canvas_ref`
- AgentRun delivery identity：`runtime_session_id`、`delivery_trace_ref`
- Canvas identity：`canvas_id`、`canvas_mount_id`、`project_id`
- current runtime surface：`current_surface_frame_id`、`surface_revision`、`vfs`、`vfs_access_policy`
- runtime resource surface：`resource_surface_ref`
- action surface：RuntimeGateway `surface_for_actor(UserCanvas, Session)`
- backend execution：`RuntimeBackendAnchor` 或清晰的 unavailable diagnostic

持久事实归属保持不变：

- `RuntimeSessionExecutionAnchor`：runtime session 到 AgentRun 的 backlink。
- `LifecycleAgent` / current delivery binding：AgentRun 当前 delivery runtime。
- `AgentFrame` current revision：current VFS、MCP、capability 和 visible Canvas/workspace module surface。
- Canvas repository：Canvas source、scope、access、mount id。
- Canvas runtime state tables：Canvas runtime observation / interaction latest state，按 `(run_id, agent_id, canvas_mount_id)` 归属。

## Module Model

目标是形成两个小接口、深实现：

### AgentRun Canvas runtime context

外部 seam 位于 `crates/agentdash-application/src/canvas/diagnostics.rs` 或紧邻的应用层 Canvas runtime context 模块。该接口从 `run_id + agent_id + canvas_mount_id` 解析 AgentRun Canvas ownership，并输出：

- Canvas ownership：`run_id`、`agent_id`、`canvas_id`、`canvas_mount_id`、`agent_run_canvas_ref`
- Delivery binding：`runtime_session_id`、`delivery_trace_ref`
- Current frame binding：`current_agent_frame`、visible Canvas 校验结果
- Runtime surface projection：resource surface ref、active VFS、VFS access policy、runtime action catalog、backend anchor
- State ownership helpers：读取/写入 observation、interaction snapshot 使用同一 `(run_id, agent_id, canvas_mount_id)`

这个接口的价值是让 API route 只表达请求意图：读 snapshot、upsert binding、invoke runtime、submit agent input、record observation。Project、Canvas、current frame、resource surface、backend placement 的组合规则集中在 context 解析和投影里。

### Workspace Module runtime context

外部 seam 位于 `crates/agentdash-workspace-module/src/workspace_module/` 内部，名称可在实现时确定，例如 `WorkspaceModuleRuntimeContext` / `WorkspaceModuleRuntimeContextResolver`。该接口由 `ExecutionContext` 或 AgentRun Canvas context 构造，统一持有：

- Runtime identity：delivery runtime session id、turn id、agent id、Project auth identity
- Runtime surface：`SharedRuntimeVfs`、VFS access policy、resource surface ref projection
- Backend/action：`ResolvedInvocationBackend`、backend readiness、channel readiness、RuntimeGateway actor/context、runtime action catalog
- AgentRun bridge：`SharedWorkspaceModuleAgentRunBridgeHandle` 与 current AgentFrame runtime surface update adapter
- Canvas module state：Canvas mount visibility request、Canvas binding changed request、Canvas runtime observation/interaction latest state ownership

`WorkspaceModuleOperateCommand`、`WorkspaceModuleInvokeCommand`、`WorkspaceModulePresentCommand` 应逐步从“携带一串 runtime/backend/VFS 参数”变成“携带 operation intent + runtime context”。这样 Canvas runtime update、Extension channel invoke、Canvas inspect、Canvas present 都通过同一份 runtime context 获得 session/backend/VFS/bridge。

### Interface Ownership

- `CanvasAgentRunContext` owns Canvas-specific identity and visible-frame validation.
- `WorkspaceModuleRuntimeContext` owns tool/module execution runtime facts.
- `AgentRunRuntimeSurfaceQuery` owns current runtime surface projection from delivery runtime session.
- `VfsSurfaceResolver` owns `surface_ref` parsing and VFS bundle resolution.
- RuntimeGateway owns action/channel/MCP invocation admission.

## Data Flow

### AgentRun Canvas snapshot

```text
route params run_id + agent_id + canvas_mount_id
  -> resolve_agent_run_canvas_context(...)
     -> validates run/agent/project/canvas/current frame visibility
  -> enrich existing CanvasAgentRunContext through WorkspaceModuleRuntimeContext / AgentRunRuntimeSurfaceQuery
     -> read current runtime surface by context.runtime_session_id
     -> derive resource_surface_ref
     -> derive runtime action surface
     -> derive backend anchor when needed
  -> build CanvasRuntimeSnapshot from enriched context
  -> return contract DTO
```

`build_runtime_snapshot_with_bindings` 不再通过 `session_id` 隐式决定资源面。它应接收显式 `resource_surface_ref` 或上层已闭包的 runtime context。

### Canvas runtime binding update

```text
PUT runtime-bindings/{alias}
  -> resolve AgentRunCanvasRuntimeContext
  -> resolve WorkspaceModuleRuntimeContext for the same delivery runtime
  -> apply RuntimeSurfaceUpdateRequest::CanvasBindingChanged through workspace module runtime context
  -> refresh/re-enrich CanvasAgentRunContext from updated active VFS/current surface projection
  -> return snapshot with same runtime resource surface binding
```

### Workspace Module operate/invoke/present

```text
ExecutionContext / AgentRun delivery
  -> WorkspaceModuleRuntimeContextResolver
     -> delivery runtime session
     -> Project authorization context
     -> SharedRuntimeVfs + access policy
     -> backend readiness + ResolvedInvocationBackend
     -> RuntimeGateway actor/context + runtime action catalog
     -> AgentRun bridge handle when available
  -> WorkspaceModuleAgentSurface command intent
     -> list/operate/invoke/present modules
     -> Canvas operations consume Canvas-specific child context
```

The command structs remain useful as intent carriers, but runtime placement and backend facts should come from the resolved context. Tests should exercise the context interface rather than rebuilding each command's parameter list.

### Canvas iframe assets

```text
agentdash.assets.url("mount://path")
  -> parent page reads snapshot.resource_surface_ref
  -> POST /vfs-surfaces/read-file-blob
  -> VfsSurfaceResolver parses session-runtime / agent-run ref
  -> reads current runtime VFS
```

### Extension Canvas panel

Packaged Canvas snapshot is static authoring/runtime preview content. When rendered inside an AgentRun workspace, the parent panel must bind it to the AgentRun Canvas runtime context before enabling resource asset resolution. If the package snapshot references a Canvas mount that is not exposed in the current runtime context, the panel should surface a host-level unavailable state.

## Contract Direction

Canvas runtime DTO should make action surface and resource surface explicit:

- `resource_surface_ref`: VFS resource read surface for iframe asset URLs.
- `runtime_bridge`: action invocation bridge/catalog.

The current split where普通 Canvas snapshot uses `runtime_bridge.surface` and AgentRun snapshot uses `runtime_bridge.actions` should be collapsed or renamed so frontend code no longer performs shape probing.

Recommended direction:

- Use one `CanvasRuntimeBridgeSnapshotDto` shape for both ordinary and AgentRun snapshots.
- Keep `surface?: RuntimeSurfaceDto` when action context matters.
- If a lightweight action list is desired, name it as a separate `action_catalog` field rather than overloading `runtime_bridge`.

## Backend Selection

Backend choice belongs to the AgentRun current runtime surface closure, not to ad hoc Canvas route parameters. Canvas runtime context should expose the selected backend anchor when resource access or extension/channel invocation needs it.

This means Canvas code never submits backend ids from iframe/runtime JS. Parent/API host resolves backend placement from current AgentRun runtime surface, matching RuntimeGateway's existing admission model.

## Adjacent Module Audit

这类问题在项目里不是“缺少绑定层”，而是“已有绑定层之后，局部消费路径还在传裸字段”。第一轮横向评估如下：

- AgentRun runtime surface：整体是健康边界。`DeliveryRuntimeSelectionService` 和 `AgentRunRuntimeSurfaceQuery` 已集中解析 current delivery、execution anchor、current AgentFrame、VFS、backend anchor 和权限；`AgentRunResourceSurfaceQuery` 也能把 current runtime surface 投影成 workspace resource surface。Canvas 应复用这些入口，不应在 route/local helper 中重新拼 `runtime_session_id`、VFS 和 resource ref。
- Workspace Module：应纳入本任务一起收束。`WorkspaceModuleResolveContext`、`WorkspaceModuleOperationRuntimeSource`、`ResolvedInvocationBackend` 已经把 module visibility、operation catalog、backend readiness 集中起来；`canvas.inspect` / `canvas.get_interaction_state` 会从 runtime session 反查 execution anchor，再按 `(run_id, agent_id, canvas_mount_id)` 读取 Canvas latest state。下一步是把 operate/invoke/present/canvas runtime surface update 共用的 delivery runtime session、current user、SharedRuntimeVfs、backend、AgentRun bridge 收成 `WorkspaceModuleRuntimeContext`，让 Canvas snapshot projection 和工具执行共享同一份 runtime facts。
- Extension runtime：后端 action/channel invocation 基本健康。API 只接收 `session_id`，再通过 `resolve_current_runtime_surface_with_backend_for_project_for_api` 解出 current surface 与 backend anchor，并校验 Project/backend access。风险集中在 `ExtensionCanvasPanel`：它加载静态 packaged Canvas snapshot 后，把 AgentRun bridge base 与 `snapshot.canvas_mount_id` 重新拼接给 `CanvasRuntimePreview`，但 packaged snapshot 本身没有 runtime resource surface；这和本任务的 Canvas snapshot 缺 resource surface 是同类症状。
- VFS surface：当前模型健康。`surface_ref` 是 VFS surface API 的公开坐标，route 每次 parse `ResolvedVfsSurfaceSource` 并重新 resolve bundle，再执行 read/write/upload。前端传 `surface_ref` 本身不是裸奔；真正的问题是上游 Canvas snapshot 没有从 AgentRun current surface 投影出正确的 canonical `resource_surface_ref`。
- MCP runtime binding：当前模型健康。`CurrentSurfaceRuntimeMcpAccess` 通过 current runtime MCP surface with backend 构造 discovery/call context；`McpRuntimeBindingContext` 只消费已解析的 VFS 与 backend anchor，没有直接让调用方指定 backend。
- Companion gate/control：已有较强绑定层。`CompanionToolContext` 从 ExecutionContext 提取 delivery runtime session，并通过 `resolve_current_frame_from_delivery_trace_ref` 解析 lifecycle anchor；`CompanionGateControlService` 在 parent/child gate resolve 时也会用 runtime session 反查当前 frame 并校验 gate owner。这里目前不像 Canvas 一样缺少 resource/action projection，但命令对象里 parent/child runtime session id 较多，后续改动应继续让 gate control service 持有校验闭包，而不是把这些 id 当作调用方权威。

因此，本任务实现范围聚焦 Canvas 用户问题，同时包含 Workspace Module 内部 runtime context 收束。其它相邻模块作为对照边界和回归观察点。

## Trade-Offs

- Reusing `CanvasAgentRunContext` and Canvas runtime state avoids a second binding truth and aligns with migration 0026's stated ownership model.
- The missing piece is not storage; it is projection: snapshot/resource/action/backend consumers are not consistently reading the existing binding layer.
- Pulling Workspace Module runtime facts into one context increases the implementation slice, but it gives the fix locality: Canvas present, bind_data, inspect, action invocation and snapshot resource projection all exercise the same interface.
- Unifying DTO shape may touch generated contracts and frontend types, but this is preferable in the current pre-release phase because the existing split is already causing semantic drift.

## Validation

- Backend API/unit tests prove existing `CanvasAgentRunContext`/runtime state derives resource surface, action surface, VFS, Project and backend from the same AgentRun Canvas binding.
- Workspace Module tests prove operate/invoke/present/canvas runtime update consume the shared runtime context and report clear diagnostics when session/backend/bridge facts are unavailable.
- Frontend tests prove `agentdash.assets.url` works for AgentRun snapshots without relying on `session_id`.
- Contract check proves Canvas DTO changes are generated and consumed consistently.
