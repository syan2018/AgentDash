# 清理 session hub 错误归属路径

## Goal

清理 `session/hub` 中不属于 hub 抽象的 runtime surface、MCP discovery、frame surface 解析与业务 mutation 路径，建立新的归属边界，使 Session runtime live-cache、AgentRun/Lifecycle runtime surface、RuntimeGateway action、Canvas/WorkspaceModule/Permission 等业务路径各自回到正确模块。

本任务的核心目标不是修补 `mcp.list_tools` 的单点缺失 anchor，而是把暴露该问题的错误依赖链路整体收束：RuntimeGateway 的 session action 不能再依赖 `SessionCapabilityService -> SessionRuntimeInner(session/hub)` 作为 AgentRun runtime surface 查询入口；Canvas、extension runtime、VFS `SessionRuntime` / `AgentRun` surface、MCP 工具发现等 consumer 必须消费同一套 AgentRun / Lifecycle runtime surface query 结果。

## Problem Statement

Canvas iframe 中调用：

```ts
await window.agentdash.invoke("mcp.list_tools", {});
```

当前会进入 RuntimeGateway，并被识别为 `mcp.list_tools` session action，但后端报错：

```text
runtime action 当前不可执行:
runtime backend anchor missing:
component=runtime_mcp_tool_discovery,
session_id=Some("..."),
turn_id=None
```

已确认直接原因：

- Canvas runtime route 在 `crates/agentdash-api/src/routes/canvases.rs` 正确组装 `RuntimeActor::UserCanvas` 与 `RuntimeContext::Session`。
- `runtime_bridge.surface` 使用 `RuntimeGateway::surface_for_actor`，只能说明 action key 对 actor/context 可见，不能证明 MCP runtime tool surface 已经闭合可执行。
- RuntimeGateway 的 `mcp.list_tools` / `mcp.call_tool` provider 在 `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` 绑定到 `RuntimeSessionMcpAccess`。
- 当前 `RuntimeSessionMcpAccess` 由 `SessionCapabilityService` 实现，而 `SessionCapabilityService` 委托 `SessionRuntimeInner(session/hub)`。
- `SessionRuntimeInner::discover_runtime_mcp_tool_entries` 在 active turn 分支读取 `turn.session_frame.runtime_backend_anchor`；但在 idle / 非 turn 分支通过 `RuntimeSessionExecutionAnchor` 找到 current `AgentFrame` 后只取 `typed_mcp_servers`、`typed_vfs` 和 capability state，并把 `backend_anchor` 与 `identity` 置空。
- 因此 Canvas 点击这类非 Agent turn 内调用会稳定得到 `runtime_mcp_tool_discovery` missing backend anchor，且 `turn_id=None`。

该现象说明 `session/hub` 仍承担了不该承担的职责：它既在做 live active turn cache，又在做 AgentFrame runtime surface 查询、MCP tool discovery admission、runtime backend anchor 解析、capability projection 采样、Canvas exposure/adoption 等跨控制面的工作。

## Desired Architecture Direction

### Session Hub Remaining Responsibility

`session/hub` 应收束为 Session live runtime coordination 边界：

- runtime registry / active turn state。
- connector session lifecycle 与 active turn cache。
- live connector tool update / runtime tool refresh 的执行协调。
- hook runtime delivery binding cache。
- active turn 内部需要的 transient snapshots。
- session launch/turn supervision 的低层 runtime 协调。

这些职责的共同点是：它们服务“当前进程内 live session/turn 如何运行”，而不是回答“某个 AgentRun 当前 runtime surface 是什么”。

### Runtime Surface Query Responsibility

AgentRun runtime surface 查询应由 AgentRun / Lifecycle control-plane 承担。对外粒度应是 run/agent/runtime address，而不是 `AgentFrame` 对象：

```text
runtime_session_id
  -> RuntimeSessionExecutionAnchor
  -> AgentRun/Lifecycle runtime address
  -> internal current surface revision lookup
  -> closed runtime surface:
       VFS
       MCP servers
       CapabilityState
       RuntimeBackendAnchor
       Identity/admission context
       Project/run/agent/runtime surface provenance
```

该 query facade 应位于 application 层 AgentRun/Lifecycle 相关模块，而不是 API route、Canvas、RuntimeGateway provider、VFS route 或 session hub 内。它内部可以读取 current `AgentFrame` revision，但 `AgentFrame` 不应成为 RuntimeGateway 或 API consumer 的依赖类型。

### RuntimeGateway Session Action Responsibility

`mcp.list_tools`、`mcp.call_tool` 仍是 RuntimeGateway session action，但其 backing access 应消费新的 AgentRun runtime surface query port。Provider 只负责 action input/output、actor/context admission 和调用 `RuntimeSessionMcpAccess`；它不应知道 `session/hub`，也不应知道 `AgentFrame`。

### Business Path Responsibility

Canvas、WorkspaceModule、Permission、VFS surface、Extension runtime 等业务模块只能提交或消费明确的 application use case：

- Canvas runtime invoke 只组装 Canvas actor/context 并调用 RuntimeGateway。
- Canvas runtime snapshot / VFS asset 只消费 runtime surface query 返回的 VFS/resource surface。
- WorkspaceModule operation dispatch 不直接写 AgentFrame surface，不直接调用 hub adoption primitive。
- Permission grant surface-changing effect 不在 API route 直接 adopt active runtime。
- Extension runtime backend target、workspace context 与 VFS target 只通过同一 query facade 获得 runtime backend anchor 和 VFS。

## Requirements

- 定义 `session/hub` 的保留职责与迁出职责，并在 `design.md` 中列出迁移矩阵。
- 新增或抽出 AgentRun/Lifecycle runtime surface query facade，作为 `runtime_session_id -> current closed surface` 的唯一 application 查询入口。
- query facade 必须从 `RuntimeSessionExecutionAnchor` 和 AgentRun/Lifecycle runtime address 派生 runtime surface，并生成 `RuntimeBackendAnchor`；不允许 consumer 在 API route 或 hub idle 分支重新拼装。
- query facade 可以在内部读取 current `AgentFrame` revision，但 `AgentFrame` 不能出现在 RuntimeGateway access、API route consumer 或 Canvas/Extension/VFS consumer 的公开 contract 中。
- 明确 `AgentFrame` 的当前实际暴露面，并把它收束为内聚实现细节：frame construction、launch closure、surface query、surface update use case、repository adapter 可以直接持有 `AgentFrame`；RuntimeGateway、API route、Canvas/Extension/VFS/Terminal consumer、session hub idle query 不应直接持有 `AgentFrame` 或 `AgentFrameRepository`。
- `mcp.list_tools` / `mcp.call_tool` 的 backing access 必须迁出 `SessionCapabilityService -> SessionRuntimeInner`，改为依赖新的 runtime surface query port。
- `SessionCapabilityService` 的职责必须拆分或重新命名，不能继续作为 RuntimeGateway MCP access、Canvas exposure/adoption、frame target resolver、skill baseline projector 和 pending runtime command adapter 的混合 facade。
- `session/hub/tool_builder.rs` 中的 MCP discovery idle surface 解析必须迁出；active turn live tool refresh 可留在 hub，但只能消费已闭合的 runtime surface / execution context。
- `get_current_runtime_backend_anchor(session_id)` 不能只从 active turn cache 读取。需要 runtime backend anchor 的 API/业务路径必须走新的 AgentRun/Lifecycle runtime surface query。
- `resolve_session_frame_vfs`、Canvas runtime snapshot、Extension runtime action/channel、VFS surface `SessionRuntime` source、VFS surface `AgentRun` current delivery source、RuntimeGateway MCP session actions 应共享同一 AgentRun runtime surface query / resource surface facade，避免同一个 session 被不同模块解析出不同 VFS/backend/MCP surface。
- Terminal spawn 也使用 `resolve_session_frame_vfs` + backend anchor 模式，属于同类 current runtime surface consumer；本任务需要纳入排查和迁移设计，避免保留最后一条旧 API consumer。
- 明确旧路径涉及的模块、调用点、迁移目标与验收检查：
  - `crates/agentdash-application/src/session/hub/tool_builder.rs`
  - `crates/agentdash-application/src/session/capability_service.rs`
  - `crates/agentdash-api/src/app_state.rs`
  - `crates/agentdash-api/src/bootstrap/runtime_gateway.rs`
  - `crates/agentdash-api/src/session_construction.rs`
  - `crates/agentdash-api/src/routes/canvases.rs`
  - `crates/agentdash-api/src/routes/extension_runtime.rs`
  - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs`
  - VFS surface `AgentRun` source 中的 current frame / lifecycle projection 路径
  - `crates/agentdash-api/src/routes/terminals.rs`
  - `crates/agentdash-application/src/workspace_module/*`
  - Permission grant apply/revoke surface adoption path
  - session launch / active turn tool refresh paths that legitimately remain in hub
- 保留 active runtime adoption primitive 时，必须把它降为新的 surface update/use case 内部 primitive；业务模块不能直接调用。
- 项目未上线，不设计兼容旧路径的 fallback；迁移后旧入口应删除、私有化或变成新 query/update service 的内部 helper。
- 为 `mcp.list_tools` Canvas idle 调用补回归测试，证明没有 active turn 时仍能从 AgentRun/Lifecycle current runtime surface 获得 backend anchor 并完成 MCP discovery admission。
- 为 query facade 增加单元测试或应用层测试，覆盖 active turn 与 idle/current surface 两种读取场景，并验证 backend anchor、VFS、MCP servers、capability state 来自同一 closed surface closure。

## Non-Goals

- 不重做 RuntimeGateway action 分类模型；`mcp.list_tools` / `mcp.call_tool` 仍是 SessionRuntime action。
- 不改变 Canvas iframe bridge 的浏览器 API 形态。
- 不重新设计 MCP preset / relay transport 协议。
- 不迁移历史 session event 或历史 runtime trace 数据。
- 不把所有 session runtime 内部协调逻辑移出 hub；hub 保留 live runtime coordination 职责。

## Acceptance Criteria

- [ ] `RuntimeGateway` 的 MCP session action backing access 不再由 `SessionCapabilityService` 直接实现并委托 `SessionRuntimeInner::discover_runtime_mcp_tool_entries`。
- [ ] 新的 AgentRun/Lifecycle runtime surface query 能从 `runtime_session_id` 返回 closed surface：VFS、MCP servers、capability state、runtime backend anchor、project/run/agent/runtime surface provenance；RuntimeGateway/API consumer 不依赖 `AgentFrame` 类型。
- [ ] Canvas iframe 在非 active turn 内调用 `window.agentdash.invoke("mcp.list_tools", {})` 不再触发 `runtime_mcp_tool_discovery` missing backend anchor。
- [ ] `session/hub/tool_builder.rs` 不再包含 idle AgentFrame surface 解析和 runtime backend anchor missing 兜底报错；hub 内保留的 tool builder 逻辑只处理 active runtime / connector live update。
- [ ] `resolve_session_frame_vfs`、Canvas runtime snapshot、Extension runtime、VFS surface `SessionRuntime` / `AgentRun` source、RuntimeGateway MCP access 共享同一 query/resource surface facade，不再各自实现 runtime session 或 run/agent -> internal frame revision -> VFS/backend 解析。
- [ ] `get_current_runtime_backend_anchor(session_id)` 类型的 active-turn-only helper 不再被 API/业务 consumer 用作 current runtime backend anchor 查询入口。
- [ ] Canvas、WorkspaceModule、Permission 等业务路径没有直接调用 active runtime adoption primitive 或手写 current frame capability surface 的入口。
- [ ] 迁移矩阵列出所有旧路径、正确归属模块、处理策略和测试覆盖。
- [ ] Rust 编译通过，相关 application/API 单元测试或集成测试覆盖 MCP list/call、Canvas runtime invoke、Extension runtime backend target、VFS session surface query。
- [ ] 相关架构约束更新到 `.trellis/spec/backend` 或任务 `design.md`，说明为什么 runtime surface query 不属于 `session/hub`。

## Confirmed Evidence

- `crates/agentdash-api/src/routes/canvases.rs` 的 `/runtime-invoke` 正确使用 `RuntimeActor::UserCanvas` 与 `RuntimeContext::Session`。
- `crates/agentdash-api/src/routes/canvases.rs` 的 runtime bridge surface 使用 `RuntimeGateway::surface_for_actor`，只表达 action visibility。
- `crates/agentdash-api/src/app_state.rs` 将 `SessionCapabilityService` 注入为 `Arc<dyn RuntimeSessionMcpAccess>`。
- `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` 将该 access 绑定给 `McpListToolsProvider` 与 `McpCallToolProvider`。
- `crates/agentdash-application/src/session/capability_service.rs` 的 `RuntimeSessionMcpAccess` 实现委托 hub discovery。
- `crates/agentdash-application/src/session/hub/tool_builder.rs` 的 idle discovery 分支把 `backend_anchor` 与 `identity` 置空，随后要求 backend anchor，导致 Canvas idle 调用失败。
- `crates/agentdash-domain/src/workflow/mod.rs` 直接 `pub use agent_frame::AgentFrame` 与 `AgentFrameRepository`，当前 application 单 crate 内模块很容易横向裸引用 frame entity/repository。
- `crates/agentdash-api/src/session_construction.rs` 直接 import `AgentFrameSurfaceExt`、`resolve_current_frame_from_delivery_trace_ref` 与 `AgentFrame`，并向 Canvas/Extension/VFS/Terminal 等 API consumer 返回 `SessionFrameVfsResult { vfs, runtime_backend_anchor, frame }`。
- `crates/agentdash-api/src/routes/lifecycle_views.rs` 与 `routes/sessions.rs` 直接通过 current frame resolver 构建视图；这类 presentation projection 可以保留读取语义，但应避免成为 runtime action/current surface consumer 的复制入口。
- `crates/agentdash-application/src/session/capability_service.rs`、`session/hub/tool_builder.rs`、`permission/service.rs`、`canvas/tools.rs`、`workspace_module/tools.rs` 当前都能间接或直接触发 AgentFrame revision 写入/adoption；这些应归到 runtime surface update use case，而不是 session capability facade/hub。
- `crates/agentdash-application/src/session/launch/*`、`agent_run/frame/*`、`lifecycle/dispatch_service.rs` 等启动/构造链路直接持有 `AgentFrame` 是合理内部实现，因为它们负责创建、闭合或启动 AgentRun surface。
- `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs` 已有 `FrameLaunchSurface::runtime_backend_anchor(...)`，说明 backend anchor 的派生逻辑应来自 closed frame surface。
- `crates/agentdash-application/src/agent_run/frame/construction/mod.rs` 的 `build_envelope_from_frame(...)` 已经在 construction 链路中通过 closed launch surface 生成 runtime backend anchor。
- `crates/agentdash-api/src/session_construction.rs` 当前通过 `session_capability.get_current_runtime_backend_anchor(session_id)` 获取 anchor，该 helper 只读 active turn cache，idle 时不是可靠 current surface 查询。
- `crates/agentdash-api/src/routes/terminals.rs` 的 terminal launch target 也经由 `resolve_session_frame_vfs` 获取 VFS/backend anchor，说明旧 helper 已成为多个 API consumer 的隐性汇聚点。
- `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs` 的 `ResolvedVfsSurfaceSource::AgentRun` 当前在 API 层独立执行 anchor/current frame/lifecycle projection；如果不纳入同一 resource surface facade，`SessionRuntime` 与 `AgentRun` 两条 VFS source 仍可能对同一 delivery runtime 解析出不同 VFS。
- 子代理调查未发现现有单一 query facade 能同时返回 VFS、MCP servers、capability state、runtime backend anchor、identity/admission context 与 provenance；当前只有 `resolve_current_frame_from_delivery_trace_ref` 等 partial helper。

## Research Tasks

- 调查 `session/hub` 当前模块职责和所有外部调用点，分类为保留在 hub、迁出到 AgentRun/Lifecycle runtime surface query、迁出到 RuntimeGateway/provider、迁出到业务 use case 四类。
- 调查 RuntimeGateway MCP action、direct MCP tools、relay MCP discovery、runtime tool assembly 之间的边界，明确哪些逻辑是 declaration/active turn refresh，哪些是 user/runtime action invoke。
- 调查 Canvas、WorkspaceModule、Extension runtime、VFS surface、Permission grant 当前使用 `session_capability` / `resolve_session_frame_vfs` / adoption primitive 的路径，形成迁移矩阵。
- 调查 Terminal spawn、VFS `AgentRun` source 与其它未列出的 API consumer 是否仍依赖 `resolve_session_frame_vfs`、独立 current frame resolver 或 active-turn-only backend anchor helper。
- 调查现有测试夹具和可复用 helper，设计最小回归测试覆盖 Canvas idle `mcp.list_tools` 与 query facade closed surface。
- 调查 `AgentFrame` 当前实际暴露面，形成允许直接持有 `AgentFrame` 的内聚实现区域与必须迁到 query/update port 的 consumer 清单。

## Resolved Decisions

- 新 query facade 归属为 AgentRun/Lifecycle control-plane，命名采用 `agent_run::runtime_surface` / `AgentRunRuntimeSurfaceQuery` / `AgentRunRuntimeSurfaceQueryPort`；避免放在 `agent_run::frame` 下让 `AgentFrame` 成为对外 API 语义。
- query facade 返回新的 `AgentRunRuntimeSurface` / `CurrentRuntimeSurface` 风格结构，而不是直接返回 `FrameLaunchSurface`；launch-only 语义不泄漏到 current query path。
- active turn runtime action query 以 AgentRun/Lifecycle current committed runtime surface 为事实源，只附加 `active_turn_id`、live identity/hook context 等 transient metadata；connector live tool refresh 仍使用 hub active turn snapshot。
- 本任务硬收 RuntimeGateway、API current surface consumer、VFS `SessionRuntime` / `AgentRun` resource surface、session hub idle query、Canvas/WorkspaceModule/Permission surface-changing business update/adoption；Lifecycle/session presentation read-model 中的 frame DTO 作为 transitional 白名单，不阻塞本任务。
- Permission grant adoption 纳入实施范围，但以“API route 不直接 adopt、application use case 内部处理 revision + adoption”为验收目标；若具体 mutation 迁移风险过高，可在完成 RuntimeGateway/API current-surface 后拆后续子任务，但本任务必须删除或封闭 route-level direct adoption。

## Deferred Follow-up

- 拆分 application crate 或引入更强 Rust visibility boundary，防止 `AgentFrame` / `AgentFrameRepository` 被横向模块随手 import。
- 将 Lifecycle/session presentation read-model 的 frame projection 迁到专门 read-model facade，并和 runtime surface query facade 保持隔离。
