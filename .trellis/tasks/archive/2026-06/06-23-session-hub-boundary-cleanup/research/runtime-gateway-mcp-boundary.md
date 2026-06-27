# Research: RuntimeGateway MCP Boundary

- Query: 调查 RuntimeGateway MCP session action 与 MCP discovery / tool assembly 边界，找出 `mcp.list_tools` / `mcp.call_tool` 应迁出的正确 access 形态。
- Scope: internal
- Date: 2026-06-23

## Findings

### Files Found

- `.trellis/workflow.md` — Trellis planning/research workflow and research persistence rules.
- `.trellis/spec/backend/runtime-gateway.md` — RuntimeGateway action kind、actor/context admission、Session MCP action 和 runtime tool declaration boundary 规范。
- `.trellis/spec/backend/session/architecture.md` — Session/RuntimeSession 与 AgentFrame control-plane 事实源边界。
- `.trellis/spec/backend/session/execution-context-frames.md` — `ExecutionContext`、active turn snapshot、tool hot update 边界。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` — MCP tool capability、discovery provenance、tool policy 过滤规范。
- `crates/agentdash-application/src/runtime_gateway/session_actions.rs` — `mcp.list_tools` / `mcp.call_tool` provider、`RuntimeSessionMcpAccess` trait、tool execution adapter。
- `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` — API bootstrap 把 MCP providers 注册到 `RuntimeGateway`。
- `crates/agentdash-api/src/app_state.rs` — 当前把 `SessionCapabilityService` 注入成 `Arc<dyn RuntimeSessionMcpAccess>`。
- `crates/agentdash-application/src/session/capability_service.rs` — 当前混合 facade，包含 RuntimeGateway MCP access、Canvas exposure/adoption、runtime command、capability view。
- `crates/agentdash-application/src/session/tool_assembly.rs` — prompt/launch tool assembly，把 `ExecutionContext` 转成 callable tools 与 `RuntimeToolSchemaEntry`。
- `crates/agentdash-application/src/session/hub/tool_builder.rs` — active turn tool refresh、hub adoption、当前 idle MCP discovery 错误归属点。
- `crates/agentdash-application-ports/src/mcp_discovery.rs` — application port: `McpToolDiscoveryRequest`、`DiscoveredMcpTool`、`McpToolDiscovery`。
- `crates/agentdash-executor/src/mcp/mod.rs` — executor discovery implementation，拆 direct / relay MCP server。
- `crates/agentdash-executor/src/mcp/direct.rs` — direct HTTP MCP list/call adapter。
- `crates/agentdash-executor/src/mcp/relay.rs` — relay MCP list/call adapter，调用 `McpRelayProvider`。
- `crates/agentdash-spi/src/platform/mcp_relay.rs` — relay MCP SPI 与 `RelayMcpCallContext`。
- `crates/agentdash-spi/src/connector/mod.rs` — `ExecutionSessionFrame.runtime_backend_anchor`、`RuntimeToolProvider` 和 connector-facing `ExecutionContext`。
- `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs` — `FrameLaunchSurface` 和 `runtime_backend_anchor(...)` 派生逻辑。
- `crates/agentdash-application/src/agent_run/frame/construction/mod.rs` — `build_envelope_from_frame(...)` 用 closed launch surface 生成 `FrameLaunchEnvelope.runtime_backend_anchor`。
- `crates/agentdash-application/src/agent_run/frame/surface.rs` — `AgentFrameSurfaceExt` typed surface reader。
- `crates/agentdash-application/src/lifecycle/session_association.rs` — `resolve_current_frame_from_delivery_trace_ref(...)` 已有 runtime session -> current AgentFrame 解析 helper。
- `crates/agentdash-api/src/session_construction.rs` — 当前 `resolve_session_frame_vfs(...)` 用 current frame 取 VFS，但 backend anchor 仍走 active-turn-only helper。
- `crates/agentdash-api/src/routes/canvases.rs` — Canvas runtime bridge snapshot 与 runtime invoke route。
- `crates/agentdash-api/src/routes/extension_runtime.rs` — Extension runtime 当前依赖 `resolve_session_frame_vfs` 取得 backend target。
- `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs` — VFS `SessionRuntime` source 当前依赖 `resolve_session_frame_vfs`。
- `crates/agentdash-api/src/routes/permission_grants.rs` — Permission grant effect 当前直接调用 active runtime adoption primitive。

### Related Specs

- `.trellis/spec/backend/runtime-gateway.md`: Session MCP action 仍属于 `RuntimeActionKind::SessionRuntime`，但 MCP runtime server surface 来源应是 current AgentFrame / active execution surface；`surface_for_actor` 只表达 action visibility，不表达具体 MCP tool surface。
- `.trellis/spec/backend/session/architecture.md`: `RuntimeSession` 是 delivery / trace substrate；`AgentFrame` 是 capability / context / VFS / MCP 的事实源；runtime trace 通过 `RuntimeSessionExecutionAnchor` 回到 control-plane。
- `.trellis/spec/backend/session/execution-context-frames.md`: `ExecutionContext` 是 connector-facing projection；active `TurnExecution` 是 per-turn runtime 快照，不承担 owner/context/VFS 解析；tool hot update 只服务 live connector replace-set。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`: `CapabilityState.tool_policy` 是工具暴露层必须消费的唯一 policy；MCP discovery provenance 必须保留并同源生成 `DynAgentTool` 与 `RuntimeToolSchemaEntry`。

### Current Complete Call Chain: `mcp.list_tools`

1. Canvas route constructs session invocation:
   - `invoke_canvas_runtime_action(...)` loads Canvas and validates `session_id` in `crates/agentdash-api/src/routes/canvases.rs:368`.
   - It builds `RuntimeInvocationRequest::new(...)` with `RuntimeActor::UserCanvas { session_id, canvas_id }` and `RuntimeContext::Session { session_id, project_id, workspace_id: None }` in `crates/agentdash-api/src/routes/canvases.rs:386`.
   - It calls `state.services.runtime_gateway.invoke(request).await` in `crates/agentdash-api/src/routes/canvases.rs:400`.
2. RuntimeGateway admission:
   - `RuntimeGateway::invoke(...)` locates static/dynamic provider, validates request, checks `provider.supports(...)`, then invokes provider in `crates/agentdash-application/src/runtime_gateway/gateway.rs:83`.
   - Session actor/context admission requires non-empty context `session_id`, actor with a session id, and actor/context session equality in `crates/agentdash-application/src/runtime_gateway/gateway.rs:162`.
   - `surface_for_actor(...)` only returns action descriptors filtered by action kind after actor/context validation in `crates/agentdash-application/src/runtime_gateway/gateway.rs:65`; it does not perform MCP discovery.
3. Provider bootstrap:
   - `AppState::new_with_integrations(...)` casts `SessionCapabilityService` to `Arc<dyn RuntimeSessionMcpAccess>` in `crates/agentdash-api/src/app_state.rs:232`.
   - `build_runtime_gateway(...)` receives `session_mcp_access` in `crates/agentdash-api/src/bootstrap/runtime_gateway.rs:16`.
   - It registers `McpListToolsProvider::new(session_mcp_access.clone())` and `McpCallToolProvider::new(session_mcp_access)` in `crates/agentdash-api/src/bootstrap/runtime_gateway.rs:35`.
4. `McpListToolsProvider`:
   - `MCP_LIST_TOOLS_ACTION` is `"mcp.list_tools"` in `crates/agentdash-application/src/runtime_gateway/session_actions.rs:14`.
   - `RuntimeSessionMcpAccess` defines `list_mcp_tools(&self, session_id)` in `crates/agentdash-application/src/runtime_gateway/session_actions.rs:66`.
   - `McpListToolsProvider` is a `RuntimeProvider` in `crates/agentdash-application/src/runtime_gateway/session_actions.rs:95`.
   - Provider deserializes `McpListToolsInput`, requires `RuntimeContext::Session`, calls `access.list_mcp_tools(&session_id)`, then applies optional `server_names` filtering before returning `McpListToolsOutput`.
5. Current access implementation:
   - `SessionCapabilityService` implements `RuntimeSessionMcpAccess` in `crates/agentdash-application/src/session/capability_service.rs:265`.
   - `list_mcp_tools(...)` delegates to `self.hub.discover_runtime_mcp_tool_entries(session_id)` in `crates/agentdash-application/src/session/capability_service.rs:270`.
   - It maps each `DiscoveredMcpTool` into `RuntimeMcpToolDescriptor` in `crates/agentdash-application/src/session/capability_service.rs:275`.
6. Hub discovery:
   - `SessionRuntimeInner::discover_runtime_mcp_tool_entries(...)` starts in `crates/agentdash-application/src/session/hub/tool_builder.rs:374`.
   - Active turn branch reads `turn.session_frame.mcp_servers`, `runtime_backend_anchor`, `vfs`, `identity`, `turn_id`, and `turn.capability_state` from live registry in `crates/agentdash-application/src/session/hub/tool_builder.rs:378`.
   - Idle branch resolves current frame via `RuntimeSessionExecutionAnchor -> LifecycleAgent -> AgentFrame` in `crates/agentdash-application/src/session/hub/tool_builder.rs:402`.
   - The idle branch then returns `frame.typed_mcp_servers()`, `frame.typed_vfs()`, and `project_capability_state_from_frame(&frame)`, but sets `backend_anchor` and `identity` to `None` in `crates/agentdash-application/src/session/hub/tool_builder.rs:428`.
   - Immediately after, it requires `backend_anchor` and emits `RuntimeBackendAnchorError::Missing { component: "runtime_mcp_tool_discovery", session_id, turn_id }` in `crates/agentdash-application/src/session/hub/tool_builder.rs:437`.
   - It applies AgentRun effective capability projection in `crates/agentdash-application/src/session/hub/tool_builder.rs:447`.
   - It calls `mcp_tool_discovery.discover_tool_entries(McpToolDiscoveryRequest { servers, capability_state, call_context })` in `crates/agentdash-application/src/session/hub/tool_builder.rs:454`.
7. MCP discovery port and implementation:
   - `McpToolDiscoveryRequest` contains `servers`, `capability_state`, and optional `RelayMcpCallContext` in `crates/agentdash-application-ports/src/mcp_discovery.rs:17`.
   - `ExecutorMcpToolDiscovery::discover_tool_entries(...)` partitions requested servers into relay/direct, discovers direct entries, then extends relay entries if a relay provider exists in `crates/agentdash-executor/src/mcp/mod.rs:37`.
   - Direct discovery lists HTTP MCP tools and filters by `capability_state.is_capability_tool_enabled(...)` in `crates/agentdash-executor/src/mcp/direct.rs:218` and `crates/agentdash-executor/src/mcp/direct.rs:250`.
   - Relay discovery calls `McpRelayProvider::list_relay_tools(servers, call_context)` and filters by requested server plus capability policy in `crates/agentdash-executor/src/mcp/relay.rs:112`.

### Current Complete Call Chain: `mcp.call_tool`

1. Route/admission/bootstrap path is identical to `mcp.list_tools` until provider dispatch.
2. `MCP_CALL_TOOL_ACTION` is `"mcp.call_tool"` in `crates/agentdash-application/src/runtime_gateway/session_actions.rs:15`.
3. `McpCallToolProvider` is a `RuntimeProvider` in `crates/agentdash-application/src/runtime_gateway/session_actions.rs:176`.
4. Provider deserializes `McpCallToolInput`, validates that either `runtime_name` or `server_name + tool_name` is present, requires object/null arguments, obtains `session_id`, then calls `access.call_mcp_tool(&session_id, input)`.
5. Current `SessionCapabilityService::call_mcp_tool(...)` calls the same `self.hub.discover_runtime_mcp_tool_entries(session_id)` in `crates/agentdash-application/src/session/capability_service.rs:293`.
6. It finds the selected entry by `runtime_name` or `server_name + tool_name` in `crates/agentdash-application/src/session/capability_service.rs:298`.
7. It executes the selected `entry.tool` through `execute_runtime_mcp_tool(...)` in `crates/agentdash-application/src/session/capability_service.rs:306`.
8. `execute_runtime_mcp_tool(...)` calls `DynAgentTool::execute("rt-mcp-{runtime_name}", arguments, CancellationToken::new(), None)` in `crates/agentdash-application/src/runtime_gateway/session_actions.rs:264`.
9. If the discovered tool is relay-backed, `RelayMcpToolAdapter::execute(...)` calls `McpRelayProvider::call_relay_tool(...)` with the cloned `RelayMcpCallContext` plus `tool_call_id` in `crates/agentdash-executor/src/mcp/relay.rs:66`.
10. If the discovered tool is direct HTTP MCP, `McpToolAdapter::execute(...)` calls the direct rmcp client `call_tool(...)` in `crates/agentdash-executor/src/mcp/direct.rs:176`.

### Boundary Map

#### Runtime action declaration

- RuntimeGateway declaration is action-level only.
- `RuntimeGateway::surface_for_actor(...)` exposes descriptors for providers whose `action_kind()` matches context kind in `crates/agentdash-application/src/runtime_gateway/gateway.rs:65`.
- Canvas snapshot uses this to enable bridge surface in `build_canvas_runtime_bridge_surface(...)` in `crates/agentdash-api/src/routes/canvases.rs:524`.
- This boundary should never imply MCP server/tool closure. It should not call `McpToolDiscovery`, not inspect `AgentFrame`, and not require backend anchor.

#### Prompt tool assembly

- Prompt/launch tool assembly starts from an already prepared `ExecutionContext`.
- `LaunchPlan` builds `ExecutionSessionFrame` containing `mcp_servers`, `vfs`, `runtime_backend_anchor`, and `identity` in `crates/agentdash-application/src/session/launch/plan.rs:265`.
- `TurnPreparation` calls `deps.assemble_tool_surface(&session_id, &context)` and writes `context.turn.assembled_tools` in `crates/agentdash-application/src/session/launch/preparation.rs:100`.
- `TurnPreparationDeps::assemble_tool_surface(...)` delegates to `session::tool_assembly::assemble_tool_surface_for_execution_context(...)` in `crates/agentdash-application/src/session/launch/deps.rs:185`.
- `assemble_tool_surface_for_execution_context(...)` calls `RuntimeToolProvider::build_tools(context)` for platform runtime tools, then uses `McpToolDiscoveryRequest` for direct/relay MCP in `crates/agentdash-application/src/session/tool_assembly.rs:17`.
- It requires `context.session.runtime_backend_anchor` before MCP discovery in `crates/agentdash-application/src/session/tool_assembly.rs:42` and passes `RelayMcpCallContext { session_id, turn_id, backend_anchor, vfs, identity }` in `crates/agentdash-application/src/session/tool_assembly.rs:55`.
- This boundary is declaration/assembly for the agent/connector. It may list MCP tools because schemas/tools are needed for prompt execution, but it must consume an already closed `ExecutionContext`; it should not resolve idle runtime session -> AgentFrame itself.

#### Active turn tool refresh

- Active tool refresh is live runtime coordination and should stay in session live runtime.
- `SessionRuntimeInner::adopt_persisted_agent_frame_revision(...)` validates target frame, requires an active turn, recomposes tools from the new frame-derived surface, calls `connector.update_session_tools(...)`, updates active turn cache, and emits runtime context transition in `crates/agentdash-application/src/session/hub/tool_builder.rs:183`.
- The active turn requirement is explicit: the function errors when no active turn exists in `crates/agentdash-application/src/session/hub/tool_builder.rs:250`.
- This code owns transient live coordination: connector replace-set, active turn cache, hook runtime binding, transition notification. It should remain in hub or a live-runtime service.
- The migration target is not to remove active refresh, but to make its input a closed runtime surface / `AgentFrameRuntimeTarget` from an application use case, not an API/business direct adoption call.

#### User/runtime action invoke

- Canvas/user runtime invoke is explicit action execution, not prompt assembly and not connector tool refresh.
- Canvas route only supplies actor/context and input in `crates/agentdash-api/src/routes/canvases.rs:386`.
- RuntimeGateway admission only validates actor/context/action support in `crates/agentdash-application/src/runtime_gateway/gateway.rs:83`.
- Provider should deserialize/validate `McpListToolsInput` or `McpCallToolInput`, then call `RuntimeSessionMcpAccess`.
- `RuntimeSessionMcpAccess` should query a current closed runtime surface from AgentRun/AgentFrame control-plane, feed that surface to `McpToolDiscovery`, and for call execute the selected `DynAgentTool`.
- It should not depend on `SessionRuntimeInner`, active turn cache, or hub idle fallback logic.

### What Must Stay In Session Live Runtime

- Runtime registry and active `TurnExecution` state.
- Connector session lifecycle and active turn supervision.
- Live connector tool update via `connector.update_session_tools(...)`.
- Active turn cache update for `ExecutionSessionFrame.mcp_servers`, VFS, capability state, and hook/runtime transition emission.
- Hook runtime delivery binding cache and per-turn `turn_id` projections.
- Launch/turn preparation consumption of an already closed `ExecutionContext`.
- Pure assembly helper `assemble_tool_surface_for_execution_context(...)` can remain reusable as long as it takes a closed `ExecutionContext` / closed surface and does not do idle control-plane lookup.

### What Should Move To Current AgentFrame Surface Resolver

- `runtime_session_id -> RuntimeSessionExecutionAnchor -> LifecycleAgent -> AgentFrameRepository.get_current(agent_id)` lookup currently duplicated in hub idle paths and API helpers.
- Closed surface construction from current `AgentFrame`: typed VFS, typed MCP servers, typed capability state, execution profile where needed, runtime backend anchor, identity/admission context, provenance.
- `get_current_runtime_backend_anchor(session_id)` as a business/API query. Current implementation only reads active turn cache in `crates/agentdash-application/src/session/hub/tool_builder.rs:111`.
- `resolve_session_frame_vfs(...)` should become a consumer of the resolver. It currently gets VFS from current frame, but backend anchor from `session_capability.get_current_runtime_backend_anchor(session_id).await.ok()` in `crates/agentdash-api/src/session_construction.rs:70`.
- RuntimeGateway MCP access should consume the resolver instead of `SessionCapabilityService -> SessionRuntimeInner`.
- Canvas runtime snapshot/VFS binding, Extension runtime backend target, Terminal backend target, and VFS `SessionRuntime` source should consume the same resolver/facade rather than assembling partial surfaces independently.
- AgentRun effective capability/admission projection should be applied in or immediately after this resolver, so all consumers share one capability closure.

### Recommended New Access Shape

The most suitable new implementation is a dedicated application-layer access, for example:

```rust
pub struct AgentRunRuntimeSessionMcpAccess {
    surface_resolver: Arc<dyn CurrentRuntimeSessionSurfaceResolver>,
    mcp_tool_discovery: Arc<dyn McpToolDiscovery>,
}
```

This should replace `SessionCapabilityService: RuntimeSessionMcpAccess`.

The resolver should live under AgentRun/AgentFrame ownership, for example `crates/agentdash-application/src/agent_run/runtime_surface_query.rs` or `crates/agentdash-application/src/agent_run/frame/runtime_surface_query.rs`. It should not live in `session/hub` or API routes.

Suggested resolver dependencies:

- `RuntimeSessionExecutionAnchorRepository`
- `LifecycleAgentRepository`
- `LifecycleRunRepository` when project/run provenance or permission checks need to be returned
- `AgentFrameRepository`
- `PermissionGrantRepository` or `AgentRunEffectiveCapabilityService` input dependencies, if final execution capability projection should be emitted by the resolver
- Optional identity source only if current-frame idle actions need a persisted/derived identity; otherwise output should explicitly mark `identity: None` rather than mixing with active turn identity

Suggested resolver input:

```rust
pub struct CurrentRuntimeSessionSurfaceRequest {
    pub runtime_session_id: String,
    pub component: &'static str,
    pub require_backend_anchor: bool,
}
```

If API permission/project binding is handled in the facade rather than the route, add:

```rust
pub struct CurrentRuntimeSessionSurfaceAuth {
    pub current_user: Option<AuthIdentity>,
    pub required_project_permission: Option<ProjectPermission>,
}
```

Suggested resolver output:

```rust
pub struct CurrentRuntimeSessionSurface {
    pub runtime_session_id: String,
    pub anchor: RuntimeSessionExecutionAnchor,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub frame: AgentFrame,
    pub vfs: Option<Vfs>,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub capability_state: CapabilityState,
    pub runtime_backend_anchor: Option<RuntimeBackendAnchor>,
    pub identity: Option<AuthIdentity>,
    pub source: RuntimeSurfaceSource,
}
```

`runtime_backend_anchor` should be derived with the same semantics as `FrameLaunchSurface::runtime_backend_anchor(...)`. The existing implementation derives backend id/root/workspace metadata from closed VFS default mount in `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs:187`, and launch construction already stores that value in `FrameLaunchEnvelope` in `crates/agentdash-application/src/agent_run/frame/construction/mod.rs:385`.

For MCP access, the implementation should build:

```rust
McpToolDiscoveryRequest {
    servers: surface.mcp_servers.clone(),
    capability_state: surface.capability_state.clone(),
    call_context: Some(RelayMcpCallContext {
        session_id: surface.runtime_session_id.clone(),
        turn_id: None, // or active turn id only if an explicit live-turn overlay is added
        tool_call_id: None,
        backend_anchor: surface.runtime_backend_anchor.clone(),
        vfs: surface.vfs.clone(),
        identity: surface.identity.clone(),
    }),
}
```

`mcp.list_tools` output remains `Vec<RuntimeMcpToolDescriptor>`.

`mcp.call_tool` input remains `McpCallToolInput`, but the new access should resolve and discover from the current surface, match by `runtime_name` or `server_name + tool_name`, then execute the selected `DynAgentTool` through the existing `execute_runtime_mcp_tool(...)`.

### Why Canvas Idle Exposes Backend Anchor Missing

- Canvas bridge snapshot asks `RuntimeGateway::surface_for_actor(...)`, which only proves `mcp.list_tools` is a visible Session Runtime action for `UserCanvas` + `RuntimeContext::Session` in `crates/agentdash-api/src/routes/canvases.rs:524`.
- Actual iframe call enters `/runtime-invoke`, builds `RuntimeActor::UserCanvas` and `RuntimeContext::Session`, and calls `RuntimeGateway::invoke(...)` in `crates/agentdash-api/src/routes/canvases.rs:386`.
- Gateway invokes `McpListToolsProvider`, which calls the current `RuntimeSessionMcpAccess`.
- Current access is `SessionCapabilityService`, which delegates to hub discovery.
- In idle/non-turn state, hub cannot read active `turn.session_frame.runtime_backend_anchor`; it resolves current frame and reads MCP/VFS/capability, but explicitly returns `backend_anchor = None` and `identity = None` in `crates/agentdash-application/src/session/hub/tool_builder.rs:428`.
- The next block requires `backend_anchor` and raises `runtime_mcp_tool_discovery` missing anchor with `turn_id=None` in `crates/agentdash-application/src/session/hub/tool_builder.rs:437`.
- Therefore the Canvas failure is not caused by route actor/context. It exposes that idle RuntimeGateway MCP access is wrongly backed by session live runtime/hub instead of the current AgentFrame surface closure.

### Minimal Regression Test Placement

Best minimal split:

1. New application-layer resolver test near the new resolver module, e.g. `crates/agentdash-application/src/agent_run/runtime_surface_query.rs`.
   - Build in-memory `RuntimeSessionExecutionAnchorRepository`, `LifecycleAgentRepository`, `LifecycleRunRepository`, and `AgentFrameRepository`.
   - Create a current `AgentFrame` with typed VFS default mount containing `backend_id`, typed MCP servers, and `CapabilityState` that enables the MCP server.
   - Assert resolver returns the same frame id/revision, VFS, MCP servers, capability state, project/run/agent provenance, and a non-empty `RuntimeBackendAnchor`.
   - Add an active-turn/live-overlay variant only if the chosen resolver contract supports live-turn source; otherwise keep active turn tests in hub.
2. New MCP access test near the new access implementation, e.g. `crates/agentdash-application/src/runtime_gateway/session_actions.rs` only if the access stays in runtime_gateway, or a new `crates/agentdash-application/src/agent_run/runtime_session_mcp_access.rs`.
   - Use fake resolver returning an idle `CurrentRuntimeSessionSurface` with `runtime_backend_anchor: Some(...)`.
   - Use fake `McpToolDiscovery` that captures `RelayMcpCallContext.backend_anchor`.
   - Invoke real `RuntimeGateway` with `RuntimeActor::UserCanvas` and `RuntimeContext::Session` for `MCP_LIST_TOOLS_ACTION`.
   - Assert output contains discovered tool and captured context has the backend anchor.
3. Existing provider tests in `crates/agentdash-application/src/runtime_gateway/session_actions.rs` should remain provider-only validation tests. They currently use `FakeRuntimeSessionMcpAccess` and only prove provider maps access results, not current-frame idle discovery.

An API route integration test in `crates/agentdash-api/src/routes/canvases.rs` would be heavier and not necessary for the minimum regression, because route code already constructs the correct actor/context; the failure is in application backing access.

### Migration Steps

1. Add current runtime session surface resolver under AgentRun/AgentFrame application module.
   - Start from existing `resolve_current_frame_from_delivery_trace_ref(...)` in `crates/agentdash-application/src/lifecycle/session_association.rs:38`.
   - Return a typed `CurrentRuntimeSessionSurface` that includes anchor, run/project/agent/frame provenance, typed VFS, MCP servers, capability state, backend anchor, and identity/admission context.
   - Reuse `FrameLaunchSurface::runtime_backend_anchor(...)` semantics rather than duplicating VFS mount parsing.
2. Implement `AgentRunRuntimeSessionMcpAccess`.
   - Depend on resolver + `McpToolDiscovery`.
   - Build `McpToolDiscoveryRequest` from the resolved surface.
   - Move list descriptor mapping and call target matching out of `SessionCapabilityService`.
3. Change `AppState` bootstrap.
   - Stop casting `SessionCapabilityService` to `Arc<dyn RuntimeSessionMcpAccess>` in `crates/agentdash-api/src/app_state.rs:232`.
   - Construct the new resolver/access and pass it to `build_runtime_gateway(...)`.
4. Remove RuntimeGateway MCP access from `SessionCapabilityService`.
   - Delete `impl RuntimeSessionMcpAccess for SessionCapabilityService`.
   - Keep or split remaining responsibilities into explicit services/use cases: live adoption, Canvas exposure, pending runtime command, capability view, skill baseline.
5. Remove idle AgentFrame surface lookup from `session/hub/tool_builder.rs`.
   - `discover_runtime_mcp_tool_entries(...)` should either be deleted or reduced to active-turn/live refresh use only.
   - `get_runtime_mcp_servers(...)` idle fallback and `get_latest_capability_state(...)` current-frame fallback should move to resolver/facade if business consumers need them.
6. Replace API/business partial helpers.
   - `resolve_session_frame_vfs(...)` should delegate to the new resolver and no longer call active-turn-only `get_current_runtime_backend_anchor(...)`.
   - Extension runtime, Terminal route, Canvas runtime VFS snapshot, and VFS `SessionRuntime` source should all consume the resolver/facade result.
7. Encapsulate adoption primitive.
   - `adopt_persisted_agent_frame_revision(...)` should remain live-runtime internal or behind a surface update use case.
   - `permission_grants.rs` currently calls it directly in `crates/agentdash-api/src/routes/permission_grants.rs:288`; route-level business code should instead call a grant/surface update application use case.
8. Add tests.
   - Resolver closed-surface test.
   - RuntimeGateway `UserCanvas` idle `mcp.list_tools` test through real provider + new access.
   - Existing hub active turn adoption/tool refresh tests should remain, updated only for renamed/internalized APIs.
   - Optional follow-up tests for Extension runtime backend target and VFS `SessionRuntime` source once those consumers are migrated.

### Risks

- Active turn freshness: if current AgentFrame revision is updated but active turn cache has not adopted it, RuntimeGateway MCP access via resolver may see current frame while live connector still has older tool set. For user runtime actions this is acceptable if AgentFrame is declared the control-plane truth; for connector live refresh, hub must still coordinate adoption.
- Identity semantics: active turn `ExecutionSessionFrame.identity` exists, but idle current frame does not currently carry an obvious identity. New resolver must make identity source explicit. Do not silently invent identity from API user unless the use case requires and records that admission context.
- Capability projection duplication: current hub applies `AgentRunEffectiveCapabilityService::execution_capability_state_for_runtime_session(...)` before discovery in `crates/agentdash-application/src/session/hub/tool_builder.rs:497`. The resolver/access migration must preserve this projection or all MCP tool filtering may widen/narrow unexpectedly.
- Direct vs relay MCP behavior: direct MCP can list/call without backend anchor, but relay MCP requires backend placement context. The new resolver should still require backend anchor for runtime MCP discovery when relay-capable servers are present; do not hide missing backend anchor by silently dropping relay tools unless product semantics explicitly want partial discovery.
- API permission boundaries: current `resolve_session_frame_vfs(...)` performs project permission check after resolving run/project. Moving it to application resolver may require either a permission-aware facade or keeping API permission check around the resolver result.
- SessionCapabilityService split risk: it currently bundles unrelated entry points. Migrating MCP first should avoid renaming every consumer at once, but the final state should prevent new consumers from treating it as a general runtime surface query facade.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task, but the dispatch prompt provided the explicit task path and write target. Research was written only under `.trellis/tasks/06-23-session-hub-boundary-cleanup/research/`.
- No production code was changed and no tests were run.
- No existing single resolver/facade already returns closed surface with VFS, MCP servers, capability state, backend anchor, identity, and provenance. Existing helpers are partial: `resolve_current_frame_from_delivery_trace_ref(...)` returns anchor/agent/frame; `resolve_session_frame_vfs(...)` returns VFS plus an active-turn-only backend anchor attempt.
- No external documentation was required for this investigation. MCP protocol execution details were inferred from local `agentdash-executor` direct/relay adapters and SPI ports.
