# Research: AgentFrame exposure inventory

- Query: 清点当前代码库中 `AgentFrame` / `agent_run::frame` 的实际暴露面，区分内聚实现内部引用与跨模块/API/business consumer 的裸引用，并为 `session-hub-boundary-cleanup` 规划补迁移矩阵。
- Scope: internal
- Date: 2026-06-23

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `.trellis/workflow.md` | Trellis planning/research workflow；研究结果必须落到 task `research/`。 |
| `.trellis/spec/project-overview.md` | 定义 `LifecycleAgent / AgentFrame / RuntimeSessionExecutionAnchor` 核心边界。 |
| `.trellis/spec/backend/session/architecture.md` | Session 只拥有 runtime trace/delivery；`AgentFrame` 是 capability/context/VFS/MCP 事实源。 |
| `.trellis/spec/backend/session/runtime-execution-state.md` | 规定 runtime registry / active turn / connector live session 的边界，强调 AgentRun surface 来自 AgentFrame/AgentRun 控制面闭包。 |
| `.trellis/spec/backend/session/execution-context-frames.md` | 说明 `ExecutionContext` 是 connector-facing projection，active turn snapshot 不应成为 application 事实源。 |
| `.trellis/spec/backend/runtime-gateway.md` | 现有规范仍写着 MCP provider 通过 `RuntimeSessionMcpAccess` 进入 SessionHub，是本任务要更新的关键旧约束。 |
| `.trellis/spec/backend/capability/architecture.md` | AgentRun effective capability/admission 是 runtime 能力读取入口；surface-changing grant 写 `AgentFrame` revision。 |
| `.trellis/spec/backend/permission/architecture.md` | Permission Grant effect 落到 `AgentFrame` revision 或 run/agent control scope；active-runtime adoption 应可诊断。 |
| `.trellis/spec/backend/vfs/architecture.md` | AgentRun resource surface 应由应用层 resolver 输出闭包 surface。 |
| `.trellis/spec/backend/workflow/architecture.md` | `AgentFrame` 是 runtime surface revision，读取 workspace/VFS 应从 anchor 进 current frame 后消费 typed surface。 |
| `.trellis/tasks/06-23-session-hub-boundary-cleanup/prd.md` | 本任务要求 `AgentFrame` 不出现在 RuntimeGateway/API/Canvas/Extension/VFS consumer 公开 contract 中。 |
| `.trellis/tasks/06-23-session-hub-boundary-cleanup/design.md` | 已有迁移矩阵草案，本研究补充 AgentFrame 裸引用维度。 |
| `.trellis/tasks/06-23-session-hub-boundary-cleanup/implement.md` | Phase 0/1/2/3/4 需要补入本清单里的迁移检查项。 |
| `crates/agentdash-domain/src/workflow/agent_frame.rs` | `AgentFrame` domain entity 定义。 |
| `crates/agentdash-domain/src/workflow/repository.rs` | `AgentFrameRepository` trait 定义。 |
| `crates/agentdash-domain/src/workflow/mod.rs` | domain workflow facade 公开 re-export `AgentFrame` / repository。 |
| `crates/agentdash-application/src/agent_run/mod.rs` | application `agent_run` facade 公开 `frame` module 与 frame helpers。 |
| `crates/agentdash-application/src/agent_run/frame/*` | Frame builder、typed surface、launch envelope、hook runtime、construction 主实现。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` | Postgres `AgentFrameRepository` 实现。 |
| `crates/agentdash-api/src/session_construction.rs` | API helper 直接解析 runtime session -> current `AgentFrame` -> typed VFS，并返回 frame。 |
| `crates/agentdash-api/src/routes/canvases.rs` | Canvas runtime snapshot 通过 `resolve_session_frame_vfs` 消费 frame VFS。 |
| `crates/agentdash-api/src/routes/extension_runtime.rs` | Extension runtime action/channel 通过 `resolve_session_frame_vfs` 获取 VFS/backend target。 |
| `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs` | `SessionRuntime` VFS source 和 AgentRun VFS resolver 直接 import frame surface helper。 |
| `crates/agentdash-api/src/routes/terminals.rs` | Terminal spawn 通过 `resolve_session_frame_vfs` 获取 VFS/backend anchor。 |
| `crates/agentdash-api/src/routes/permission_grants.rs` | API route 直接持有 `AgentFrame` / `AgentFrameRuntimeTarget` 并触发 active runtime adoption。 |
| `crates/agentdash-api/src/routes/lifecycle_views.rs` | Lifecycle debug/read view 直接读取 `AgentFrame` 并输出 `AgentFrameRuntimeView`。 |
| `crates/agentdash-api/src/app_state.rs` | AppState 将 `SessionCapabilityService` clone 为 `RuntimeSessionMcpAccess` 注入 RuntimeGateway。 |
| `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` | RuntimeGateway provider 只依赖 `RuntimeSessionMcpAccess`，本身不 import `AgentFrame`。 |
| `crates/agentdash-application/src/runtime_gateway/session_actions.rs` | `RuntimeSessionMcpAccess` trait 与 MCP providers；provider 边界可保持。 |
| `crates/agentdash-application/src/session/capability_service.rs` | 混合 facade：MCP access、backend anchor、runtime target resolve、Canvas exposure、adoption、effective capability。 |
| `crates/agentdash-application/src/session/hub/tool_builder.rs` | hub idle MCP discovery / backend anchor / adoption primitive 的核心裸 frame 入口。 |
| `crates/agentdash-application/src/session/types.rs` | `AgentFrameRuntimeTarget` 定义在 session 模块，当前被多个业务模块使用。 |
| `crates/agentdash-application/src/canvas/tools.rs` | Canvas tool 调用 `expose_canvas_mount_revision_and_adopt`，业务 mutation 走 session capability facade。 |
| `crates/agentdash-application/src/workspace_module/tools.rs` | WorkspaceModule Canvas create/present 测试和路径使用 frame builder/target/adoption。 |
| `crates/agentdash-application/src/workspace_module/visibility.rs` | WorkspaceModule visibility 测试/路径使用 `AgentFrameRuntimeTarget`。 |
| `crates/agentdash-application/src/permission/service.rs` | Permission service 写 surface-changing grant 的新 `AgentFrame` revision。 |
| `crates/agentdash-application/src/agent_run/effective_capability.rs` | AgentRun effective capability 使用 session 模块的 `AgentFrameRuntimeTarget` 作为输出 target。 |
| `crates/agentdash-application/src/lifecycle/session_association.rs` | `resolve_current_frame_from_delivery_trace_ref` 是 runtime session -> anchor -> current frame 的通用 helper。 |
| `crates/agentdash-contracts/src/runtime/workflow.rs` | `AgentFrameRefDto` / `AgentFrameRuntimeView` 等 API DTO 暴露 frame refs/read view。 |

### Type Definitions, Pub / Re-export, Repository / Service API

- `AgentFrame` 是 domain workflow entity，字段包括 `id`、`agent_id`、`revision`、`effective_capability_json`、`context_slice_json`、`vfs_surface_json`、`mcp_surface_json`、`visible_workspace_module_refs_json` 等；定义见 `crates/agentdash-domain/src/workflow/agent_frame.rs:10`。
- `AgentFrameRepository` 是 public domain repository trait，直接返回 / 接收 `AgentFrame`：`create(&AgentFrame)`、`get(Uuid) -> Option<AgentFrame>`、`get_current(agent_id) -> Option<AgentFrame>`、`list_by_agent(agent_id) -> Vec<AgentFrame>`，见 `crates/agentdash-domain/src/workflow/repository.rs:82`。
- `agentdash_domain::workflow` facade 公开 re-export `AgentFrame`，见 `crates/agentdash-domain/src/workflow/mod.rs:14`。这会让 API/application 任意模块很容易绕过 use case 直接拿 domain entity。
- Infrastructure 暴露 `PostgresAgentFrameRepository`：crate 根 re-export 在 `crates/agentdash-infrastructure/src/lib.rs:16`；Postgres impl 定义/转换/trait impl 在 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:340`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:376`、`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:419`。这是 repository adapter 暴露，属于可接受持久化边界。
- `agent_run` application facade 公开 `pub mod frame`，并 re-export `AgentFrameBuilder`、`AgentFrameHookRuntime`、`AgentFrameSurfaceExt`、`FrameLaunchEnvelope`、`FrameRuntimeSurface`、`FrameSurfaceDraft`，见 `crates/agentdash-application/src/agent_run/mod.rs:5` 和 `crates/agentdash-application/src/agent_run/mod.rs:36`。这就是 `agent_run::frame` 的主要跨模块暴露面。
- `AgentFrameSurfaceExt` 是 typed JSON surface read helper，定义/impl 在 `crates/agentdash-application/src/agent_run/frame/surface.rs:71` 和 `crates/agentdash-application/src/agent_run/frame/surface.rs:85`。它的用途是避免每个 consumer 手写 JSON parse，但目前已被 API route 直接使用，形成 query facade 缺失的症状。
- `AgentFrameBuilder` 是新 revision 构建器，定义在 `crates/agentdash-application/src/agent_run/frame/builder.rs:80`，`build()` 依赖 `AgentFrameRepository` 并返回 `AgentFrame`，见 `crates/agentdash-application/src/agent_run/frame/builder.rs:225`。在 frame construction / lifecycle dispatch / permission update use case 内是合理内部实现。
- `FrameLaunchSurface` / `FrameLaunchEnvelope` 分别定义在 `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs:93` 和 `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs:230`，是 launch-ready closure；session launch pipeline 使用它们合理，但 current runtime surface query 不应直接暴露 launch-only 语义。
- `AgentFrameRuntimeTarget` 当前定义在 session 类型里，字段是 `frame_id` + `delivery_runtime_session_id`，见 `crates/agentdash-application/src/session/types.rs:67`。它被 AgentRun capability、hook runtime、workspace module、permission adoption 使用，说明“frame target”语义被放在 session 模块且扩散到了业务模块。

### API Crate Exposure

API crate 确认直接 import/use `AgentFrame` 或 `agent_run::frame` 的位置：

- `crates/agentdash-api/src/session_construction.rs:3` import `agentdash_application::agent_run::frame::surface::AgentFrameSurfaceExt`，`crates/agentdash-api/src/session_construction.rs:5` import `agentdash_domain::workflow::AgentFrame`。`SessionFrameVfsResult` 直接包含 `pub frame: AgentFrame`，见 `crates/agentdash-api/src/session_construction.rs:16` 和 `crates/agentdash-api/src/session_construction.rs:20`。这是最明确的 API helper 层裸暴露。
- `resolve_session_frame_vfs` 直接走 `resolve_current_frame_from_delivery_trace_ref` 查当前 frame，见 `crates/agentdash-api/src/session_construction.rs:26` 和 `crates/agentdash-api/src/session_construction.rs:37`；随后 backend anchor 又回到 `session_capability.get_current_runtime_backend_anchor(session_id)`，见 `crates/agentdash-api/src/session_construction.rs:70` 和 `crates/agentdash-api/src/session_construction.rs:73`。这是 current surface query 拆散在 API + session hub 的典型旧路径。
- Canvas snapshot 通过 API helper 消费 frame VFS：`crates/agentdash-api/src/routes/canvases.rs:43` import helper，`crates/agentdash-api/src/routes/canvases.rs:586` 调用并取 `.vfs`。Canvas runtime invoke 本身未直接 import `AgentFrame`，问题在 downstream MCP access。
- Extension runtime action/channel 通过 helper 获取 `frame_result`，见 `crates/agentdash-api/src/routes/extension_runtime.rs:18`、`crates/agentdash-api/src/routes/extension_runtime.rs:132`、`crates/agentdash-api/src/routes/extension_runtime.rs:184`。这是 backend target + workspace context current surface consumer。
- VFS surface resolver 直接 import `AgentFrameSurfaceExt` 和 `resolve_current_frame_from_delivery_trace_ref`，见 `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:3` 和 `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:10`；`SessionRuntime` source 走 `resolve_session_frame_vfs`，见 `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:220` 和 `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:222`；AgentRun source 也自行查 current frame，见 `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:300`。需要收束到 application query facade。
- Terminal spawn 使用 `resolve_session_frame_vfs`，见 `crates/agentdash-api/src/routes/terminals.rs:16` 和 `crates/agentdash-api/src/routes/terminals.rs:270`；缺 VFS 报错文案直接说 “AgentFrame 未记录 VFS”，见 `crates/agentdash-api/src/routes/terminals.rs:284`。这是本任务应加入迁移矩阵的额外 API consumer。
- Permission grants route 直接 import `AgentFrameRuntimeTarget` 与 `AgentFrame`，见 `crates/agentdash-api/src/routes/permission_grants.rs:9` 和 `crates/agentdash-api/src/routes/permission_grants.rs:18`；`adopt_grant_effect` 接收 `Option<&AgentFrame>`，见 `crates/agentdash-api/src/routes/permission_grants.rs:279` 和 `crates/agentdash-api/src/routes/permission_grants.rs:282`；route 直接调用 `session_capability.adopt_persisted_agent_frame_revision(AgentFrameRuntimeTarget { ... })`，见 `crates/agentdash-api/src/routes/permission_grants.rs:288` 和 `crates/agentdash-api/src/routes/permission_grants.rs:291`。这是业务 mutation/adoption 泄漏到 API route。
- Lifecycle read/debug view 直接 import `AgentFrameSurfaceExt` 和 `AgentFrame`，见 `crates/agentdash-api/src/routes/lifecycle_views.rs:10` 和 `crates/agentdash-api/src/routes/lifecycle_views.rs:25`；`get_agent_frame_runtime` 以 `frame_id` 加载 frame 并输出 `AgentFrameRuntimeView`，见 `crates/agentdash-api/src/routes/lifecycle_views.rs:97` 和 `crates/agentdash-api/src/routes/lifecycle_views.rs:101`；`agent_frame_runtime_to_view(frame: &AgentFrame)` 在 `crates/agentdash-api/src/routes/lifecycle_views.rs:336`。这是 lifecycle inspection/read model 暴露，不是 RuntimeGateway/current surface consumer，但若作为 runtime surface 公共入口继续扩张，应迁到 Lifecycle/AgentRun read facade。
- API bootstrap 中 `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:11` import frame construction，`crates/agentdash-api/src/bootstrap/session_construction_provider.rs:14` import `FrameLaunchEnvelope`。这是 session launch provider 装配，属于 launch pipeline adapter，暂可接受。
- RuntimeGateway API bootstrap 自身不直接 import `AgentFrame`；它接收 `Arc<dyn RuntimeSessionMcpAccess>` 并注入 `McpListToolsProvider` / `McpCallToolProvider`，见 `crates/agentdash-api/src/bootstrap/runtime_gateway.rs:16`、`crates/agentdash-api/src/bootstrap/runtime_gateway.rs:35`、`crates/agentdash-api/src/bootstrap/runtime_gateway.rs:38`。实际错误归属来自 AppState 把 `SessionCapabilityService` 当成该 access。
- AppState 当前把 `SessionCapabilityService` clone 成 `Arc<dyn RuntimeSessionMcpAccess>`，见 `crates/agentdash-api/src/app_state.rs:232` 和 `crates/agentdash-api/src/app_state.rs:233`。这是 RuntimeGateway MCP backing access 的 DI 泄漏点。

### Application Exposure Outside `agent_run/frame`

需要区分三类：可接受内部实现、应迁到 query facade、应迁到 update use case。

#### 可接受为内部实现或控制面读写

- `agent_run/frame/*` 是 frame construction / typed surface / launch closure 的内聚实现；`AgentFrameBuilder`、`AgentFrameSurfaceExt`、`FrameLaunchEnvelope` 在这里出现是预期。
- `lifecycle/session_association.rs` 的 `resolve_current_frame_from_delivery_trace_ref` 是当前通用 helper，入口定义在 `crates/agentdash-application/src/lifecycle/session_association.rs:38`。它实现 `runtime_session_id -> anchor -> agent -> current frame`，适合作为新 query facade 的内部 helper，而不适合被 API/hub/consumer 直接散用。
- Lifecycle dispatch/materialization 创建 `LifecycleAgent` / `AgentFrame` / `RuntimeSessionExecutionAnchor`，例如 dispatch service 使用 `AgentFrameRepository` 与 `AgentFrameBuilder`，见 `crates/agentdash-application/src/lifecycle/dispatch_service.rs:18`、`crates/agentdash-application/src/lifecycle/dispatch_service.rs:24`、`crates/agentdash-application/src/lifecycle/dispatch_service.rs:397`。这是 control-plane identity materialization，可接受。
- Workflow orchestration launchers使用 `AgentFrameRepository` / `AgentFrameBuilder` 创建 agent node runtime identity，例如 `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:4`、`crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:13`。这是 workflow runtime materialization，可接受。
- Session launch / assembly pipeline 使用 `FrameLaunchEnvelope`、`FrameSurfaceDraft`、`AgentFrameBuilder`，例如 `crates/agentdash-application/src/session/assembly_builder.rs:401`、`crates/agentdash-application/src/session/launch/plan.rs:15`、`crates/agentdash-application/src/session/launch/preparation.rs:3`。按 spec，session launch 消费 frame construction 产物进入 connector projection，暂可接受。
- `session/capability_state.rs` 的 `project_capability_state_from_frame(frame: &AgentFrame)` 是 frame surface projection utility，见 `crates/agentdash-application/src/session/capability_state.rs:50`。可以作为新 query facade / update use case 内部 helper保留，但不应让 hub/API consumer直接调用它来拼 current surface。
- `permission/service.rs` 写 surface-changing grant 的新 `AgentFrame` revision：import `AgentFrameBuilder` / `AgentFrameRepository` 在 `crates/agentdash-application/src/permission/service.rs:11` 和 `crates/agentdash-application/src/permission/service.rs:20`；构建新 revision 在 `crates/agentdash-application/src/permission/service.rs:362`。这是 permission update use case 内部写事实，方向正确；问题是 API route随后直接 adopt。
- Hooks provider / active workflow snapshot 依赖 `AgentFrameRepository` 来构造 hook snapshot 或 hook runtime，见 `crates/agentdash-application/src/hooks/provider.rs:7` 和 `crates/agentdash-application/src/hooks/active_workflow_snapshot.rs:4`。按 spec hook runtime target 是 control target，属于 hook subsystem 内部投影，可保留但应消费明确 target。
- Infrastructure / repository_set / test_support 对 `AgentFrameRepository` 的暴露是装配、测试、持久化边界，暂不属于本次业务/API consumer 泄漏。

#### 应迁到 AgentRun/Lifecycle runtime surface query

- `SessionCapabilityService::get_current_runtime_backend_anchor` 只是转发 hub helper，见 `crates/agentdash-application/src/session/capability_service.rs:51` 和 `crates/agentdash-application/src/session/capability_service.rs:56`。hub helper 只读 active turn cache 的 backend anchor，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:111` 和 `crates/agentdash-application/src/session/hub/tool_builder.rs:120`。API/current surface consumer 不应使用它。
- `SessionRuntimeInner::discover_runtime_mcp_tool_entries` 是 RuntimeGateway MCP backing discovery 的实际旧入口，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:374`。active 分支读 active turn session frame，idle 分支在 `crates/agentdash-application/src/session/hub/tool_builder.rs:411` 反查 current frame，但最终 `backend_anchor = None`、`identity = None`、`turn_id = String::new()` 的 tuple 导致 `runtime_mcp_tool_discovery` missing anchor，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:395`、`crates/agentdash-application/src/session/hub/tool_builder.rs:437`、`crates/agentdash-application/src/session/hub/tool_builder.rs:455`。这应整体迁出 hub，改由 MCP access 消费 closed runtime surface query。
- `SessionCapabilityService impl RuntimeSessionMcpAccess` 位于 `crates/agentdash-application/src/session/capability_service.rs:265`，`list_mcp_tools` 和 `call_mcp_tool` 都委托 `hub.discover_runtime_mcp_tool_entries`，见 `crates/agentdash-application/src/session/capability_service.rs:270`、`crates/agentdash-application/src/session/capability_service.rs:272`、`crates/agentdash-application/src/session/capability_service.rs:293`、`crates/agentdash-application/src/session/capability_service.rs:295`。应替换为 `CurrentSurfaceRuntimeMcpAccess` 或等价新类型，依赖 AgentRun runtime surface query port。
- `resolve_session_frame_vfs` 及其 API consumers（Canvas snapshot、Extension runtime、VFS `SessionRuntime`、Terminal spawn）都属于“current runtime surface query”。它们应迁到 `runtime_session_id -> AgentRun/Lifecycle runtime address -> closed surface` facade，返回 VFS/backend anchor/provenance，而不是 `AgentFrame`。
- `vfs_surfaces/resolver.rs` 中 AgentRun source 也自行调用 `resolve_current_frame_from_delivery_trace_ref` 并读 typed VFS，见 `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:300`。虽然它不走 SessionRuntime helper，但仍是 API route 内 current frame logic，应迁到 application AgentRun resource/runtime surface resolver。
- `agent_run/effective_capability.rs` 输出 `AgentFrameRuntimeTarget`，见 `crates/agentdash-application/src/agent_run/effective_capability.rs:143`。语义上是 AgentRun capability view 可接受，但 type 来自 session 模块，建议在本任务或后续把 target 改成 AgentRun/Lifecycle runtime address + surface revision ref，避免 `session::AgentFrameRuntimeTarget` 作为 capability query public contract。

#### 应迁到 runtime surface update / adoption use case

- `SessionCapabilityService::resolve_runtime_session_target`、`adopt_persisted_agent_frame_revision`、`expose_canvas_mount_revision_and_adopt` 集中暴露业务 mutation/adoption helper，见 `crates/agentdash-application/src/session/capability_service.rs:80`、`crates/agentdash-application/src/session/capability_service.rs:92`、`crates/agentdash-application/src/session/capability_service.rs:103`。这些应降为 runtime surface update use case 内部 primitive 或被替换。
- Hub adoption primitive `SessionRuntimeInner::adopt_persisted_agent_frame_revision` 在 `crates/agentdash-application/src/session/hub/tool_builder.rs:183`，内部校验 current frame、同步 active turn cache、connector tools、hook runtime target。这个 primitive 可作为 live runtime coordination 内部能力保留，但业务模块/API 不应直接调用。
- Canvas business tool 调用 `expose_canvas_mount_revision_and_adopt`，见 `crates/agentdash-application/src/canvas/tools.rs:262`。这是 Canvas exposure surface-changing mutation，应迁到明确的 Canvas/runtime surface update use case。
- WorkspaceModule tools 调用同一 helper，见 `crates/agentdash-application/src/workspace_module/tools.rs:682`。WorkspaceModule create/present 不应直接经 session capability facade 写 frame/adopt，应迁到 runtime surface update use case。
- Permission route 直接 adoption 已列在 API Exposure；应把 grant approve/revoke 的 surface-changing adoption 封到 `PermissionGrantService` 或一个 runtime surface update service 内，让 route 只调用 use case 并映射结果。
- `companion/tools.rs` 使用 `AgentFrameRuntimeTarget` 获取 hook runtime，见 `crates/agentdash-application/src/companion/tools.rs:36` 和 `crates/agentdash-application/src/companion/tools.rs:688`。这是工具执行时需要 hook target 的 adapter 场景，可短期保留，但目标类型应来自 hook/runtime control target，而不是让 companion 业务层持有 session-scoped frame target。
- `workspace_module/visibility.rs` / tests 使用 `AgentFrameRuntimeTarget`，见 `crates/agentdash-application/src/workspace_module/visibility.rs:104` 和 `crates/agentdash-application/src/workspace_module/visibility.rs:332`。需要确认是否仅测试；如果 production path 需要 visible module surface，应通过 AgentRun effective capability/current surface query 或 update use case，而不是直接 frame target。

### RuntimeGateway Boundary

- `RuntimeGateway` provider 边界本身是干净的：`RuntimeSessionMcpAccess` trait 只暴露 `list_mcp_tools(session_id)` 和 `call_mcp_tool(session_id, input)`，见 `crates/agentdash-application/src/runtime_gateway/session_actions.rs:66`；providers 只持有 `Arc<dyn RuntimeSessionMcpAccess>`，见 `crates/agentdash-application/src/runtime_gateway/session_actions.rs:79`、`crates/agentdash-application/src/runtime_gateway/session_actions.rs:81`、`crates/agentdash-application/src/runtime_gateway/session_actions.rs:160`、`crates/agentdash-application/src/runtime_gateway/session_actions.rs:162`。
- 因此本任务不需要重做 RuntimeGateway action/provider；只需把 trait implementation 从 `SessionCapabilityService -> SessionRuntimeInner` 切到 `AgentRunRuntimeSurfaceQueryPort -> MCP discovery`。
- AppState DI 是切换点：当前 `Arc::new(session_capability.clone())` 注入 RuntimeGateway，见 `crates/agentdash-api/src/app_state.rs:232`。迁移后这里应构造新 access，`SessionCapabilityService` 不再实现 `RuntimeSessionMcpAccess`。

### Contracts / DTO Exposure

- `AgentFrameRefDto` / `AgentFrameRuntimeView` 是 contracts 层显式 DTO，定义在 `crates/agentdash-contracts/src/runtime/workflow.rs:799` 和 `crates/agentdash-contracts/src/runtime/workflow.rs:1324`。这不是 Rust domain entity 裸传递，而是 read-model ref/view 暴露；保留用于 lifecycle/debug/read UI 是可接受的。
- `AgentRunResourceSurfaceCoordinateView.surface_frame_ref` 见 `crates/agentdash-contracts/src/runtime/workflow.rs:923`，用于说明 resource surface provenance，和本任务要求的 surface provenance 不冲突。
- Project agent / mailbox / lifecycle routes 组装 `AgentFrameRefDto`，例如 `crates/agentdash-api/src/routes/project_agents.rs:256`、`crates/agentdash-api/src/routes/agent_run_mailbox_contracts.rs:51`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:1070`。这些是接受结果/ref DTO，不等同于 current runtime surface query consumer。
- Caveat: 如果 `AgentFrameRuntimeView` 被前端当作 Canvas/Extension/VFS/MCP current surface API 使用，应纳入迁移；本次 rg 未发现 RuntimeGateway/Canvas 直接消费该 endpoint。

### Internal vs Migration Classification

| Exposure | Current examples | Classification | Target |
| --- | --- | --- | --- |
| Domain entity/repository | `AgentFrame`, `AgentFrameRepository` | Acceptable domain/persistence API | Keep; do not expose to API consumer as query result. |
| Infrastructure repository adapter | `PostgresAgentFrameRepository` | Acceptable adapter | Keep. |
| `agent_run/frame` construction/builder/surface | `AgentFrameBuilder`, `FrameLaunchEnvelope`, `AgentFrameSurfaceExt` inside construction/launch | Acceptable internal implementation | Keep under AgentRun/Lifecycle/frame construction. |
| Lifecycle dispatch/materialization | `LifecycleDispatchService`, workflow agent node launcher | Acceptable control-plane creation | Keep; may feed query facade. |
| Session launch / active turn execution context | launch planner/preparation, hot update using active `ExecutionContext` | Acceptable live runtime coordination | Keep in session/hub for active connector refresh only. |
| Hub idle MCP discovery | `SessionRuntimeInner::discover_runtime_mcp_tool_entries` idle branch | Migrate | New RuntimeGateway MCP access over AgentRun runtime surface query. |
| Active-turn-only backend anchor helper | `get_current_runtime_backend_anchor` | Migrate for API/business consumers | Query facade returns backend anchor from closed surface; hub helper private only if active refresh needs it. |
| API `resolve_session_frame_vfs` | Canvas, Extension, VFS, Terminal | Migrate | API adapter over AgentRun/Lifecycle current runtime surface query; no `AgentFrame` result. |
| API route direct frame adoption | permission grants route | Migrate | Permission/runtime surface update use case handles revision + adoption. |
| Canvas/WorkspaceModule exposure/adoption | `expose_canvas_mount_revision_and_adopt` callers | Migrate | Typed runtime surface update use case; hub adoption primitive internal. |
| AgentRun effective capability target type | `AgentFrameRuntimeTarget` in view | Refine | AgentRun runtime address / surface ref type outside session module. |
| Lifecycle debug/read DTO | `AgentFrameRuntimeView`, frame refs | Accept with guard | Keep as read model; do not use as current surface query facade. |

### Suggested Migration Matrix Additions

Add these rows to `design.md` / `implement.md`:

| Old Path | Current Exposure | Target Owner | Action / Test |
| --- | --- | --- | --- |
| `agentdash-application/src/agent_run/mod.rs` re-export of `AgentFrameSurfaceExt` | Makes typed frame parse helper easy to import from API/routes | AgentRun internal or query facade | Stop new API imports; current surface API consumes query DTO. Add grep check for `agentdash-api.*AgentFrameSurfaceExt`. |
| `agentdash-api/src/session_construction.rs::SessionFrameVfsResult` | API helper returns `AgentFrame` and backend anchor from session capability | AgentRun/Lifecycle runtime surface query adapter | Replace with `SessionRuntimeSurfaceResult` without `AgentFrame`; include VFS, backend anchor, provenance. |
| `agentdash-api/src/routes/lifecycle_views.rs::get_agent_frame_runtime` | Direct `frame_id` debug read view | Lifecycle read-model facade | Keep if explicitly debug/read endpoint; document it is not current runtime surface query. Consider moving conversion to application read service. |
| `agentdash-api/src/routes/vfs_surfaces/resolver.rs::resolve_agent_run_frame_vfs_for_agent` | API route manually resolves current frame for AgentRun VFS | AgentRun resource/runtime surface query | Move current frame + lifecycle projector composition into application query facade. |
| `agentdash-application/src/session/types.rs::AgentFrameRuntimeTarget` | Session-owned type used by AgentRun capability, hooks, workspace module, permission | AgentRun runtime address / hook control target / update target | Split target types: query address, hook target, update/adoption target. |
| `agentdash-application/src/agent_run/effective_capability.rs` | Public view exposes session `AgentFrameRuntimeTarget` | AgentRun effective capability | Return AgentRun/Lifecycle runtime address + surface ref; keep `AgentFrame` internal. |
| `agentdash-application/src/session/capability_service.rs::effective_capability_view_for_runtime_session` | Session facade maps session id to frame target then calls AgentRun capability | AgentRun runtime surface/capability query | Move to query facade or delivery adapter; remove from mixed session capability service. |
| `agentdash-application/src/session/hub/tool_builder.rs::get_runtime_mcp_servers` and `get_latest_capability_state` | Hub reads current frame even outside active turn | Query facade for current surface; hub only active snapshot | Keep active-turn read for refresh; migrate idle/current reads. |
| `agentdash-application/src/companion/tools.rs` hook target use | Companion tool holds `AgentFrameRuntimeTarget` to get hook runtime | Hook/runtime control target adapter | Ensure companion consumes hook control target produced by AgentRun surface/query or command context, not raw session-target lookup. |
| `agentdash-application/src/workspace_module/visibility.rs` | WorkspaceModule code/tests use `AgentFrameRuntimeTarget` | AgentRun effective capability / surface update | Verify production path; route visibility reads through AgentRun capability query. |
| `agentdash-api/src/routes/permission_grants.rs::adopt_grant_effect` | Route-level `AgentFrame`/adoption coupling | Permission service + runtime surface update use case | Move adoption inside application service; route only maps DTO/errors. |
| Static validation | Existing imports can regress | Check manifest | Add focused grep/check after implementation: API crate should not import `agentdash_domain::workflow::AgentFrame` except lifecycle debug/read model allowlist; API crate should not import `agent_run::frame::surface::AgentFrameSurfaceExt`. |

### Related Specs

- `.trellis/spec/project-overview.md`: `AgentFrame` 是 effective runtime surface revision；`RuntimeSessionExecutionAnchor` 独立索引 runtime session 到 run/agent/frame。
- `.trellis/spec/backend/session/architecture.md`: `RuntimeSession` 不拥有 business ownership、permission scope、Lifecycle progress 或 Agent effective surface。
- `.trellis/spec/backend/session/runtime-execution-state.md`: runtime map / active turn / connector live session 分离；AgentRun lifecycle surface 从 AgentRun runtime address 构造。
- `.trellis/spec/backend/session/execution-context-frames.md`: active turn `ExecutionContext` 只是 connector-facing projection；hot update 可读 active snapshot，但不写回成 application 事实源。
- `.trellis/spec/backend/runtime-gateway.md`: 当前 “Provider 通过 `RuntimeSessionMcpAccess` 进入 SessionHub” 是需要更新的旧约束；MCP session action 仍是 Session action，但 backing access 应消费 AgentRun runtime surface query。
- `.trellis/spec/backend/capability/architecture.md`: AgentRun effective capability/admission 是 runtime 能力读取唯一入口。
- `.trellis/spec/backend/permission/architecture.md`: surface-changing Grant 写 `AgentFrame` revision；adoption failure 必须可见。
- `.trellis/spec/backend/vfs/architecture.md`: AgentRun surface resolver 应在 application 层输出已闭包 resource surface。
- `.trellis/spec/backend/workflow/architecture.md`: 读取 workspace/VFS 时从 runtime session anchor 到 current frame 再消费 typed surface，但这应由 application resolver/facade 承担，而不是 API/hub consumer 各自实现。

### External References

- No external references used. This is an internal codebase boundary inventory.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task, but the dispatch prompt explicitly provided `.trellis/tasks/06-23-session-hub-boundary-cleanup`; output was written to that explicit path.
- This inventory is `rg` + targeted file/spec inspection, not an AST visibility analysis. It may include test-only references; entries called out as tests should be verified during implementation before changing production APIs.
- Frontend generated DTO uses `AgentFrameRefDto` / `AgentFrameRuntimeView`; this report does not classify DTO refs as boundary violations unless they are used as current runtime surface query inputs.
- No code was modified and no tests were run; this research only writes the requested artifact.
