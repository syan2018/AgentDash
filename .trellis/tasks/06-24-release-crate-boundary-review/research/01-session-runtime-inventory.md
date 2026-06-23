# Research: session runtime inventory

- Query: 对 `crates/agentdash-application/src/session/**` 做 RuntimeSession 边界盘点，标出 public exports、外部调用点、RuntimeSession substrate 应保留职责、应迁出的 AgentRun/Lifecycle/query/update/API adapter 职责，以及首批 child task。
- Scope: internal
- Date: 2026-06-24

## Findings

### 输入与相关规格

- 任务文件：`.trellis/tasks/06-24-release-crate-boundary-review/prd.md`、`design.md`、`implement.md`、`review-briefs/session-runtime.md`。
- 相关 specs：
  - `.trellis/spec/backend/session/architecture.md`：session 目标是 `RuntimeSession` substrate，保存 turn/tool/event/resume/debug/projection/trace lineage；业务 ownership、permission scope、lifecycle progress、Agent effective surface 不属于 session。
  - `.trellis/spec/backend/session/session-startup-pipeline.md`：启动链路为 `LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> PreparedTurn -> ConnectorAcceptedTurn -> CommittedTurn -> AttachedTurn`，`FrameLaunchEnvelope` 是 frame construction 到 session launch 的唯一 handoff。
  - `.trellis/spec/backend/session/runtime-execution-state.md`：active turn、connector live session、active runtime snapshot 分离；current VFS/MCP/backend anchor/capability/AgentRun target consumer 应走 AgentRun runtime surface query。
  - `.trellis/spec/backend/session/execution-context-frames.md`：`ExecutionContext` 是 connector-facing projection，不是应用事实源。
  - `.trellis/spec/backend/runtime-gateway.md`：RuntimeGateway session MCP actions 由 AgentRun/Lifecycle current runtime surface query 支撑，不应回读 SessionHub/AgentFrame。
  - `.trellis/spec/backend/workflow/architecture.md`：AgentFrame owns runtime surface，RuntimeSession 是 delivery/trace evidence，`RuntimeSessionExecutionAnchor` 是 authoritative backlink。
  - `.trellis/spec/backend/capability/architecture.md`、`.trellis/spec/backend/vfs/architecture.md`、`.trellis/spec/cross-layer/desktop-local-runtime.md`、`.trellis/spec/cross-layer/frontend-backend-contracts.md` 提供 capability/VFS/backend anchor/API contract 边界。

### Files found

`rg --files crates/agentdash-application/src/session` 找到 76 个 session 文件，当前可按职责分组如下：

- `crates/agentdash-application/src/session/mod.rs`：session module public facade，当前 re-export 面过宽。
- `crates/agentdash-application/src/session/runtime_builder.rs`、`runtime_services.rs`、`hub/mod.rs`、`hub/factory.rs`、`hub/facade.rs`、`hub/hook_dispatch.rs`、`hub/runtime_context_transition.rs`、`hub/tool_builder.rs`：runtime 装配、ready gate、service factory、live adoption、hook/transition/tool helper。
- `crates/agentdash-application/src/session/launch/command.rs`、`connector_start.rs`、`commit.rs`、`deps.rs`、`ingestion.rs`、`mod.rs`、`orchestrator.rs`、`plan.rs`、`planner.rs`、`preparation.rs`、`service.rs`：launch pipeline，从 `LaunchCommand` 接入，到 `FrameLaunchEnvelope`、tool assembly、connector start、commit 和 stream attach。
- `crates/agentdash-application/src/session/runtime_registry.rs`、`runtime_control.rs`、`runtime_commands.rs`、`runtime_transition_service.rs`、`turn_supervisor.rs`、`turn_processor.rs`、`control.rs`、`core.rs`、`stall_detector.rs`：live runtime/turn/session 控制与运行态 command/outbox。
- `crates/agentdash-application/src/session/persistence.rs`、`eventing.rs`、`effects_service.rs`、`terminal_effects.rs`、`terminal_cache.rs`、`tool_result_cache.rs`、`post_turn_handler.rs`、`continuation.rs`、`branching.rs`、`compaction_checkpoint.rs`、`title_generator.rs`、`title_service.rs`：RuntimeSession 事件、持久化、terminal/tool result、turn 结束处理、projection/read-model 辅助。
- `crates/agentdash-application/src/session/context.rs`、`context_frame.rs`、`context_projector.rs`、`context_usage_marking.rs`、`context_usage_projection.rs`、`assignment_context_frame.rs`、`auto_resume_context_frame.rs`、`compaction_context_frame.rs`、`guidelines_context_frame.rs`、`identity_context_frame.rs`、`pending_action_context_frame.rs`：connector context frame 与 runtime trace projection。
- `crates/agentdash-application/src/session/dimension/*.rs`、`baseline_capabilities.rs`、`tool_assembly.rs`、`prompt_vfs.rs`、`path_policy.rs`：capability/VFS/MCP/tool surface projection 和 runtime tool assembly。
- `crates/agentdash-application/src/session/hooks_service.rs`、`hook_delegate.rs`、`hook_events.rs`、`hook_messages.rs`、`hook_injection_sink.rs`、`hub_support.rs`：hook runtime delivery、hook trigger/event envelope、terminal hook 绑定。
- `crates/agentdash-application/src/session/bootstrap.rs`、`construction.rs`、`construction_planner.rs`、`plan.rs`、`types.rs`：legacy construction/planning/types。`construction.rs` 为 `pub(crate)`，但 `types.rs` 当前承载多种跨边界 DTO。

### Public exports

- `session/mod.rs` 把多数 root module 设为 `pub mod`，包括 `baseline_capabilities`、`bootstrap`、`construction_planner`、`context`、`runtime_transition_service`、`continuation`、`control`、`core`、`effects_service`、`eventing`、`hooks_service`、`launch`、`persistence`、`runtime_builder`、`runtime_commands`、`runtime_control`、`runtime_services`、`terminal_*`、`title_*`、`tool_result_cache`、`turn_processor`、`types`；`hub`、`tool_assembly`、`dimension` 是 `pub(crate)`（`crates/agentdash-application/src/session/mod.rs:3`、`:21`、`:30`、`:53`、`:57`）。
- `session/mod.rs` 还 re-export AgentRun/Lifecycle 类型：`AgentFrameHookRuntime`、`WorkflowApplicationError`（`crates/agentdash-application/src/session/mod.rs:59`、`:60`）。这是反向依赖泄漏：session facade 暴露了 AgentRun frame hook runtime 和 lifecycle error。
- service re-export 覆盖 core/control/eventing/effects/hook/launch/runtime/title/tool result/turn processor/persistence（`crates/agentdash-application/src/session/mod.rs:74`、`:75`、`:77`、`:80`、`:82`、`:88`、`:100`、`:105`、`:107`、`:111`、`:112`、`:118`）。
- `types.rs` re-export `agentdash_spi::CapabilityState` 和 session persistence 类型（`crates/agentdash-application/src/session/types.rs:7`、`:10`），并定义 `AgentFrameRuntimeTarget`（`:67`）与 `SessionExecutionState`（`:247`）。`AgentFrameRuntimeTarget` 被 AgentRun、permission、workspace_module、companion、hooks 等多处导入，语义上属于 AgentRun current delivery runtime target，而非 RuntimeSession substrate。
- 结论：public facade 应收敛为 RuntimeSession substrate ports 和少量 application-facing use case；`AgentFrameRuntimeTarget`、AgentRun frame/hook target、WorkflowApplicationError、baseline capability/dimension 类型不应由 `session` 作为公共入口继续分发。

### Hub and RuntimeInner

- `SessionRuntimeInner` 的模块注释已经承认它是“装配对象与尚待下沉的 session 内部实现”，最终应只保留装配/ready gate（`crates/agentdash-application/src/session/hub/mod.rs:1`）。
- `SessionRuntimeInner` 字段覆盖 connector、hook provider、runtime registry、turn supervisor、stores、persistence、VFS、skill discovery、terminal callback、runtime tool provider、MCP discovery、backend execution deps、`AgentFrameRepository`、`RuntimeSessionExecutionAnchorRepository`、`LifecycleAgentRepository`、permission repo、mailbox runtime adapter、lifecycle gate repo（`crates/agentdash-application/src/session/hub/mod.rs:46`、`:61`、`:87`、`:88`、`:89`）。其中 AgentFrame/Lifecycle/Permission/Mailbox 是跨边界依赖，应经 AgentRun/Lifecycle/application ports 注入，而不是由 session substrate 直接拥有业务事实。
- `runtime_builder.active_runtime_surface_adopter()` 直接把 `SessionRuntimeInner` 暴露成 `AgentRunActiveRuntimeSurfaceAdopter`（`crates/agentdash-application/src/session/runtime_builder.rs:143`），当前是必要桥接，但目标形态应隐藏为 AgentRun update service 的内部 live runtime adapter。
- `SessionCapabilityService` 在当前 `crates/**` 中没有定义；只在任务文档和旧 spec 文本中出现。当前对应行为主要落在 `SessionRuntimeInner::adopt_persisted_frame_revision_into_active_runtime` 与 `AgentRunRuntimeSurfaceUpdateService` 上（`crates/agentdash-application/src/session/hub/tool_builder.rs:79`、`:318`；`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:23`、`:62`）。

### Launch pipeline

- `launch/mod.rs` 只对外导出 command/plan/service（`crates/agentdash-application/src/session/launch/mod.rs:13`、`:27`）。这是合理的 facade 方向，但 `session/mod.rs` 又把它整体公开，外部仍可直接构造 `LaunchCommand`。
- `LaunchSource` 把 HTTP、LifecycleAgent user message、hook auto resume、companion dispatch、workflow/routine/local relay 等请求意图放在 session 内（`crates/agentdash-application/src/session/launch/command.rs:10`、`:22`）。这些 source 是 AgentRun/Lifecycle/API adapter intent，session 可消费已归一的 delivery command，但不应长期作为业务入口枚举的归属地。
- AgentRun 已定义 `FrameLaunchEnvelopeProvider`，注释明确 “session launch 的唯一输入”（`crates/agentdash-application/src/agent_run/frame/launch_envelope_provider.rs:66`、`:67`、`:70`）。`SessionLaunchOrchestrator` 通过 provider 构建 envelope（`crates/agentdash-application/src/session/launch/orchestrator.rs:89`），这是正确边界。
- `LaunchPlan` 根据 `FrameLaunchEnvelope` 构造 `ExecutionContext`，把 runtime backend anchor 写入 `ExecutionSessionFrame`（`crates/agentdash-application/src/session/launch/plan.rs:132`、`:162`、`:278`）。这属于 RuntimeSession launch substrate。
- 越界点：`SessionLaunchOrchestrator` 仍通过 runtime session -> AgentFrame -> LifecycleAgent 判断/标记 bootstrap（`crates/agentdash-application/src/session/launch/orchestrator.rs:282`、`:294`、`:307`、`:318`）。`TurnCommitter` 仍写入 AgentFrame revision、更新 LifecycleAgent delivery binding、同步 hook runtime target（`crates/agentdash-application/src/session/launch/commit.rs:224`、`:296`）。这些是 AgentRun/Lifecycle 事实更新，应迁到 AgentRun frame surface service 或 Lifecycle launch/commit adapter。

### Runtime surface, adoption, and backend anchor

- AgentRun current surface query 已存在：`AgentRunRuntimeSurfaceQueryPort` 和 `AgentRunRuntimeSurfaceQuery` 从 execution anchor、run、agent、current frame 投影当前 capability/VFS/MCP/backend anchor（`crates/agentdash-application/src/agent_run/runtime_surface.rs:57`、`:81`、`:208`、`:272`、`:298`、`:308`）。这是 current surface/backend anchor 的目标入口。
- API current surface helper 已通过 AgentRun query 实现：`resolve_current_runtime_surface_for_api`、`resolve_current_runtime_surface_with_backend_for_api`、`resolve_runtime_session_resource_vfs_for_api`（`crates/agentdash-api/src/session_construction.rs:47`、`:68`、`:89`、`:146`）。文件名仍叫 `session_construction.rs`，但职责已经是 API adapter for AgentRun runtime surface。
- RuntimeGateway MCP access 已迁到 current surface query：`CurrentSurfaceRuntimeMcpAccess` 持有 `AgentRunRuntimeSurfaceQueryPort`，`list_tools` 调 `current_runtime_surface_with_backend`，再构建 `McpToolDiscoveryRequest`（`crates/agentdash-application/src/runtime_gateway/mcp_access.rs:23`、`:39`、`:59`、`:95`、`:104`）。
- RuntimeSession 内部 live turn tool assembly 仍应保留：`assemble_tool_surface_for_execution_context` 用 active `ExecutionContext` 组装 runtime tools/MCP tools，并通过 `require_runtime_backend_anchor("tool_assembly", Some(session_id))` 明确要求 connector-facing anchor（`crates/agentdash-application/src/session/tool_assembly.rs:17`、`:42`、`:59`）。这是 launch/hot refresh substrate，不是 API current surface query。
- workspace module runtime tool invoke 同样从 active `ExecutionContext` 要求 backend anchor（`crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:270`）。这属于运行中 tool 调用，不应替代 API current surface query。
- active runtime adoption 当前路径：AgentRun update service 创建/写入新 AgentFrame revision 后调用 adopter（`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:71`、`:124`、`:147`），adopter 由 session builder 提供（`crates/agentdash-api/src/bootstrap/session.rs:219`），具体实现刷新 connector tools、runtime registry、session profile、active turn snapshot，并记录 context transition（`crates/agentdash-application/src/session/hub/tool_builder.rs:79`、`:185`、`:191`、`:195`、`:210`、`:318`）。行为应保留为 live RuntimeSession adapter，入口应只由 AgentRun update use case 持有。

### External dependency entrypoints

- API bootstrap/app state：`ServiceSet` 仍公开 `SessionCoreService`、`SessionEventingService`、`SessionRuntimeService`、`SessionControlService`、`SessionLaunchService`、`SessionRuntimeTransitionService`，同时公开 `runtime_surface_update` 和 `RuntimeGateway`（`crates/agentdash-api/src/app_state.rs:24`、`:55`、`:63`、`:104`）。`bootstrap/session.rs` 组装 `AgentRunRuntimeSurfaceQuery`、`AgentRunRuntimeSurfaceUpdateService`，并把 session adopter 注入 update service（`crates/agentdash-api/src/bootstrap/session.rs:206`、`:214`、`:219`、`:257`）。
- API routes：canvas route 使用 API current surface helper 和 RuntimeGateway surface（`crates/agentdash-api/src/routes/canvases.rs:44`、`:530`、`:588`）；extension/terminal route 使用 backend-required current surface helper（`crates/agentdash-api/src/routes/extension_runtime.rs:18`、`:133`、`:192`；`crates/agentdash-api/src/routes/terminals.rs:17`、`:271`）；VFS resolver 仍在 route 层拼接 session resource VFS 和 AgentRun runtime surface（`crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:21`、`:212`、`:280`）；permission grants route 通过 runtime surface update service 更新有效 surface（`crates/agentdash-api/src/routes/permission_grants.rs:229`、`:270`）。
- AgentRun mailbox/delivery/workspace：mailbox 依赖 `SessionCoreService`、`SessionControlService`、`SessionEventingService`、`SessionLaunchService` 和 `SessionExecutionState`（`crates/agentdash-application/src/agent_run/mailbox.rs:34`、`:179`、`:182`）；message delivery 直接构造 `LaunchCommand` 并调用 `SessionLaunchService`（`crates/agentdash-application/src/agent_run/message_delivery.rs:8`、`:28`、`:53`）；workspace query/command policy 读 session state 与 control service（`crates/agentdash-application/src/agent_run/workspace/query.rs:24`、`:41`；`crates/agentdash-application/src/agent_run/workspace/command_policy.rs:15`、`:22`）。
- AgentRun frame/capability/update：`AgentRunFrameSurfaceService` 的 doc 已说明业务模块提交 typed construction/update intent，不拥有 `AgentFrameBuilder` 或 live adoption timing（`crates/agentdash-application/src/agent_run/frame/surface_service.rs:1`、`:88`、`:253`）；但它仍从 `session` 导入 `AgentFrameRuntimeTarget`（`:16`、`:165`、`:193`）。`AgentRunActiveRuntimeSurfaceAdopter` trait 也从 `session::types::AgentFrameRuntimeTarget` 取 target（`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:19`、`:23`、`:26`）。
- Business update flows：workspace module 用 `runtime_surface_update.effective_capability_view_for_delivery_runtime` 和 `RuntimeSurfaceUpdateRequest::CanvasBindingChanged`（`crates/agentdash-application/src/workspace_module/tools.rs:92`、`:672`、`:876`）；canvas runtime helper 提交 canvas mount exposure（`crates/agentdash-application/src/canvas/runtime_surface.rs:10`、`:52`）；permission runtime surface update 仍使用 `AgentFrameRuntimeTarget` 构造 adoption target（`crates/agentdash-application/src/permission/runtime_surface_update.rs:21`、`:36`、`:173`）；companion tools 仍导入 `AgentFrameRuntimeTarget` 和 `LaunchCommand`（`crates/agentdash-application/src/companion/tools.rs:36`、`:480`、`:689`）。
- Runtime tools/reconcile/test support：runtime tool provider 注入 session core/eventing/control/launch/hook/runtime transition（`crates/agentdash-application/src/runtime_tools/provider.rs:12`、`:39`、`:44`）；reconcile 依赖 `SessionRuntimeService`（`crates/agentdash-application/src/reconcile/boot.rs:12`、`:26`；`crates/agentdash-application/src/reconcile/terminal_cancel.rs:22`、`:29`）；test support 仍直接暴露 session services（`crates/agentdash-application/src/test_support/agent_run_steering.rs:11`、`:35`）。
- Domain anchors：`RuntimeSessionExecutionAnchor` 与 repository 是 AgentRun/Lifecycle 到 RuntimeSession 的 backlink 合同（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29`；`crates/agentdash-domain/src/workflow/repository.rs:136`）。Session 可以记录/消费 delivery trace id，但不应直接成为 AgentFrame current surface 的查询入口。

### Code patterns

- 正向模式：AgentRun frame construction 输出 `FrameLaunchEnvelope`，session launch 消费 envelope（`crates/agentdash-application/src/agent_run/frame/runtime_launch.rs:1`、`:237`；`crates/agentdash-application/src/session/launch/orchestrator.rs:89`）。
- 正向模式：API/RuntimeGateway current surface 通过 `AgentRunRuntimeSurfaceQuery` 解析 backend anchor 和 MCP surface，不碰 SessionHub（`crates/agentdash-api/src/session_construction.rs:47`、`:68`；`crates/agentdash-application/src/runtime_gateway/mcp_access.rs:23`、`:39`）。
- 正向模式：运行中 tool assembly 使用 active turn `ExecutionContext` 和 `require_runtime_backend_anchor`，只服务 connector live session（`crates/agentdash-application/src/session/tool_assembly.rs:17`、`:42`）。
- 越界模式：session launch/commit 直接读写 AgentFrame/LifecycleAgent（`crates/agentdash-application/src/session/launch/orchestrator.rs:282`、`:307`；`crates/agentdash-application/src/session/launch/commit.rs:224`、`:296`）。
- 越界模式：AgentRun/permission/workspace/companion 从 `session` 导入 `AgentFrameRuntimeTarget`，说明 target 类型归属错位（`crates/agentdash-application/src/agent_run/frame/surface_service.rs:16`；`crates/agentdash-application/src/permission/runtime_surface_update.rs:21`；`crates/agentdash-application/src/workspace_module/visibility.rs:104`；`crates/agentdash-application/src/companion/tools.rs:36`）。
- 越界模式：`session/mod.rs` re-export AgentRun/Lifecycle 类型，扩大了 session crate/module 的依赖方向（`crates/agentdash-application/src/session/mod.rs:59`、`:60`）。

### RuntimeSession substrate 应保留

- RuntimeSession metadata/store/event/projection/lineage：`SessionCoreService`、`SessionEventingService`、`SessionPersistence`、lineage/projection/read model 相关文件应保留，但对外 facade 应只暴露明确 use case/ports（`crates/agentdash-application/src/session/core.rs:11`；`eventing.rs:37`；`persistence.rs:20`）。
- Live runtime/turn delivery：`SessionRuntimeRegistry`、`TurnSupervisor`、connector active session、active turn snapshot、cancel/stall/reconcile 是 RuntimeSession 的运行态基础设施（`crates/agentdash-application/src/session/runtime_registry.rs:15`；`turn_supervisor.rs:228`；`runtime_control.rs:14`；`stall_detector.rs:22`）。
- Launch substrate：从 `FrameLaunchEnvelope` 到 `ExecutionContext`、tool assembly、connector start、event commit、stream attach 的 pipeline 应保留在 RuntimeSession，前提是 AgentFrame/Lifecycle 写入被移出（`crates/agentdash-application/src/session/launch/plan.rs:132`；`preparation.rs:59`；`connector_start.rs`；`ingestion.rs`）。
- Runtime delivery commands/transition outbox：`SessionRuntimeTransitionService` 注释已经限定为 delivery outbox、turn-boundary pending transition、active runtime snapshot，这部分可保留（`crates/agentdash-application/src/session/runtime_transition_service.rs:18`、`:32`、`:46`、`:123`）。
- Live adoption adapter：connector tool refresh、active turn/session profile/runtime registry 热更新可保留在 session 内部，但只作为 `AgentRunRuntimeSurfaceUpdateService` 的 adapter，不作为业务/API 可见 service（`crates/agentdash-application/src/session/hub/tool_builder.rs:185`、`:191`、`:195`、`:210`）。
- Hook delivery runtime：hook trigger dispatch、terminal hook binding、hook runtime cache 可作为 RuntimeSession delivery adapter 保留，但 `AgentFrameHookRuntimeTarget`/target resolution 应迁到 AgentRun frame/hook target 层（`crates/agentdash-application/src/session/hooks_service.rs:46`、`:146`）。

### 应迁出职责

- AgentRun current runtime surface query：已经由 `agent_run/runtime_surface.rs` 承担。所有 API route、RuntimeGateway、resource browser、terminal/extension/canvas snapshot 都应只依赖 query facade，不直接依赖 session hub、AgentFrame resolver 或 `ExecutionContext`（`crates/agentdash-application/src/agent_run/runtime_surface.rs:57`；`crates/agentdash-api/src/session_construction.rs:47`）。
- AgentRun runtime surface update/adoption entrypoint：`AgentRunActiveRuntimeSurfaceAdopter` trait 和 `AgentFrameRuntimeTarget` 应迁到 `agent_run` 或 `agent_run::frame`，session 只实现 live adapter（`crates/agentdash-application/src/agent_run/runtime_surface_update.rs:23`；`crates/agentdash-application/src/session/types.rs:67`）。
- AgentFrame write/commit：`TurnCommitter` 中创建/更新 AgentFrame、绑定 LifecycleAgent current delivery、同步 hook runtime target 应由 AgentRunFrameSurfaceService/LaunchCommit adapter 负责，session commit 只记录 RuntimeSession accepted turn/trace（`crates/agentdash-application/src/session/launch/commit.rs:224`、`:296`；`crates/agentdash-application/src/agent_run/frame/surface_service.rs:253`）。
- Lifecycle bootstrap/progress：`SessionLaunchOrchestrator` 中的 `resolve_agent_needs_bootstrap` 和 `mark_agent_bootstrapped` 应迁到 Lifecycle/AgentRun launch adapter；session 只接收 envelope 中已判定的 launch flags（`crates/agentdash-application/src/session/launch/orchestrator.rs:282`、`:307`）。
- API adapter：`crates/agentdash-api/src/session_construction.rs` 职责实际是 current runtime surface API adapter，应改名/迁到 AgentRun/runtime surface adapter；VFS/canvas/extension/terminal route 中的 surface helper 拼装应收敛到该 adapter 或 application facade（`crates/agentdash-api/src/session_construction.rs:47`、`:89`；`routes/vfs_surfaces/resolver.rs:212`、`:280`）。
- Capability/dimension projection：`baseline_capabilities.rs`、`dimension/*`、`CapabilityState` public re-export 更像 capability/AgentRun projection layer；session 内只应保留 active turn delivery 所需的 serialized snapshot/delta application（`crates/agentdash-application/src/session/mod.rs:3`、`:21`；`crates/agentdash-application/src/session/types.rs:7`）。
- Launch request intent：`LaunchCommand` source 枚举包含业务来源，应由 AgentRun/Lifecycle/API adapter 归一为 delivery command；session launch service 可保留消费入口，但 public facade 不应鼓励业务模块直接构造每种 source（`crates/agentdash-application/src/session/launch/command.rs:10`、`:136`）。

### 首批 child task 建议

1. `session-public-facade-tightening`：收敛 `session/mod.rs` public exports；迁出 `AgentFrameRuntimeTarget`、`AgentFrameHookRuntimeTarget`、`AgentFrameHookRuntime`、`WorkflowApplicationError` 的 session re-export；把只供内部装配的 service/module 改为 crate-private 或通过明确 port 暴露。
2. `agentrun-runtime-target-ownership`：在 `agent_run::frame` 或 `agent_run::runtime_surface_update` 定义 AgentRun runtime target/adopter trait；更新 AgentRun、permission、workspace_module、companion、hooks 调用点，session 仅实现 live adapter。
3. `launch-commit-agentrun-boundary`：把 `TurnCommitter` 中 AgentFrame revision 写入、LifecycleAgent current delivery binding、hook runtime target 同步迁到 AgentRunFrameSurfaceService/LaunchCommit adapter；session commit 保留 RuntimeSession accepted turn 和 trace 记录。
4. `launch-lifecycle-bootstrap-boundary`：把 `resolve_agent_needs_bootstrap`、`mark_agent_bootstrapped` 从 `session/launch/orchestrator.rs` 迁到 Lifecycle/AgentRun launch adapter，让 `FrameLaunchEnvelope` 携带 session launch 需要的 final flags。
5. `current-surface-api-adapter-cleanup`：将 `agentdash-api/src/session_construction.rs` 重命名/迁移为 AgentRun runtime surface API adapter，并收敛 canvas/VFS/extension/terminal route 的 current surface helper 入口。
6. `runtime-session-substrate-split-prep`：把 RuntimeSession substrate 清单固化为 crate/module 边界：store/event/projection、live runtime registry、turn supervisor、launch stages、tool assembly、transition outbox、hook delivery adapter；为后续 crate split 建立 ports。
7. `capability-dimension-owner-cleanup`：评估 `baseline_capabilities.rs`、`dimension/*`、runtime capability projection 的最终 owner，保留 session active-turn delivery snapshot，迁出 AgentRun/capability effective surface 相关类型和 public re-export。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件按用户显式给出的 active task path `.trellis/tasks/06-24-release-crate-boundary-review` 写入。
- 未找到 `.trellis/spec/backend/session/index.md`；本次读取的是该目录下的具体 session specs。
- `SessionCapabilityService` 在当前 `crates/**` 中未找到，只有 `.trellis/spec/backend/session/architecture.md:112`、`.trellis/spec/backend/hooks/execution-hook-runtime.md:141` 和任务 brief 中的历史命名；当前等价关注点是 `AgentRunRuntimeSurfaceUpdateService` + `SessionRuntimeInner` live adoption adapter。
- 本次是架构调研，没有修改业务源码，也没有运行测试。
