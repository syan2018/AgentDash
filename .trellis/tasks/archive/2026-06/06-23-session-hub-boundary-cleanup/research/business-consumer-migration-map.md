# Research: business-consumer-migration-map

- Query: 调查 Canvas / WorkspaceModule / Extension runtime / VFS surface / Permission 对 `session_capability`、`resolve_session_frame_vfs`、runtime backend anchor、active runtime adoption primitive 的依赖，并形成旧路径迁移矩阵。
- Scope: internal
- Date: 2026-06-23

## Findings

### Files found

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-23-session-hub-boundary-cleanup/prd.md` | 任务需求，明确 Canvas idle `mcp.list_tools` 缺 backend anchor 是触发点，但目标是收束 session hub 错误归属。 |
| `.trellis/tasks/06-23-session-hub-boundary-cleanup/design.md` | 已有设计草案，定义 `CurrentRuntimeSurfaceResolver` 方向、query/update 边界与初始迁移矩阵。 |
| `.trellis/tasks/06-23-session-hub-boundary-cleanup/implement.md` | 分阶段计划，特别是 Phase 2 迁移 MCP access、Phase 3 迁移 API consumer、Phase 4 收敛 update/adoption。 |
| `.trellis/spec/backend/session/architecture.md` | Session 边界：`RuntimeSession` 是 delivery/trace substrate，AgentFrame 是 capability/context/VFS/MCP 事实源。 |
| `.trellis/spec/backend/session/runtime-execution-state.md` | Session runtime map、active turn、connector live session 的边界；AgentRun workspace/resource surface 从 AgentFrame/AgentRun 闭包得到。 |
| `.trellis/spec/backend/session/execution-context-frames.md` | `ExecutionContext` 是 connector-facing projection，不是 application 层事实源；active turn hot update 可 clone active turn frame。 |
| `.trellis/spec/backend/capability/architecture.md` | AgentRun effective capability/admission 是 runtime 能力读取入口；surface-changing Grant 写 AgentFrame revision，再 adopt 到 active runtime。 |
| `.trellis/spec/backend/vfs/architecture.md` | AgentRun resource surface 由 AgentRun surface resolver 输出，resource browser/connector launch/conversation snapshot 应消费同一 VFS surface。 |
| `.trellis/spec/backend/runtime-gateway.md` | RuntimeGateway Session MCP action、Canvas runtime bridge、Extension runtime action 的 actor/context/target 边界。 |
| `.trellis/spec/backend/permission/architecture.md` | Permission grant lifecycle 与 surface-changing grant 写 frame revision、active runtime adoption 的约束。 |
| `.trellis/spec/backend/permission/grant-lifecycle.md` | Permission grant REST/API 与 active runtime adoption failure 可见错误的测试要求。 |
| `crates/agentdash-api/src/session_construction.rs` | API helper `resolve_session_frame_vfs`：从 runtime session 解析 current AgentFrame VFS，但 backend anchor 又回到 `session_capability` active-turn helper。 |
| `crates/agentdash-api/src/routes/canvases.rs` | Canvas runtime snapshot/invoke route：snapshot 走 `resolve_session_frame_vfs`，invoke 只组装 Canvas actor/context 后进 RuntimeGateway。 |
| `crates/agentdash-api/src/routes/extension_runtime.rs` | Extension action/channel route：通过 `resolve_session_frame_vfs` 获取 VFS/backend anchor，再组装 backend target/workspace metadata。 |
| `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs` | VFS surface resolver：`SessionRuntime` source 走 `resolve_session_frame_vfs`，`AgentRun` source 另行解析 current frame/lifecycle VFS。 |
| `crates/agentdash-api/src/routes/permission_grants.rs` | Permission approve/revoke route：service 写 grant effect 后 route-local 调用 `session_capability.adopt_persisted_agent_frame_revision`。 |
| `crates/agentdash-api/src/routes/terminals.rs` | 额外发现的 consumer：terminal spawn 也通过 `resolve_session_frame_vfs` 获取 VFS/backend anchor。 |
| `crates/agentdash-api/src/app_state.rs` | `SessionCapabilityService` 被注入为 `RuntimeSessionMcpAccess`，供 RuntimeGateway MCP providers 使用。 |
| `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` | `McpListToolsProvider` / `McpCallToolProvider` 当前绑定 `Arc<dyn RuntimeSessionMcpAccess>`。 |
| `crates/agentdash-api/src/bootstrap/session.rs` | runtime tool composer 通过 `SharedSessionToolServicesHandle` 注入 `SessionCapabilityService` 给 WorkspaceModule/Canvas runtime tools。 |
| `crates/agentdash-application/src/runtime_gateway/session_actions.rs` | `mcp.list_tools` / `mcp.call_tool` provider 只从 request context 取 session_id，再委托 `RuntimeSessionMcpAccess`。 |
| `crates/agentdash-application/src/runtime_gateway/extension_actions.rs` | Extension provider 要求 `RuntimeTarget::Backend`；不自行解析 session surface。 |
| `crates/agentdash-application/src/session/capability_service.rs` | 混合 facade：active anchor、runtime target、Canvas exposure/adoption、effective capability view、RuntimeGateway MCP access 都在这里。 |
| `crates/agentdash-application/src/session/hub/tool_builder.rs` | hub 内旧 MCP discovery 与 adoption primitive；idle MCP fallback 从 AgentFrame 取 MCP/VFS/capability 但丢 backend anchor/identity。 |
| `crates/agentdash-application/src/session/tool_assembly.rs` | active turn tool assembly 从 `ExecutionContext.session.runtime_backend_anchor` 构造 MCP discovery call context，属于合法 live runtime refresh 路径。 |
| `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs` | `FrameRuntimeSurface` / launch surface 已有从 frame surface/VFS 生成 runtime backend anchor 的逻辑。 |
| `crates/agentdash-application/src/agent_run/frame/construction/mod.rs` | launch envelope 从 closed frame surface 生成 `runtime_backend_anchor`。 |
| `crates/agentdash-application/src/lifecycle/session_association.rs` | `resolve_current_frame_from_delivery_trace_ref` 已实现 `runtime_session_id -> anchor -> agent -> current AgentFrame`。 |
| `crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs` | run/agent 入口可选择 current delivery runtime/current frame，适合作为 run/agent resolver 输入。 |
| `crates/agentdash-application/src/canvas/tools.rs` | Canvas create/attach/present exposure helper，最终调用 `SessionCapabilityService::expose_canvas_mount_revision_and_adopt`。 |
| `crates/agentdash-application/src/canvas/runtime.rs` | Canvas runtime snapshot model 与 runtime bridge surface DTO；测试仅覆盖桥 surface 可包含 `mcp.list_tools` action key。 |
| `crates/agentdash-application/src/workspace_module/tools.rs` | WorkspaceModule tool：visibility 走 effective capability view；create/present 走 Canvas exposure/adoption；invoke 走 backend target。 |
| `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs` | WorkspaceModule runtime tool assembly：从 active `ExecutionContext` 读取 VFS/backend anchor，构造 invoke tool。 |
| `crates/agentdash-application/src/workspace_module/mod.rs` | Shared helper `resolve_invocation_backend` 从 VFS + backend anchor 选 extension invocation workspace。 |
| `crates/agentdash-api/src/routes/workspace_module.rs` | WorkspaceModule HTTP present 不做 runtime surface mutation；只校验 runtime session/project 绑定并返回 presentation。 |
| `crates/agentdash-application/src/permission/service.rs` | Permission service 写 surface-changing grant 的新 AgentFrame revision，并返回 `effect_frame`。 |

### Related specs

- `.trellis/spec/backend/session/architecture.md`: `RuntimeSession` 不拥有 Agent effective surface；AgentFrame 是 capability/context/VFS/MCP 事实源。
- `.trellis/spec/backend/session/runtime-execution-state.md`: runtime map、active turn、connector live session 分离；workspace resource surface 从 AgentRun runtime address 构造。
- `.trellis/spec/backend/session/execution-context-frames.md`: active turn hot update 可以读 active turn snapshot，但不把 `ExecutionContext` 写回为事实源。
- `.trellis/spec/backend/capability/architecture.md`: AgentRun effective capability/admission 是唯一运行时能力读取入口；surface-changing Grant 写 frame revision。
- `.trellis/spec/backend/vfs/architecture.md`: AgentRun surface resolver 输出已闭包的 resource surface，resource browser、connector launch、conversation snapshot 共享。
- `.trellis/spec/backend/runtime-gateway.md`: RuntimeGateway `surface_for_actor` 只证明 action visibility；Session MCP tool surface 由 action output 返回。
- `.trellis/spec/backend/permission/architecture.md`: approve/revoke 后 active-runtime adoption failure 必须可见。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: AgentRun runtime frame resolution contract 要求 session-facing frame 读从 `runtime_session_id` 与 execution anchor 出发。

### External references

- None. 本调查仅使用仓库内代码、任务文档和 Trellis specs。

### Confirmed code patterns

1. `resolve_session_frame_vfs` 已经用 current frame helper，但 backend anchor 来源仍是旧 active-turn-only helper。
   - `crates/agentdash-api/src/session_construction.rs:26` 定义 `resolve_session_frame_vfs`。
   - `crates/agentdash-api/src/session_construction.rs:37` 调用 `resolve_current_frame_from_delivery_trace_ref`。
   - `crates/agentdash-api/src/session_construction.rs:70` 再通过 `state.services.session_capability.get_current_runtime_backend_anchor(session_id)` 取 anchor。
   - `crates/agentdash-api/src/session_construction.rs:76` 从 current frame 取 `typed_vfs()`。

2. Canvas runtime snapshot 直接消费 `resolve_session_frame_vfs` 的 VFS；Canvas runtime invoke 不直接碰 VFS/backend。
   - `crates/agentdash-api/src/routes/canvases.rs:341` `get_canvas_runtime_snapshot`。
   - `crates/agentdash-api/src/routes/canvases.rs:351` 调用 `resolve_canvas_runtime_vfs`。
   - `crates/agentdash-api/src/routes/canvases.rs:577` `resolve_canvas_runtime_vfs`。
   - `crates/agentdash-api/src/routes/canvases.rs:586` 调用 `resolve_session_frame_vfs(...).vfs`。
   - `crates/agentdash-api/src/routes/canvases.rs:368` `invoke_canvas_runtime_action`。
   - `crates/agentdash-api/src/routes/canvases.rs:386` 组装 `RuntimeInvocationRequest`。
   - `crates/agentdash-api/src/routes/canvases.rs:388` 使用 `RuntimeActor::UserCanvas`。
   - `crates/agentdash-api/src/routes/canvases.rs:392` 使用 `RuntimeContext::Session`。
   - `crates/agentdash-api/src/routes/canvases.rs:400` 调用 `state.services.runtime_gateway.invoke(request)`。

3. Canvas runtime bridge surface 只表示 action visibility，不表示 MCP tool discovery 可执行。
   - `crates/agentdash-api/src/routes/canvases.rs:524` `build_canvas_runtime_bridge_surface`。
   - `crates/agentdash-api/src/routes/canvases.rs:529` 调用 `runtime_gateway.surface_for_actor(...)`。
   - `crates/agentdash-application/src/canvas/runtime.rs:56` `CanvasRuntimeBridgeSnapshot` 持有 `Option<RuntimeSurface>`。
   - `crates/agentdash-application/src/canvas/runtime.rs:296` 测试 `canvas_runtime_bridge_snapshot_can_attach_actor_surface` 仅断言 action key 是 `mcp.list_tools`。

4. Canvas `mcp.list_tools` 报错链路涉及的 consumer 与旧路径如下。
   - Consumer: Canvas iframe/preview 调用 `/canvases/{id}/runtime-invoke`，route 见 `crates/agentdash-api/src/routes/canvases.rs:368`。
   - Gateway provider: `McpListToolsProvider::invoke` 从 Session context 取 `session_id`，见 `crates/agentdash-application/src/runtime_gateway/session_actions.rs:130`，再调用 `self.access.list_mcp_tools(&session_id)`，见 `crates/agentdash-application/src/runtime_gateway/session_actions.rs:137`。
   - Bootstrap: `crates/agentdash-api/src/app_state.rs:232` 将 `SessionCapabilityService` clone 成 `Arc<dyn RuntimeSessionMcpAccess>`；`crates/agentdash-api/src/bootstrap/runtime_gateway.rs:35` 绑定 `McpListToolsProvider`，`crates/agentdash-api/src/bootstrap/runtime_gateway.rs:38` 绑定 `McpCallToolProvider`。
   - Access impl: `crates/agentdash-application/src/session/capability_service.rs:265` 为 `SessionCapabilityService` 实现 `RuntimeSessionMcpAccess`；`list_mcp_tools` 委托 `hub.discover_runtime_mcp_tool_entries(session_id)`，见 `crates/agentdash-application/src/session/capability_service.rs:270`。
   - Hub discovery: `crates/agentdash-application/src/session/hub/tool_builder.rs:374` 定义 `discover_runtime_mcp_tool_entries`。
   - Active branch: `crates/agentdash-application/src/session/hub/tool_builder.rs:378` 先读 active turn；`crates/agentdash-application/src/session/hub/tool_builder.rs:386` 从 active `turn.session_frame.runtime_backend_anchor` 取 anchor。
   - Idle branch: `crates/agentdash-application/src/session/hub/tool_builder.rs:411` idle 时用 `resolve_current_frame_from_delivery_trace_ref` 取 current AgentFrame；`crates/agentdash-application/src/session/hub/tool_builder.rs:428` 返回 tuple 时 `backend_anchor = None`、`identity = None`、`turn_id = String::new()`。
   - Error source: `crates/agentdash-application/src/session/hub/tool_builder.rs:437` 要求 `backend_anchor.ok_or_else(...)`；`crates/agentdash-application/src/session/hub/tool_builder.rs:440` component 是 `runtime_mcp_tool_discovery`；idle 时 `turn_id` 为空，最终就是 Canvas idle `turn_id=None` missing backend anchor。

5. Extension runtime action/channel 通过 API helper 取 backend target 与 VFS workspace context；provider 本身只执行 target。
   - `crates/agentdash-api/src/routes/extension_runtime.rs:111` `invoke_project_extension_runtime_action`。
   - `crates/agentdash-api/src/routes/extension_runtime.rs:132` 调用 `resolve_session_frame_vfs`。
   - `crates/agentdash-api/src/routes/extension_runtime.rs:133` 要求 backend anchor。
   - `crates/agentdash-api/src/routes/extension_runtime.rs:153` 组装 `RuntimeTarget::Backend { backend_id }`。
   - `crates/agentdash-api/src/routes/extension_runtime.rs:156` 附加 workspace metadata。
   - `crates/agentdash-api/src/routes/extension_runtime.rs:163` `invoke_project_extension_runtime_channel`。
   - `crates/agentdash-api/src/routes/extension_runtime.rs:184` channel 同样调用 `resolve_session_frame_vfs`。
   - `crates/agentdash-api/src/routes/extension_runtime.rs:304` `require_extension_runtime_backend_anchor`，missing component 是 `extension_runtime`。
   - `crates/agentdash-api/src/routes/extension_runtime.rs:328` `select_extension_invocation_workspace` 用 anchor root/default mount 从 VFS 选 workspace。
   - `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:123` provider 从 request 提取 session/project。
   - `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:124` provider 要求 backend target。
   - `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:255` `backend_target` 只接受 `RuntimeTarget::Backend`。

6. VFS surface `SessionRuntime` source 只消费 VFS，旧 helper 内隐含 backend anchor 旧依赖。
   - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:220` 匹配 `ResolvedVfsSurfaceSource::SessionRuntime { session_id }`。
   - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:221` 先校验 session permission。
   - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:222` 调用 `resolve_session_frame_vfs(...).vfs.unwrap_or_default()`。
   - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:227` `ResolvedVfsSurfaceSource::AgentRun` 走另一路 `resolve_agent_run_frame_vfs`。
   - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:285` `resolve_agent_run_frame_vfs_for_agent`。
   - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:290` 通过 `execution_anchor_repo.list_by_run` 选 anchor。
   - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:300` 有 anchor 时也调用 `resolve_current_frame_from_delivery_trace_ref`。
   - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:336` 用 `AgentRunLifecycleSurfaceProjector` 叠 lifecycle evidence surface。

7. Terminal spawn 是调查范围之外但同类旧 helper consumer，应纳入迁移排查。
   - `crates/agentdash-api/src/routes/terminals.rs:258` `resolve_terminal_launch_target`。
   - `crates/agentdash-api/src/routes/terminals.rs:270` 调用 `resolve_session_frame_vfs`。
   - `crates/agentdash-api/src/routes/terminals.rs:271` 要求 `result.runtime_backend_anchor`，missing component 是 `terminal_spawn`。
   - `crates/agentdash-api/src/routes/terminals.rs:281` 要求 current frame VFS。

8. WorkspaceModule visibility 通过 `SessionCapabilityService` 间接解析 runtime session target/current frame。
   - `crates/agentdash-application/src/workspace_module/tools.rs:45` `WorkspaceModuleVisibilitySource`。
   - `crates/agentdash-application/src/workspace_module/tools.rs:70` `effective_view`。
   - `crates/agentdash-application/src/workspace_module/tools.rs:90` 通过 session services 取 `capability`。
   - `crates/agentdash-application/src/workspace_module/tools.rs:92` 调用 `effective_capability_view_for_runtime_session(session_id)`。
   - `crates/agentdash-application/src/session/capability_service.rs:161` 该方法先 `resolve_runtime_session_target(session_id)`。
   - `crates/agentdash-application/src/session/capability_service.rs:171` 再通过 frame repo 读取 frame。
   - `crates/agentdash-application/src/session/capability_service.rs:176` 调用 `AgentRunEffectiveCapabilityService::effective_view_from_frame`。

9. WorkspaceModule Canvas create/present 是 surface-changing update，不是查询。
   - `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:196` 注入 `workspace_module_create`。
   - `crates/agentdash-application/src/workspace_module/tools.rs:436` create tool 调用 `create_or_attach_canvas_for_session`。
   - `crates/agentdash-application/src/canvas/tools.rs:61` `create_or_attach_canvas_for_session`。
   - `crates/agentdash-application/src/canvas/tools.rs:150` 调用 `expose_canvas_to_session`。
   - `crates/agentdash-application/src/workspace_module/tools.rs:1036` present Canvas 时调用 `expose_existing_canvas_for_session`。
   - `crates/agentdash-application/src/canvas/tools.rs:199` `expose_existing_canvas_for_session`。
   - `crates/agentdash-application/src/canvas/tools.rs:208` 调用 `expose_canvas_to_session`。
   - `crates/agentdash-application/src/canvas/tools.rs:244` `expose_canvas_to_session`。
   - `crates/agentdash-application/src/canvas/tools.rs:260` 通过 `session_services.capability.expose_canvas_mount_revision_and_adopt(session_id, canvas)` 写 frame revision 并 adopt。
   - `crates/agentdash-application/src/session/capability_service.rs:102` `expose_canvas_mount_revision_and_adopt` 的注释说明“先写新的 AgentFrame revision，再采用到 active runtime”。
   - `crates/agentdash-application/src/session/capability_service.rs:136` 用 `AgentFrameBuilder` 构造新 frame。
   - `crates/agentdash-application/src/session/capability_service.rs:150` 随后调用 `adopt_persisted_agent_frame_revision`。

10. WorkspaceModule invoke 工具从 active `ExecutionContext` 取 backend anchor，属于 active turn runtime tool assembly。
    - `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:143` `WorkspaceModuleRuntimeToolProvider::build_tools`。
    - `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:162` 从 `ExecutionContext` 构造 shared VFS。
    - `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:268` 调用 `context.session.require_runtime_backend_anchor("workspace_module_invoke", Some(session_id))`。
    - `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:285` `resolve_invocation_backend(context.session.vfs.as_ref(), Some(backend_anchor))`。
    - `crates/agentdash-application/src/workspace_module/mod.rs:60` `resolve_invocation_backend` 只从 VFS + anchor 选 backend/workspace。

11. WorkspaceModule HTTP present 不直接改 surface。
    - `crates/agentdash-api/src/routes/workspace_module.rs:78` 注释明确运行时能力变更由 Agent turn 内 `workspace_module_present` 工具路径负责。
    - `crates/agentdash-api/src/routes/workspace_module.rs:117` 只在请求携带 runtime session 时校验 session/project 绑定。
    - `crates/agentdash-api/src/routes/workspace_module.rs:147` `ensure_runtime_session_belongs_to_project` 只读 execution anchor 和 run project。

12. Permission approve/revoke route 直接调用 active runtime adoption primitive。
    - `crates/agentdash-api/src/routes/permission_grants.rs:217` `approve_grant`。
    - `crates/agentdash-api/src/routes/permission_grants.rs:227` 构造 `PermissionGrantService`。
    - `crates/agentdash-api/src/routes/permission_grants.rs:233` route 调用 `adopt_grant_effect`。
    - `crates/agentdash-api/src/routes/permission_grants.rs:258` `revoke_grant`。
    - `crates/agentdash-api/src/routes/permission_grants.rs:274` revoke 后同样调用 `adopt_grant_effect`。
    - `crates/agentdash-api/src/routes/permission_grants.rs:279` `adopt_grant_effect`。
    - `crates/agentdash-api/src/routes/permission_grants.rs:288` 调用 `state.services.session_capability.adopt_persisted_agent_frame_revision(...)`。
    - `crates/agentdash-application/src/permission/service.rs:322` `apply_grant_effect`。
    - `crates/agentdash-application/src/permission/service.rs:332` admission-only grant 直接返回 no frame；surface-changing grant 继续。
    - `crates/agentdash-application/src/permission/service.rs:351` 从 current frame 读取状态。
    - `crates/agentdash-application/src/permission/service.rs:379` 写新的 `effect_frame`。

13. `SessionCapabilityService` 当前是误导性混合 facade。
    - `crates/agentdash-application/src/session/capability_service.rs:39` `get_runtime_mcp_servers`。
    - `crates/agentdash-application/src/session/capability_service.rs:43` `get_current_capability_state`。
    - `crates/agentdash-application/src/session/capability_service.rs:51` `get_current_runtime_backend_anchor`。
    - `crates/agentdash-application/src/session/capability_service.rs:66` `resolve_runtime_session_frame_id`。
    - `crates/agentdash-application/src/session/capability_service.rs:80` `resolve_runtime_session_target`。
    - `crates/agentdash-application/src/session/capability_service.rs:92` `adopt_persisted_agent_frame_revision`。
    - `crates/agentdash-application/src/session/capability_service.rs:103` `expose_canvas_mount_revision_and_adopt`。
    - `crates/agentdash-application/src/session/capability_service.rs:161` `effective_capability_view_for_runtime_session`。
    - `crates/agentdash-application/src/session/capability_service.rs:265` `impl RuntimeSessionMcpAccess for SessionCapabilityService`。

14. Hub adoption primitive requires active runtime/active turn, so it is not a general current surface query/update API.
    - `crates/agentdash-application/src/session/hub/tool_builder.rs:183` `adopt_persisted_agent_frame_revision`。
    - `crates/agentdash-application/src/session/hub/tool_builder.rs:218` 解析 delivery session current frame 并校验 caller 指定 frame 是 current revision。
    - `crates/agentdash-application/src/session/hub/tool_builder.rs:250` 从 runtime registry 取 turn snapshot。
    - `crates/agentdash-application/src/session/hub/tool_builder.rs:258` 若没有 active turn 报错“没有活跃 turn”。
    - `crates/agentdash-application/src/session/hub/tool_builder.rs:295` 调用 connector `update_session_tools`。
    - `crates/agentdash-application/src/session/hub/tool_builder.rs:299` 更新 runtime registry active turn cache。
    - `crates/agentdash-application/src/session/hub/tool_builder.rs:315` 发送 adopted runtime context transition。

15. Active turn live tool refresh 可以保留在 hub。
    - `crates/agentdash-application/src/session/tool_assembly.rs:17` `assemble_tool_surface_for_execution_context`。
    - `crates/agentdash-application/src/session/tool_assembly.rs:39` MCP discovery 基于当前 `ExecutionContext`。
    - `crates/agentdash-application/src/session/tool_assembly.rs:40` 要求 `context.session.require_runtime_backend_anchor("tool_assembly", Some(session_id))`。
    - `crates/agentdash-application/src/session/tool_assembly.rs:55` 构造 `RelayMcpCallContext`，直接使用 `context.session.runtime_backend_anchor`、`vfs`、`identity`。
    - 该路径的输入是 active turn/prepared turn 的 connector-facing snapshot，不应被 current resolver 替代为业务查询。

16. 已有 current frame base helper 可复用，但不足以作为最终 resolver。
    - `crates/agentdash-application/src/lifecycle/session_association.rs:38` `resolve_current_frame_from_delivery_trace_ref`。
    - `crates/agentdash-application/src/lifecycle/session_association.rs:44` `anchor_repo.find_by_session(runtime_session_id)`。
    - `crates/agentdash-application/src/lifecycle/session_association.rs:47` `agent_repo.get(anchor.agent_id)`。
    - `crates/agentdash-application/src/lifecycle/session_association.rs:53` `frame_repo.get_current(agent.id)`。
    - `crates/agentdash-application/src/lifecycle/session_association.rs:59` 返回 `(anchor, agent, frame)`，但没有闭合 VFS/MCP/capability/backend anchor/project/run provenance。

17. backend anchor 派生逻辑已在 frame launch surface 中存在。
    - `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs:32` `FrameRuntimeSurface` 明确只来自 AgentFrame 持久化 surface。
    - `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs:187` `runtime_backend_anchor` 从 VFS default mount 派生 anchor。
    - `crates/agentdash-application/src/agent_run/frame/construction/mod.rs:372` close frame launch surface。
    - `crates/agentdash-application/src/agent_run/frame/construction/mod.rs:385` launch envelope 从 closed surface 派生 `runtime_backend_anchor`。

### Business consumer migration matrix

| Business consumer / function | Current old dependency | Consumed surface kind | Current behavior/risk | Target owner | Migration action |
| --- | --- | --- | --- | --- | --- |
| Canvas runtime invoke: `invoke_canvas_runtime_action` (`canvases.rs:368`) | Indirect: RuntimeGateway MCP providers use `SessionCapabilityService -> hub.discover_runtime_mcp_tool_entries` | MCP, backend anchor, VFS, capability state, active turn id | Route itself is correct; idle `mcp.list_tools` fails downstream because hub idle branch returns no backend anchor. | RuntimeGateway MCP access + current runtime surface resolver | Keep Canvas route actor/context assembly. Replace MCP access implementation with resolver-backed access. |
| Canvas runtime bridge surface: `build_canvas_runtime_bridge_surface` (`canvases.rs:524`) | RuntimeGateway `surface_for_actor` only | Action visibility | Surface can show `mcp.list_tools` while actual MCP discovery is not executable. | RuntimeGateway | Keep. Do not treat bridge surface as tool/runtime surface closure. Tests should distinguish action visibility from MCP tool output. |
| Canvas runtime snapshot: `get_canvas_runtime_snapshot` / `resolve_canvas_runtime_vfs` (`canvases.rs:341`, `canvases.rs:577`) | `resolve_session_frame_vfs` | VFS/resource surface, frame target | Uses current frame VFS, but helper also hides old backend anchor dependency. | Canvas runtime resource adapter over current runtime surface resolver | Replace helper call with resolver VFS/resource surface. Preserve project permission and Canvas project check. |
| Canvas create/attach exposure: `create_or_attach_canvas_for_session` (`canvas/tools.rs:61`) | `expose_canvas_to_session -> session_capability.expose_canvas_mount_revision_and_adopt` | Surface mutation: VFS, capability state, workspace module visibility, active runtime adoption | Business tool writes AgentFrame revision through session capability facade and directly adopts active runtime. | Runtime surface update use case | Move Canvas mount exposure into typed AgentRun surface update use case; active adoption becomes internal primitive. |
| Canvas present exposure: `expose_existing_canvas_for_session` (`canvas/tools.rs:199`) | Same Canvas exposure/adoption helper | Surface mutation: VFS/capability/workspace module visibility/adoption | Presenting Canvas mutates frame and active runtime before emitting presentation. | Runtime surface update use case | Same as create/attach. WorkspaceModule present can call use case, then emit event. |
| RuntimeGateway MCP list: `McpListToolsProvider::invoke` (`session_actions.rs:115`) | `RuntimeSessionMcpAccess` currently implemented by `SessionCapabilityService` | MCP, backend anchor, VFS, capability state, optional active turn/frame target | Provider is clean but access is old hub facade. | RuntimeGateway MCP access | Keep provider trait boundary; inject new resolver-backed access. |
| RuntimeGateway MCP call: `McpCallToolProvider::invoke` (`session_actions.rs:196`) | Same as list | MCP, backend anchor, VFS, capability state | Calls discovery entries first, so same idle backend anchor risk as list. | RuntimeGateway MCP access | Reuse resolver-backed entries and `execute_runtime_mcp_tool`. |
| Extension action: `invoke_project_extension_runtime_action` (`extension_runtime.rs:111`) | `resolve_session_frame_vfs` + active-turn-only anchor inside helper | backend anchor, VFS workspace context, project/run permission | Idle sessions can miss backend anchor even though current frame has VFS. | API adapter over current runtime surface resolver | Query resolver once for closed surface; select workspace from resolver VFS + anchor; keep `ensure_project_backend_access`. |
| Extension channel: `invoke_project_extension_runtime_channel` (`extension_runtime.rs:163`) | Same as action | backend anchor, VFS workspace context | Same risk. | API adapter over current runtime surface resolver | Same as action. |
| Extension provider: `ExtensionRuntimeActionProvider::invoke` (`extension_actions.rs:119`) | Requires prebuilt `RuntimeTarget::Backend` | backend target only | Correctly does not know session surface. | RuntimeGateway provider | Keep. It should not call resolver directly. |
| VFS surface `SessionRuntime`: `resolve_surface_bundle` (`vfs_surfaces/resolver.rs:220`) | `resolve_session_frame_vfs` | VFS/resource surface | Reads current frame VFS, but through mixed API helper and no lifecycle surface closure consistency guarantee. | VFS API adapter over current runtime surface resolver | Replace helper with resolver VFS; decide whether SessionRuntime should include same lifecycle projection as AgentRun source. |
| VFS surface `AgentRun`: `resolve_agent_run_frame_vfs_for_agent` (`vfs_surfaces/resolver.rs:285`) | Own anchor/current frame/lifecycle projection logic | VFS/resource surface + lifecycle evidence | Separate path may diverge from SessionRuntime/Canvas snapshot. | Current runtime/resource surface resolver or shared resource projector facade | Consolidate run/agent current delivery selection and VFS projection with session-id resolver output. |
| Terminal spawn: `resolve_terminal_launch_target` (`terminals.rs:258`) | `resolve_session_frame_vfs` + backend anchor | backend anchor, VFS workspace target | Same active-turn-only anchor risk; outside requested list but same helper dependency. | API adapter over current runtime surface resolver | Include in Phase 3 grep cleanup or explicitly defer. |
| WorkspaceModule list/describe visibility: `WorkspaceModuleVisibilitySource::effective_view` (`tools.rs:70`) | `session_capability.effective_capability_view_for_runtime_session` | capability state, visible module refs, frame target | Reads current frame through session capability facade. | AgentRun effective capability/current runtime surface resolver | Replace with AgentRun effective capability service fed by resolver target/current frame. |
| WorkspaceModule invoke tool assembly: `push_invoke_tool` (`runtime_tool_provider.rs:243`) | Active `ExecutionContext.session.require_runtime_backend_anchor` | active turn backend anchor, VFS workspace context | Legit active turn tool assembly; no idle query. | Session runtime tool assembly / active turn ExecutionContext | Keep; maybe share `resolve_invocation_backend` helper with API, but do not replace with query resolver during active tool construction unless design chooses current-frame-only runtime action semantics. |
| WorkspaceModule invoke execution: `WorkspaceModuleInvokeTool` | Pre-resolved backend/gateway/channel invoker | backend target, extension runtime channel, capability-filtered module view | Uses runtime visibility via session capability facade for module list/describe filtering. | Runtime tool + AgentRun effective capability service | Keep backend target as active ExecutionContext fact; migrate visibility source off `SessionCapabilityService`. |
| WorkspaceModule present tool: `WorkspaceModulePresentTool::execute` (`tools.rs:927`) | `expose_existing_canvas_for_session` for Canvas renderer | surface mutation + presentation event | Directly couples presentation with Canvas frame mutation/adoption helper. | Runtime surface update use case | Move mutation to use case; tool emits event after update succeeds. |
| WorkspaceModule HTTP present: `present_workspace_module` (`workspace_module.rs:79`) | Only `ensure_runtime_session_belongs_to_project` via anchor/run | project binding only | No surface query or mutation. | WorkspaceModule API | Keep; if it later needs runtime-aware presentation, call resolver read-only. |
| Permission approve route: `approve_grant` (`permission_grants.rs:218`) | route-local `adopt_grant_effect` calls session capability adoption | surface-changing grant adoption | Grant service writes frame, but route owns live adoption side effect. | Permission/AgentRun surface update use case | Move adoption orchestration into application service/use case; route calls one service and maps visible error. |
| Permission revoke route: `revoke_grant` (`permission_grants.rs:259`) | Same route-local adoption | surface-changing grant adoption | Same split-brain risk. | Permission/AgentRun surface update use case | Same as approve. |
| Permission service: `apply_grant_effect` (`permission/service.rs:322`) | Writes AgentFrame directly, no live adoption | capability state, frame revision | Correct owner for grant classification/frame write, but not full runtime update. | Permission service + runtime surface update collaborator | Keep grant state machine/frame write; inject/use collaborator for active adoption at application layer. |
| Hub active turn tool refresh: `assemble_tool_surface_for_execution_context` (`tool_assembly.rs:17`) | Active `ExecutionContext` | active VFS/MCP/backend anchor/capability/identity | Legit live runtime coordination path. | session/hub | Keep in hub; it is not a current surface query. |
| Hub runtime action MCP discovery: `discover_runtime_mcp_tool_entries` (`tool_builder.rs:374`) | active turn branch + idle AgentFrame fallback | MCP/backend anchor/VFS/capability/identity | Mixes live active cache and AgentFrame query; idle branch loses backend anchor. | Split: active refresh stays in hub; RuntimeGateway MCP access moves out | Remove/privatize idle action-backing discovery after new MCP access lands. |
| `resolve_session_frame_vfs` API helper (`session_construction.rs:26`) | Current frame VFS + session capability backend helper | VFS + backend anchor + frame | Name says VFS but returns backend anchor and frame; anchor source is stale. | Thin API adapter over current runtime surface resolver | Replace or narrow to `resolve_current_runtime_surface_for_api`; ensure permission check either in adapter or resolver facade. |

### Paths that should query current runtime surface resolver

- RuntimeGateway MCP session action backing access (`mcp.list_tools`, `mcp.call_tool`): must query closed current surface for MCP servers, VFS, backend anchor, capability state, identity/admission context, and optional active turn id.
- Canvas runtime snapshot / binding resource path: query resolver for VFS/resource surface, not `resolve_session_frame_vfs`.
- Extension runtime action/channel API target selection: query resolver for backend anchor + VFS workspace context.
- VFS surface `SessionRuntime` source: query resolver for VFS and provenance; align with AgentRun resource surface policy.
- Terminal spawn/session VFS target: same current surface query if kept in this task.
- WorkspaceModule visibility read (`effective_capability_view_for_runtime_session`): query resolver/current frame or AgentRun effective capability service fed by resolver output.
- Any API route that currently wants “current backend anchor for runtime session” must use resolver, not `get_current_runtime_backend_anchor(session_id)`.

### Paths that should use runtime surface update/use case, not resolver query

- Canvas create/attach exposure (`create_or_attach_canvas_for_session`): creates or attaches Canvas, appends Canvas mount, writes AgentFrame revision, adopts active runtime.
- Canvas present exposure (`expose_existing_canvas_for_session`): makes existing Canvas visible in runtime surface before presentation.
- WorkspaceModule present for Canvas renderer: should call a typed Canvas/WorkspaceModule runtime surface update use case before emitting `workspace_module_presented`.
- Permission approve/revoke for surface-changing grants: should remain a business state transition plus AgentFrame revision write, with live adoption inside application use case.
- Direct `adopt_persisted_agent_frame_revision`: should become private/internal primitive for update use cases; consumers should pass typed update intent, not manually choose frame/session adoption.

### Helper and service naming cleanup suggestions

| Existing helper/service | Why misleading | Suggested action |
| --- | --- | --- |
| `SessionCapabilityService` | Name implies capability projection, but it exposes runtime backend anchor, runtime session target resolution, Canvas frame mutation/adoption, RuntimeGateway MCP access, runtime commands, and effective capability view. | Split. Keep a narrow capability/projection service if needed; move MCP access, current surface query, and surface update/adoption to dedicated services. |
| `SessionCapabilityService impl RuntimeSessionMcpAccess` | RuntimeGateway MCP action backing is not a capability-service responsibility and currently routes through hub. | Delete trait impl after adding resolver-backed access. |
| `get_current_runtime_backend_anchor(session_id)` | “current” sounds AgentRun current surface, but implementation reads only active turn cache. | Remove from public/API-facing services or rename to `get_active_turn_runtime_backend_anchor` and keep hub-internal. |
| `resolve_session_frame_vfs` | Name says VFS but returns `runtime_backend_anchor` and `frame`, performs permission checks, and mixes current frame lookup with active-turn anchor helper. | Replace with `resolve_current_runtime_surface_for_api` thin adapter or delete after route migration. |
| `SessionFrameVfsResult` | Carries VFS, backend anchor, and frame; not only VFS. | Replace with `CurrentRuntimeSurface`/`ApiCurrentRuntimeSurface`. |
| `discover_runtime_mcp_tool_entries` in hub | Used as RuntimeGateway MCP action backing access and active/idle surface assembler, not just hub tool builder. | Split: keep active turn refresh helper private; move runtime action discovery to new access. |
| `resolve_runtime_session_frame_id` / `resolve_runtime_session_target` under session capability | Helper actually resolves delivery RuntimeSession to AgentFrame target. | Move to current runtime surface resolver or AgentRun delivery target resolver; name `resolve_current_runtime_target` if kept. |
| `adopt_persisted_agent_frame_revision` exposed via service | Sounds like a general business API but requires active runtime and active turn. | Make private/internal to runtime surface update use case; name `adopt_persisted_frame_revision_into_active_turn` if retained. |
| `expose_canvas_mount_revision_and_adopt` under session capability | Canvas business mutation lives under session capability and directly calls adoption. | Move to Canvas/WorkspaceModule runtime surface update use case, e.g. `CanvasRuntimeSurfaceUpdateService::expose_canvas_mount`. |
| `effective_capability_view_for_runtime_session` under session capability | It resolves runtime session target and reads AgentRun effective capability; this belongs to AgentRun effective capability/current surface boundary. | Move to `AgentRunEffectiveCapabilityService` or current surface resolver facade. |
| Route-local `adopt_grant_effect` | API route owns active runtime adoption after Permission service. | Delete after Permission service/use case owns apply/revoke + live adoption. |

### Test suggestions

1. Application resolver tests for new current runtime surface:
   - `runtime_session_id` without `RuntimeSessionExecutionAnchor` returns typed unavailable error.
   - Anchor/run/agent mismatch returns typed unavailable/conflict.
   - Current AgentFrame typed VFS/MCP/capability are all read from the same frame revision.
   - VFS default mount backend id produces `RuntimeBackendAnchor`; workspace metadata produces `RuntimeBackendAnchorSource::WorkspaceBinding`.
   - Missing VFS/default backend returns typed missing backend anchor error with purpose label.

2. RuntimeGateway MCP access tests:
   - `mcp.list_tools` with no active turn but with current frame VFS/backend/MCP/capability reaches `McpToolDiscoveryRequest` with non-empty `backend_anchor`.
   - `mcp.list_tools` still filters by `server_names`.
   - `mcp.call_tool` supports both `runtime_name` and `server_name + tool_name` matching after resolver migration.
   - Capability-disabled MCP tools stay hidden.
   - Static/grep assertion: `SessionCapabilityService` no longer implements `RuntimeSessionMcpAccess`.

3. Canvas regression:
   - Route-level or integration test: Canvas `/runtime-invoke` with `action_key="mcp.list_tools"` on idle session succeeds and does not produce `component=runtime_mcp_tool_discovery` missing backend anchor.
   - Canvas runtime snapshot with `session_id` uses same current frame VFS as VFS `SessionRuntime` surface.
   - Runtime bridge surface test remains action visibility only; add separate test for MCP tool output.

4. Extension runtime tests:
   - `invoke_project_extension_runtime_action` and `invoke_project_extension_runtime_channel` use resolver backend anchor and VFS workspace selection.
   - Existing workspace selection cases (`root_ref` match, default mount fallback, no root) should move from route-private helper toward shared application helper if resolver returns workspace context.
   - Missing backend anchor from resolver maps to `Conflict` with component/purpose `extension_runtime`.

5. VFS surface tests:
   - `ResolvedVfsSurfaceSource::SessionRuntime` and Canvas runtime snapshot observe the same current frame VFS/default mount.
   - `ResolvedVfsSurfaceSource::AgentRun` current delivery path and `SessionRuntime` session-id path do not diverge for the same delivery runtime; explicitly test lifecycle mount policy if AgentRun source should include lifecycle evidence.

6. Permission/update path tests:
   - Approve/revoke surface-changing grant writes AgentFrame revision and invokes active runtime adoption inside application use case, not API route.
   - Admission-only grant does not write frame or invoke adoption.
   - Adoption failure remains a visible API error.
   - Static/grep assertion: API routes and business tools no longer call `adopt_persisted_agent_frame_revision` directly.

7. WorkspaceModule update path tests:
   - `workspace_module_create` and `workspace_module_present` call runtime surface update use case and keep existing behavior: hook runtime target aligns to produced AgentFrame revision, VFS refresh happens, presentation event follows capability update.
   - Visibility list/describe consumes AgentRun effective capability view through new resolver/effective service, not `SessionCapabilityService`.
   - `workspace_module_invoke` active turn assembly continues to consume active `ExecutionContext.session.runtime_backend_anchor`.

## Caveats / Not Found

- No external docs were needed.
- I did not run tests; this is a read-only research task.
- Current implementation already has `resolve_current_frame_from_delivery_trace_ref`, but no single closed `CurrentRuntimeSurface` object was found that returns VFS, MCP servers, capability state, backend anchor, identity/admission context, and provenance together.
- `identity` for idle MCP discovery is currently `None` in hub idle fallback. The new resolver design must decide whether identity comes from current frame/launch envelope, active turn transient metadata, or remains optional in idle paths.
- `ResolvedVfsSurfaceSource::AgentRun` currently applies lifecycle evidence projection while `SessionRuntime` source only returns frame VFS. The migration must explicitly decide whether this difference is intended or should be unified through a resource-surface resolver facade.
- `Terminal` is outside the requested business path list but uses the same `resolve_session_frame_vfs` + backend anchor pattern; leaving it untouched would preserve one old API consumer.
- Active turn semantics remain an open design decision: query paths can use current committed AgentFrame as fact source plus active turn transient metadata, while connector live refresh should remain on active `ExecutionContext`.
