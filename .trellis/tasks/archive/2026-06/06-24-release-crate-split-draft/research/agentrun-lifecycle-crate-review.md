# Research: AgentRun/Lifecycle crate boundary review

- Query: 复核 AgentRun/Lifecycle 控制面在 application crate 拆分中的正确粒度、引用方向、RuntimeSession anchor 流向与 split draft 修正点。
- Scope: internal
- Date: 2026-06-25

## Findings

### Files found

- `.trellis/tasks/06-24-release-crate-boundary-review/research/02-agentrun-lifecycle-surface.md` - 既有 AgentRun/Lifecycle surface ownership 与 RuntimeSession substrate 调研。
- `.trellis/tasks/06-24-release-crate-boundary-review/research/05-crate-split-coupling-map.md` - 既有 application crate split coupling map 与 extraction wave 建议。
- `.trellis/tasks/06-24-release-crate-split-draft/prd.md` - split draft holder 需求与验收标准。
- `.trellis/tasks/06-24-release-crate-split-draft/design.md` - 候选 crate 图、依赖方向、extraction waves 与 blocking conditions。
- `.trellis/tasks/06-24-release-crate-split-draft/implement.md` - 未来 ports / RuntimeGateway / RuntimeSession / AgentRun / Lifecycle wave checklist。
- `.trellis/spec/backend/workflow/architecture.md` - LifecycleRun、OrchestrationInstance、RuntimeNodeState、AgentFrame、RuntimeSessionExecutionAnchor 与 reducer 合同。
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession substrate、AgentFrame runtime surface、AgentRun frame/surface command boundary 合同。
- `.trellis/spec/backend/capability/architecture.md` - AgentRun effective capability/admission 读取入口合同。
- `.trellis/spec/backend/permission/architecture.md` - Permission grant 到 AgentFrame revision / AgentRun admission 的分流合同。
- `crates/agentdash-domain/src/workflow/agent_frame.rs` - AgentFrame revision domain entity，保存 capability/context/VFS/MCP/execution profile 与可见 workspace surface。
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs` - RuntimeSession 到 run/agent/frame/orchestration node 的 backlink。
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs` - LifecycleAgent current delivery binding。
- `crates/agentdash-domain/src/workflow/entity.rs` - LifecycleRun aggregate，拥有 context/orchestrations/view_projection。
- `crates/agentdash-application/src/agent_run/runtime_surface.rs` - AgentRun current runtime/resource surface query 与 RuntimeGateway MCP surface adapter。
- `crates/agentdash-application/src/agent_run/runtime_surface_update.rs` - AgentRun runtime surface update service 与 active runtime adoption port。
- `crates/agentdash-application/src/agent_run/runtime_target.rs` - AgentFrameRuntimeTarget / AgentFrameHookRuntimeTarget，已从 session 语义移到 AgentRun。
- `crates/agentdash-application/src/agent_run/effective_capability.rs` - AgentRun effective capability/admission 与 grant projection。
- `crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs` - current delivery selection 通过 anchor 解析 current frame。
- `crates/agentdash-application/src/agent_run/workspace/query.rs` - AgentRun workspace/resource surface read model，当前直接调用 Lifecycle surface projector。
- `crates/agentdash-application/src/agent_run/project_agent_start.rs` - ProjectAgent start 直接调用 LifecycleDispatchService。
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs` - Lifecycle dispatch/materialization，创建 RuntimeSession、AgentFrame、RuntimeSessionExecutionAnchor 并提交 NodeStarted。
- `crates/agentdash-application/src/lifecycle/session_association.rs` - RuntimeSession trace 到 current AgentFrame / lifecycle node association resolver。
- `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs` - AgentRun lifecycle mount/resource projection DTO 与 projector。
- `crates/agentdash-application/src/lifecycle/orchestrator.rs` - terminal callback / complete_lifecycle_node 到 OrchestrationRuntimeEvent 的 orchestration service。
- `crates/agentdash-application/src/workflow/orchestration/runtime.rs` - Orchestration runtime reducer。
- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs` - ready node drain / function / human gate / AgentCall launcher coordinator。
- `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs` - ready AgentCall node materialization，委托 LifecycleDispatchService。
- `crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs` - 已存在的 RuntimeGateway-facing MCP current surface port。

### Spec baseline

- `RuntimeSession` 是 delivery / trace substrate，不拥有业务归属、permission scope、Lifecycle progress 或 Agent effective surface；`AgentFrame` 是 capability/context/VFS/MCP 事实源，`RuntimeSessionExecutionAnchor` 负责 runtime trace/delivery 反查控制面身份（`.trellis/spec/backend/session/architecture.md:5`, `.trellis/spec/backend/session/architecture.md:29`, `.trellis/spec/backend/session/architecture.md:30`）。
- Runtime trace callback 必须先经 `RuntimeSessionExecutionAnchor` 投影到 run/agent/frame/orchestration node coordinate，再进入业务校验和 terminal effect（`.trellis/spec/backend/session/architecture.md:33`, `.trellis/spec/backend/session/architecture.md:62`, `.trellis/spec/backend/session/architecture.md:63`）。
- Workflow 规格明确 `LifecycleRun` 是 tracked life process/control ledger，`OrchestrationInstance` 归属 LifecycleRun，runtime node key 是 `orchestration_id + node_path + attempt`（`.trellis/spec/backend/workflow/architecture.md:31`, `.trellis/spec/backend/workflow/architecture.md:33`, `.trellis/spec/backend/workflow/architecture.md:36`）。
- `RuntimeSessionExecutionAnchor` 可以作为 message stream / launch evidence ref，但不拥有 node runtime；node projection 必须由 `orchestration_id + node_path + attempt` 构造（`.trellis/spec/backend/workflow/architecture.md:39`, `.trellis/spec/backend/workflow/architecture.md:41`, `.trellis/spec/backend/workflow/architecture.md:44`）。
- `AgentFrame` revision 分 dispatch launch evidence 与 runtime surface 两类生产角色；workspace/VFS 读取从 runtime session anchor 进入 agent current frame，再消费 frame typed surface（`.trellis/spec/backend/workflow/architecture.md:45`）。
- AgentRun effective capability/admission 是运行时能力读取唯一入口，输入包括 run/agent/current AgentFrame/delivery runtime 和 grant system；Permission grant 的 tool-internal effect 只进入 admission，surface-changing effect 写 AgentFrame revision（`.trellis/spec/backend/capability/architecture.md:43`, `.trellis/spec/backend/capability/architecture.md:50`, `.trellis/spec/backend/permission/architecture.md:44`, `.trellis/spec/backend/permission/architecture.md:45`）。

### Current code patterns

#### AgentFrame / RuntimeSessionExecutionAnchor / LifecycleRun facts

- `AgentFrame` domain entity 保存 `effective_capability_json`、`context_slice_json`、`vfs_surface_json`、`mcp_surface_json`、`execution_profile_json`、visible canvas mounts 与 visible workspace modules；这支持“AgentFrame 是 AgentRun/Lifecycle surface revision，不是 RuntimeSession entity”的拆分方向（`crates/agentdash-domain/src/workflow/agent_frame.rs:10`, `crates/agentdash-domain/src/workflow/agent_frame.rs:15`, `crates/agentdash-domain/src/workflow/agent_frame.rs:17`, `crates/agentdash-domain/src/workflow/agent_frame.rs:19`, `crates/agentdash-domain/src/workflow/agent_frame.rs:21`, `crates/agentdash-domain/src/workflow/agent_frame.rs:23`, `crates/agentdash-domain/src/workflow/agent_frame.rs:26`, `crates/agentdash-domain/src/workflow/agent_frame.rs:33`）。
- `RuntimeSessionExecutionAnchor` 存 runtime session id、run id、launch frame id、agent id 和 optional `orchestration_id/node_path/node_attempt`；普通 dispatch 与 orchestration dispatch 分别由 `new_dispatch` / `new_orchestration_dispatch` 构造（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:47`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:68`）。
- `LifecycleRun` aggregate 拥有 `context`、`orchestrations`、`view_projection`，并通过 `add_orchestration` / `replace_orchestration` / `orchestration_by_id` 管理 OrchestrationInstance（`crates/agentdash-domain/src/workflow/entity.rs:159`, `crates/agentdash-domain/src/workflow/entity.rs:164`, `crates/agentdash-domain/src/workflow/entity.rs:166`, `crates/agentdash-domain/src/workflow/entity.rs:170`, `crates/agentdash-domain/src/workflow/entity.rs:221`, `crates/agentdash-domain/src/workflow/entity.rs:232`, `crates/agentdash-domain/src/workflow/entity.rs:247`）。
- `LifecycleAgent.current_delivery` 保存 runtime session、launch frame、optional orchestration node coordinate；`bind_current_delivery_from_anchor` 从 anchor 复制这些事实（`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:127`, `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:131`, `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:133`, `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:135`, `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:221`）。

#### AgentRun current surface / resource surface

- `AgentRunRuntimeSurfaceQueryPort` 以 `runtime_session_id + RuntimeSurfaceQueryPurpose` 返回 current runtime surface 或带 backend anchor 的 surface（`crates/agentdash-application/src/agent_run/runtime_surface.rs:69`）。
- `AgentRunRuntimeSurfaceQuery` 依赖 `RuntimeSessionExecutionAnchorRepository`、`LifecycleRunRepository`、`LifecycleAgentRepository`、`AgentFrameRepository`，并在 `resolve_surface` 中按 anchor -> run -> agent -> current frame 顺序解析（`crates/agentdash-application/src/agent_run/runtime_surface.rs:53`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:54`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:55`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:56`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:57`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:212`）。
- Surface DTO 同时保留 `launch_evidence_frame_id` 与 `current_surface_frame_id`，这是正确的跨 crate DTO 粒度；tests 也断言两者分离（`crates/agentdash-application/src/agent_run/runtime_surface.rs:357`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:362`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:363`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:424`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:425`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:426`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:1268`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:1271`）。
- `AgentRunResourceSurfaceQuery` 已存在为 concrete service，能按 runtime session 或 run/agent 投影 resource surface，但没有对应 trait port；`rg` 没有发现 `AgentRunResourceSurfaceQueryPort`，仅有 concrete `resource_surface_for_agent_run`（`crates/agentdash-application/src/agent_run/runtime_surface.rs:84`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:121`）。
- Resource surface query 当前直接调用 `AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(...).project_workspace_read_surface(...)`（`crates/agentdash-application/src/agent_run/runtime_surface.rs:160`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:180`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:181`）。如果 AgentRun / Lifecycle 物理拆分，这里需要变成 Lifecycle surface projection port，而不是 AgentRun crate 依赖 Lifecycle implementation。
- `RuntimeGatewayMcpSurfaceQueryPort` 已移到 `agentdash-application-ports`，`CurrentSurfaceRuntimeMcpAccess` 只依赖该 port 与 MCP discovery（`crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs:61`, `crates/agentdash-application/src/runtime_gateway/mcp_access.rs:8`, `crates/agentdash-application/src/runtime_gateway/mcp_access.rs:23`）。`AgentRunRuntimeSurfaceQuery` 作为 adapter 实现该 gateway-facing port（`crates/agentdash-application/src/agent_run/runtime_surface.rs:746`）。这是正确方向。

#### AgentRun update / effective capability

- `AgentFrameRuntimeTarget` 已位于 `agent_run/runtime_target.rs`，表达要采用的 AgentFrame revision 与 delivery runtime session；这修正了旧 coupling map 中 “target 仍在 session::types” 的问题（`crates/agentdash-application/src/agent_run/runtime_target.rs:9`）。
- `AgentRunActiveRuntimeSurfaceAdopter` 是 AgentRun update 到 RuntimeSession live coordination 的 port；`AgentRunRuntimeSurfaceUpdateService` 依赖 current surface query、AgentFrameRepository、VfsService 与 adopter（`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:23`, `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:31`, `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:33`, `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:35`）。
- `AgentRunFrameSurfaceService` 是 frame/surface write facade，`RuntimeSurfaceUpdateRequest` 只携带 stable changed-resource identity，包含 Canvas、Permission、MCP、VFS、WorkspaceModule、SkillInventory、AgentProcedure 变更类型（`crates/agentdash-application/src/agent_run/frame/surface_service.rs:88`, `crates/agentdash-application/src/agent_run/frame/surface_service.rs:430`）。
- Permission update 现在通过 `AgentRunFrameSurfaceService` 包装 adapter，但 adapter 仍在 AgentRun module 内直接使用 `AgentFrameRepository` 与 `AgentFrameBuilder` 写 effect frame；这可以作为 AgentRun-owned implementation detail，但不应被 Permission/Lifecycle/API route 直接复用（`crates/agentdash-application/src/agent_run/permission_runtime_surface_update.rs:48`, `crates/agentdash-application/src/agent_run/permission_runtime_surface_update.rs:89`, `crates/agentdash-application/src/agent_run/permission_runtime_surface_update.rs:193`）。
- `AgentRunRuntimeSurfaceUpdateService::expose_canvas_mount` 仍是 specialized method，而不是统一走 `RuntimeSurfaceUpdateRequest::CanvasVisibilityRequested/CanvasBindingChanged` facade；这会让未来 public update facade 需要再收敛一次（`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:71`, `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:121`, `crates/agentdash-application/src/agent_run/runtime_surface_update.rs:141`）。
- `AgentRunEffectiveCapabilityService` 是纯 projection/admission 逻辑，输出 `AgentRunEffectiveCapabilityView` 与 `AgentRunAdmissionDecision`，但当前还没有 repository-backed trait port；`execution_capability_state_for_runtime_session` 仍直接从 runtime session anchor 查 run grants（`crates/agentdash-application/src/agent_run/effective_capability.rs:143`, `crates/agentdash-application/src/agent_run/effective_capability.rs:175`, `crates/agentdash-application/src/agent_run/effective_capability.rs:196`, `crates/agentdash-application/src/agent_run/effective_capability.rs:279`）。

#### Lifecycle dispatch / materialization / reducer

- `LifecycleDispatchService` 当前定义 `RuntimeSessionCreator` port，但同文件也有 `SessionPersistenceRuntimeSessionCreator` adapter，并直接 import session `ExecutionStatus/SessionMeta/SessionPersistence/TitleSource`；物理拆分时 port 应移到 application-ports 或 runtime-session-delivery ports，adapter 留在 RuntimeSession implementation/composition root（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:25`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:35`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:43`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:51`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:62`）。
- Graph-backed dispatch 的正确事实闭包已经存在：创建/复用 RuntimeSession，创建 AgentFrame，写 `RuntimeSessionExecutionAnchor::new_orchestration_dispatch`，`bind_current_delivery_from_anchor`，然后提交 `OrchestrationRuntimeEvent::NodeStarted` 并持久化 updated LifecycleRun（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:474`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:499`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:509`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:522`）。
- Plain dispatch 也正确写 `RuntimeSessionExecutionAnchor::new_dispatch` 并绑定 current delivery，但不触碰 orchestration reducer（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:572`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:598`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:605`）。
- Scheduler ready `AgentCall` node 通过 `LifecycleDispatchService::materialize_workflow_agent_node` 创建 agent/frame/runtime session/anchor 并返回 `AgentRuntimeRefs`；`AgentNodeLauncher` 随后返回 `NodeStarted` event，由 executor launcher apply event（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:356`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:411`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:421`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:429`, `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:159`, `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:189`, `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:194`）。
- Reducer 已按规格集中在 `workflow/orchestration/runtime.rs`：`OrchestrationRuntimeEvent` 只携带 node path/attempt/executor ref/outputs/error，`apply_orchestration_event_to_run` 先定位 orchestration，再 apply event、刷新 run status/时间戳（`crates/agentdash-application/src/workflow/orchestration/runtime.rs:179`, `crates/agentdash-application/src/workflow/orchestration/runtime.rs:266`, `crates/agentdash-application/src/workflow/orchestration/runtime.rs:275`, `crates/agentdash-application/src/workflow/orchestration/runtime.rs:303`, `crates/agentdash-application/src/workflow/orchestration/runtime.rs:330`）。
- `LifecycleOrchestrator` 的 terminal callback / `advance_current_activity` 先通过 runtime session trace 解析 lifecycle node association，再构造 terminal event 交给 reducer，符合 RuntimeSession 不直接修改 RuntimeNodeState 的方向（`crates/agentdash-application/src/lifecycle/orchestrator.rs:145`, `crates/agentdash-application/src/lifecycle/orchestrator.rs:155`, `crates/agentdash-application/src/lifecycle/orchestrator.rs:172`, `crates/agentdash-application/src/lifecycle/orchestrator.rs:179`, `crates/agentdash-application/src/lifecycle/orchestrator.rs:226`, `crates/agentdash-application/src/lifecycle/orchestrator.rs:242`）。

#### Current cross-module coupling relevant to split

- AgentRun still imports Lifecycle implementation/value DTOs heavily: `AgentRunRuntimeAddress` is defined under `lifecycle/surface/surface_projector.rs` and imported by AgentRun runtime surface, delivery selection, mailbox, workspace query, frame construction（`crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:23`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:21`, `crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs:10`, `crates/agentdash-application/src/agent_run/workspace/query.rs:20`）。
- AgentRun workspace query directly depends on `AgentRunLifecycleSurfaceProjector` and currently passes `selection.launch_frame_id` into `AgentRunRuntimeAddress.frame_id` while loading the actual current frame via `selection.current_frame_id`; this can confuse launch evidence frame and current surface frame in the resource surface path（`crates/agentdash-application/src/agent_run/workspace/query.rs:352`, `crates/agentdash-application/src/agent_run/workspace/query.rs:355`, `crates/agentdash-application/src/agent_run/workspace/query.rs:358`, `crates/agentdash-application/src/agent_run/workspace/query.rs:385`）。
- Lifecycle / workflow orchestration imports AgentRun internals: `LifecycleDispatchService` and `AgentNodeLauncher` take/pass `AgentFrameBuilder`; workflow orchestration constructs `LifecycleDispatchService` directly（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:24`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:165`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:397`, `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:13`, `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:159`）。
- Session runtime still imports AgentRun ports/implementation for live adoption and effective capability; this is acceptable as an implementation in the current monolith but must invert through application-ports before RuntimeSession substrate extraction（`crates/agentdash-application/src/session/hub/tool_builder.rs:12`, `crates/agentdash-application/src/session/hub/tool_builder.rs:13`, `crates/agentdash-application/src/session/hub/tool_builder.rs:319`, `crates/agentdash-application/src/session/runtime_builder.rs:21`, `crates/agentdash-application/src/session/runtime_builder.rs:143`）。
- Public facades still expose internals: AgentRun root re-exports `AgentFrameBuilder`, `AgentFrameSurfaceExt`, `RuntimeSurfaceUpdateRequest` and query/update services; Lifecycle root re-exports dispatch/session association/surface projector; `workflow/orchestration` publicly exports reducer and executor launcher（`crates/agentdash-application/src/agent_run/mod.rs:45`, `crates/agentdash-application/src/agent_run/mod.rs:46`, `crates/agentdash-application/src/agent_run/mod.rs:58`, `crates/agentdash-application/src/lifecycle/mod.rs:25`, `crates/agentdash-application/src/lifecycle/mod.rs:50`, `crates/agentdash-application/src/lifecycle/mod.rs:70`, `crates/agentdash-application/src/workflow/orchestration/mod.rs:15`, `crates/agentdash-application/src/workflow/orchestration/mod.rs:19`）。

## Required Review Outputs

### 1. AgentRun crate/module 应拥有的 use case、query/update facade、DTO/ports 粒度

AgentRun 应拥有以下 use cases：

- Current runtime surface query：从 `runtime_session_id` 或 run/agent current delivery 解析 `RuntimeSessionExecutionAnchor -> LifecycleRun/LifecycleAgent -> current AgentFrame`，返回 closed DTO，不暴露 `AgentFrame` entity。
- Resource/workspace surface query：以 current AgentFrame typed VFS 为 base，并通过 Lifecycle surface projection port 安装 lifecycle mount / node evidence / message stream projection；该 facade 应返回 `AgentRunResourceSurface`，调用方不应直接使用 Lifecycle projector。
- Effective capability/admission：统一输出 final visible capability、VFS、MCP、visible workspace modules、grant projection 和 tool admission decision。
- Runtime surface update：Canvas、Permission、MCP、VFS、WorkspaceModule、SkillInventory、AgentProcedureContractChanged 只提交 typed `RuntimeSurfaceUpdateRequest`；AgentRun 内部决定是否写 AgentFrame revision、是否调用 active runtime adopter。
- Delivery/runtime command target selection：通过 `LifecycleAgent.current_delivery` 和 anchor 校验选出 current runtime target，供 mailbox/workspace command surface 使用。
- Conversation/mailbox/workspace command read models：保留为 AgentRun public surface，但其错误类型和 shared address DTO 不应来自 Lifecycle implementation module。

AgentRun 应拥有或对外提供的 DTO/ports：

- `AgentRunRuntimeAddress { run_id, agent_id, frame_id }` 应从 `lifecycle/surface/surface_projector.rs` 移到 AgentRun facade 或 `agentdash-application-ports`，因为它表达 AgentRun current surface address，不是 Lifecycle implementation detail（当前定义在 `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:23`）。
- `AgentRunRuntimeSurface` 应保留 `runtime_session_id`、run/project/agent id、`runtime_address`、`launch_evidence_frame_id`、`current_surface_frame_id`、revision、capability/VFS/MCP/backend anchor/provenance/closure（当前 DTO 已满足 frame id 区分：`crates/agentdash-application/src/agent_run/runtime_surface.rs:419`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:424`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:425`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:426`）。
- `AgentRunRuntimeSurfaceQueryPort` 可以继续作为 AgentRun internal/application-facing port；`RuntimeGatewayMcpSurfaceQueryPort` 应保留为 gateway-facing reduced port，已经位于 application-ports（`crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs:61`）。
- 新增 `AgentRunResourceSurfaceQueryPort`，覆盖 current runtime resource surface 与 run/agent resource surface；当前只有 concrete service，无 trait port（`crates/agentdash-application/src/agent_run/runtime_surface.rs:84`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:121`）。
- 新增 `AgentRunEffectiveCapabilityPort`，把 `AgentRunEffectiveCapabilityService` 的 pure projection 与 repository-backed runtime-session grant projection 分离，供 RuntimeSession tool assembly 和 business tools 调用。
- `AgentRunFrameSurfaceUpdatePort` / `AgentRunFrameSurfaceCommandPort` 应成为 Canvas/Permission/WorkspaceModule/API 的唯一写入口；`AgentFrameBuilder`、`AgentFrameSurfaceExt`、`AgentFrameRepository` 只留在 AgentRun internal construction/update adapters，不从 AgentRun root public facade 暴露。
- `AgentRunActiveRuntimeSurfaceAdopter` 和 `AgentFrameRuntimeTarget` 应进入 application-ports 的 runtime-session adoption module，或者至少作为 AgentRun-owned port + RuntimeSession implementation adapter；RuntimeSession substrate extraction 时不能从 runtime-session crate import AgentRun implementation。

### 2. Lifecycle crate/module 应拥有的 orchestration/materialization/reducer 粒度

Lifecycle 应拥有以下 orchestration/materialization/reducer 粒度：

- LifecycleRun command / dispatch：创建或复用 `LifecycleRun`、`LifecycleAgent`、subject association、gate、lineage，并为 plain / graph-backed dispatch 写 anchor/current delivery。
- Orchestration activation：把 compiled `OrchestrationPlanSnapshot` materialize 为 `OrchestrationInstance`，entry nodes 进入 Ready queue。
- Runtime reducer：`OrchestrationRuntimeEvent -> OrchestrationInstance -> LifecycleRun.orchestrations[]`，包括 `NodeStarted/Completed/Failed/Cancelled/Blocked`、state exchange、transition activation、ready queue 更新和 idempotent terminal handling。
- Scheduler/executor orchestration：drain ready nodes、function/local effect/human gate/AgentCall launch。由于这些逻辑读写 `LifecycleRun.orchestrations[]`，`workflow/orchestration/runtime.rs`、`ready_node.rs`、`executor_launcher.rs`、`agent_node_launcher.rs` 在物理拆分时应被视作 Lifecycle runtime implementation，而不是一个独立 workflow definition crate。
- Workflow AgentCall materialization：`LifecycleDispatchService::materialize_workflow_agent_node` 负责用同一套控制面事实创建 `LifecycleAgent`、AgentFrame launch/effective surface、RuntimeSession、RuntimeSessionExecutionAnchor，并返回 `AgentRuntimeRefs` + delivery runtime ref；node state transition 仍由 launcher 提交 `NodeStarted` event。
- Lifecycle surface projection：生命周期 mount / orchestration node evidence / message stream projection 可以由 Lifecycle-owned implementation 提供，但 AgentRun resource surface 应通过 projection port 消费，而不是 import concrete projector。

Lifecycle 不应拥有：

- AgentRun current surface query / effective capability / admission final view。
- AgentFrame runtime surface update/write facade 的 public ownership；Lifecycle materialization 可以通过 AgentRun frame construction port 请求 frame composition。
- RuntimeSession storage/event/turn implementation；Lifecycle 只依赖 RuntimeSession creation/delivery ports。
- API route DTO。

### 3. AgentRun 与 Lifecycle 之间应通过哪些 port/DTO 连接，哪些直接引用必须消除

推荐连接 DTO/ports：

- Shared DTO: `AgentRunRuntimeAddress`、`MessageStreamProjectionRef`、`OrchestrationNodeEvidenceRef`、`OrchestrationNodeProjectionInput`、`AgentRuntimeRefs`。这些应在 `agentdash-application-ports` 或明确的 `agentdash-domain` value object facade 中，避免 AgentRun 从 Lifecycle implementation import。
- `LifecycleSurfaceProjectionPort`：AgentRun resource surface facade 提交 base VFS、AgentRun address、message stream ref、optional node evidence/projection、skill policy；Lifecycle implementation 返回 `AgentRunLifecycleSurface`。当前 `AgentRunLifecycleSurfaceProjector` 可作为该 port 的实现（`crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:209`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:224`, `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs:257`）。
- `AgentRunFrameConstructionPort`：Lifecycle dispatch / AgentCall materialization 提交 `FrameConstructionCommand::LifecycleAgentProcedure` 或更窄的 materialization request，AgentRun frame construction 返回 frame id / revision / launch envelope facts。不要把 `AgentFrameBuilder` 传过 Lifecycle 边界。
- `LifecycleAgentMaterializationPort` 或 `LifecycleDispatchPort`：ProjectAgent start / AgentRun commands 请求 Lifecycle materialize run/agent/frame/session/anchor，返回 `ExecutionDispatchResult` / `AgentRuntimeRefs` / delivery runtime ref；AgentRun 不应直接 new `LifecycleDispatchService`。
- `RuntimeSessionCreationPort`：Lifecycle 依赖 port 创建/attach RuntimeSession；RuntimeSession implementation 实现该 port。当前 `RuntimeSessionCreator` 在 Lifecycle 文件内，应该上移到 ports（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:43`）。
- `ActiveRuntimeSurfaceAdoptionPort`：AgentRun update 写完 AgentFrame 后请求 RuntimeSession implementation 同步 live runtime cache/tools/hook runtime。该 port 应由 RuntimeSession substrate 实现，不应让 RuntimeSession crate import AgentRun implementation。

必须消除或收窄的直接引用：

- AgentRun -> Lifecycle concrete DTO/import：`AgentRunRuntimeAddress`、`WorkflowApplicationError`、`AgentRunLifecycleSurfaceProjector`、`run_view_builder` read model types 不应作为 AgentRun crate 的 implementation imports；至少 shared DTO/error 应移到 common application error/ports（`crates/agentdash-application/src/agent_run/runtime_surface.rs:21`, `crates/agentdash-application/src/agent_run/workspace/query.rs:13`, `crates/agentdash-application/src/agent_run/workspace/query.rs:16`, `crates/agentdash-application/src/agent_run/workspace/query.rs:20`）。
- AgentRun -> Lifecycle service construction：`project_agent_start.rs` 直接 new `LifecycleDispatchService`；应改为依赖 `LifecycleDispatchPort`（`crates/agentdash-application/src/agent_run/project_agent_start.rs:342`）。
- Lifecycle/workflow -> AgentRun builder internals：`LifecycleDispatchService` 与 `AgentNodeLauncher` 直接 import/pass `AgentFrameBuilder`；应通过 AgentRun frame construction port 替代（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:24`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:165`, `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:13`）。
- Workflow -> Lifecycle concrete service construction：`AgentNodeLauncher` 直接构造 `LifecycleDispatchService`；如果 workflow/orchestration runtime 留在 Lifecycle crate 内则可接受，否则需要 `WorkflowAgentNodeMaterializer` port（`crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:159`）。
- Lifecycle -> Session implementation：`dispatch_service.rs` 同时定义 `RuntimeSessionCreator` 和 `SessionPersistenceRuntimeSessionCreator`，并 import session DTO；adapter 必须移到 RuntimeSession implementation/composition root（`crates/agentdash-application/src/lifecycle/dispatch_service.rs:25`, `crates/agentdash-application/src/lifecycle/dispatch_service.rs:51`）。
- Session -> AgentRun implementation：SessionHub 实现 adopter 时直接 import AgentRun target/effective capability/surface extension；未来 RuntimeSession crate 应只 import adoption port DTO，具体 AgentRun effective capability 由 port 注入或在 composition root 组合（`crates/agentdash-application/src/session/hub/tool_builder.rs:12`, `crates/agentdash-application/src/session/hub/tool_builder.rs:14`, `crates/agentdash-application/src/session/hub/tool_builder.rs:15`）。
- AgentRun root / frame root public re-export `AgentFrameBuilder`、`AgentFrameSurfaceExt`、`RuntimeSurfaceUpdateRequest` 等内部 primitive；物理 crate 前应收紧为 facade/ports（`crates/agentdash-application/src/agent_run/mod.rs:45`, `crates/agentdash-application/src/agent_run/mod.rs:46`, `crates/agentdash-application/src/agent_run/mod.rs:58`, `crates/agentdash-application/src/agent_run/frame/mod.rs:10`, `crates/agentdash-application/src/agent_run/frame/mod.rs:23`）。

### 4. 与 RuntimeSession 的关系：谁依赖端口、谁实现 substrate，anchor 如何流动

Dependency direction：

```text
AgentRun / Lifecycle / RuntimeGateway
  -> application-ports DTO/traits
  -> domain workflow entities/repositories + spi types

RuntimeSession implementation
  -> application-ports DTO/traits + domain/spi
  -> implements RuntimeSessionCreationPort / DeliveryCommandPort / ActiveRuntimeSurfaceAdoptionPort

API/local composition root
  -> wires concrete AgentRun, Lifecycle, RuntimeSession, RuntimeGateway implementations
```

Port dependency ownership：

- Lifecycle depends on `RuntimeSessionCreationPort` to create/attach delivery substrate, then immediately writes anchor and current delivery binding. It must not depend on `SessionPersistence` directly after split.
- AgentRun depends on `ActiveRuntimeSurfaceAdoptionPort` after writing AgentFrame revision, and on delivery/message command ports for mailbox/steering/start commands. It must not query SessionHub for current VFS/MCP/capability.
- RuntimeGateway depends on gateway-facing current MCP surface port, already in `application-ports`; it should never depend on AgentRun implementation（`crates/agentdash-application/src/runtime_gateway/mcp_access.rs:23`）。
- RuntimeSession implementation owns event log, turn processing, runtime registry, connector continuation, tool cache/live adoption mechanics, but current business surface remains AgentRun/Lifecycle-owned.

Anchor flow：

1. Dispatch/materialization requests RuntimeSession creation through port.
2. Lifecycle creates launch/effective AgentFrame through AgentRun construction boundary or current internal builder.
3. Lifecycle writes `RuntimeSessionExecutionAnchor(runtime_session_id, run_id, launch_frame_id, agent_id, optional orchestration_id/node_path/attempt)`.
4. Lifecycle copies anchor into `LifecycleAgent.current_delivery` via `bind_current_delivery_from_anchor`.
5. AgentRun current surface query starts from runtime session id, reads anchor, run, agent, current AgentFrame, and returns DTO with both `launch_evidence_frame_id` and `current_surface_frame_id`.
6. RuntimeSession terminal callback starts from runtime session id, resolves anchor/orchestration node, loads `LifecycleRun`, builds `OrchestrationRuntimeEvent`, applies reducer, persists updated `LifecycleRun`, then drains ready nodes.

Important constraint：`launch_frame_id` is delivery evidence; `current_surface_frame_id` is effective model-visible surface. Resource surface and workspace panels must use current frame for VFS/capability and only preserve launch frame as provenance. The current `AgentRunRuntimeSurface` DTO already models this correctly（`crates/agentdash-application/src/agent_run/runtime_surface.rs:425`, `crates/agentdash-application/src/agent_run/runtime_surface.rs:426`），but `agent_run/workspace/query.rs` still passes `selection.launch_frame_id` into `AgentRunRuntimeAddress.frame_id` while reading current frame by `selection.current_frame_id`（`crates/agentdash-application/src/agent_run/workspace/query.rs:358`, `crates/agentdash-application/src/agent_run/workspace/query.rs:385`）。该路径应修正为 current frame id，并把 launch frame 保留在 evidence/provenance 字段。

### 5. 对 split draft 的补充/修正建议

The draft direction is correct: physical crate extraction should wait until ports/visibility/import cleanup has made the intended graph visible（`.trellis/tasks/06-24-release-crate-split-draft/design.md:5`, `.trellis/tasks/06-24-release-crate-split-draft/design.md:46`, `.trellis/tasks/06-24-release-crate-split-draft/design.md:52`, `.trellis/tasks/06-24-release-crate-split-draft/design.md:57`）。建议补充以下具体条款：

- 在 `design.md` 的 candidate crates 表中明确：`workflow/orchestration/runtime.rs`、`ready_node.rs`、`executor_launcher.rs`、`agent_node_launcher.rs` 是 Lifecycle runtime/reducer implementation；workflow definition/compiler/script preflight 可保留为 workflow module/crate。否则 Workflow 与 Lifecycle 会在 AgentCall materialization 上形成不必要的双向概念依赖。
- 在 Wave 1 ports 中增加具体 modules：
  - `agent_run_surface`: `AgentRunRuntimeAddress`、current surface DTO、resource surface DTO、`AgentRunRuntimeSurfaceQueryPort`、`AgentRunResourceSurfaceQueryPort`。
  - `lifecycle_surface`: `MessageStreamProjectionRef`、`OrchestrationNodeEvidenceRef`、`OrchestrationNodeProjectionInput`、`LifecycleSurfaceProjectionPort`。
  - `runtime_session_delivery`: `RuntimeSessionCreationRequest`、`RuntimeSessionCreationPort`、delivery command/adoption ports、`AgentFrameRuntimeTarget`。
  - 保留已有 `runtime_gateway_mcp_surface`，不要把 gateway reduced DTO 反向扩成完整 AgentRun DTO。
- 把 `AgentRunRuntimeAddress` 从 Lifecycle projector module 移出；这是 AgentRun address/shared port DTO，不应让 AgentRun crate 为 address 类型依赖 Lifecycle implementation。
- 增加 frame-id invariant：所有 current surface/resource surface DTO 必须同时携带 launch evidence frame 和 current surface frame；VFS/capability/MCP 读取使用 current surface frame，anchor/debug/provenance 使用 launch evidence frame。
- 把 `RuntimeSessionCreator` 从 `lifecycle/dispatch_service.rs` 移到 ports；`SessionPersistenceRuntimeSessionCreator` 移到 RuntimeSession implementation/composition root。draft 目前只写 “RuntimeSession delivery/adoption ports”，应明确该 port 是 Lifecycle dispatch 的前置 blocker。
- 把 `AgentRunActiveRuntimeSurfaceAdopter` 从 AgentRun implementation 或 session import 中抽出到 ports；SessionHub/RuntimeSession substrate 只实现 adopter，不依赖 AgentRun concrete module。
- 把 `AgentRunFrameConstructionPort` / `WorkflowAgentNodeMaterializer` 写入 AgentRun-Lifecycle boundary。当前 Lifecycle 直接传 `AgentFrameBuilder`，这在物理拆分时会把 AgentRun frame internals 泄露给 Lifecycle。
- 在 blocking conditions 增加 rg gates：
  - `rg -n "crate::lifecycle::.*AgentRunRuntimeAddress|crate::lifecycle::surface::surface_projector" crates/agentdash-application/src/agent_run`
  - `rg -n "AgentFrameBuilder" crates/agentdash-application/src/lifecycle crates/agentdash-application/src/workflow/orchestration`
  - `rg -n "use crate::session|SessionPersistenceRuntimeSessionCreator" crates/agentdash-application/src/lifecycle`
  - `rg -n "use crate::agent_run" crates/agentdash-application/src/session`
  - `rg -n "selection\\.launch_frame_id" crates/agentdash-application/src/agent_run/workspace/query.rs crates/agentdash-api/src`
- 在 Wave 3 前新增 “current/resource surface correctness gate”：AgentRun resource surface must use current frame id for `AgentRunRuntimeAddress.frame_id`; launch frame id may appear only in provenance/evidence.
- 在 public visibility cleanup 中明确：`AgentFrameBuilder`、`AgentFrameSurfaceExt`、`surface::mount`、`workflow/orchestration/runtime` reducer internals不作为 external crate public API；对外只暴露 facade/service/port DTO。

## External references

- None. This review used repository code, Trellis task artifacts, and local specs only.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell. The user supplied `.trellis/tasks/06-24-release-crate-split-draft` explicitly, so this report was written there.
- No production code, Cargo workspace config, specs, or migrations were modified.
- I did not run compile/tests because this was a research-only architecture review.
- This review focused on requested AgentRun/Lifecycle/workflow surfaces. It did not audit every API route or every session internal call site.
