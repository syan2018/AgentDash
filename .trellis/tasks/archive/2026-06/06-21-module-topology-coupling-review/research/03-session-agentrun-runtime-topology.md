# Research: session-agentrun-runtime-topology

- Query: 盘查 Session / RuntimeSession / AgentRun Workspace / Agent Loop / Mailbox / Hook delegate 的主链路拓扑与耦合点，产出后续 review 应覆盖的问题清单。
- Scope: internal
- Date: 2026-06-21

## Findings

### Files Found

- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md` - 当前 task PRD 仍是占位内容，用户消息是本轮研究的主要任务约束。
- `.trellis/spec/project-overview.md` - 定义 `RuntimeSession` 只承载 event stream / turn / tool / resume / trace，不拥有业务归属或 lifecycle progress truth。
- `.trellis/spec/backend/session/architecture.md` - Session 目标主线与边界：`LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> PreparedTurn -> ConnectorAcceptedTurn -> CommittedTurn -> AttachedTurn`。
- `.trellis/spec/backend/session/runtime-execution-state.md` - 区分 runtime map、active turn、connector live session、backend lease、AgentRun workspace control surface。
- `.trellis/spec/backend/session/agentrun-mailbox.md` - AgentRun mailbox 是 workspace message intake、scheduler、recovery projection 的 durable fact source。
- `.trellis/spec/backend/session/execution-context-frames.md` - `ExecutionContext` 是 connector-facing projection，不是 application fact source。
- `.trellis/spec/backend/session/streaming-protocol.md` - Session NDJSON stream 以 `BackboneEnvelope` 作为 notification。
- `.trellis/spec/cross-layer/backbone-protocol.md` - Backbone 是内部 session event envelope、持久化、NDJSON 和前端消费协议。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 既有过度设计 review，已覆盖 Lifecycle、AgentRun/Session、VFS/Local/Relay、Permission/Contracts/Frontend 的重复事实源问题。
- `crates/agentdash-application/src/session/launch/command.rs` - `LaunchCommand` 与来源 constructors。
- `crates/agentdash-application/src/session/launch/orchestrator.rs` - Session launch stage 编排。
- `crates/agentdash-application/src/session/launch/planner.rs` - `FrameLaunchEnvelope` 到 `LaunchPlan` 的 planner。
- `crates/agentdash-application/src/session/launch/preparation.rs` - 构造 `PreparedTurn`、tool assembly、hook/context frames。
- `crates/agentdash-application/src/session/launch/connector_start.rs` - connector accepted boundary。
- `crates/agentdash-application/src/session/launch/commit.rs` - accepted facts 持久化边界。
- `crates/agentdash-application/src/session/launch/ingestion.rs` - stream adapter attach 边界。
- `crates/agentdash-application/src/session/hub/mod.rs` - `SessionRuntimeInner` 装配对象。
- `crates/agentdash-application/src/session/hub/factory.rs` - session runtime dependency injection / validation。
- `crates/agentdash-application/src/session/hub/tool_builder.rs` - runtime MCP/capability/tool hot update。
- `crates/agentdash-application/src/session/mailbox_delegate.rs` - hook delegate 与 mailbox turn boundary adapter。
- `crates/agentdash-application/src/session/hook_delegate.rs` - `AgentRuntimeDelegate` 的 hook runtime 实现。
- `crates/agentdash-application/src/session/tool_assembly.rs` - runtime tool provider + MCP discovery 组合。
- `crates/agentdash-application/src/session/terminal_effects.rs` - terminal effect outbox 与 terminal callback。
- `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs` - `FrameRuntimeSurface` / `FrameLaunchIntent` / `FrameLaunchEnvelope`。
- `crates/agentdash-application/src/agent_run/frame/construction/mod.rs` - `FrameConstructionService` 以 anchor/run/agent/frame 构造 launch envelope。
- `crates/agentdash-application/src/agent_run/project_agent_start.rs` - ProjectAgent draft start 到 AgentRun mailbox 初始消息。
- `crates/agentdash-application/src/agent_run/message_delivery.rs` - mailbox launch delivery 到 `SessionLaunchService`。
- `crates/agentdash-application/src/agent_run/mailbox.rs` - AgentRun mailbox command intake、scheduler、delivery。
- `crates/agentdash-application/src/agent_run/conversation_snapshot.rs` - `AgentConversationSnapshot` execution / commands / mailbox projection。
- `crates/agentdash-application/src/agent_run/workspace/query.rs` - AgentRun workspace query aggregation。
- `crates/agentdash-application/src/agent_run/workspace/projection.rs` - workspace delivery/runtime command state projection。
- `crates/agentdash-application/src/agent_run/workspace/command_policy.rs` - command precondition policy。
- `crates/agentdash-application/src/agent_run/steering.rs` - direct AgentRun steer service still exists as a parallel internal path candidate.
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs` - Lifecycle dispatch creates run/agent/frame/session/anchor/association facts.
- `crates/agentdash-application/src/workflow/orchestration/runtime.rs` - orchestration runtime event reducer and node status facts.
- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs` - ready node drain and agent/function/human launch orchestration.
- `crates/agentdash-agent-types/src/runtime/delegate.rs` - broad `AgentRuntimeDelegate` trait.
- `crates/agentdash-agent/src/agent_loop.rs` - Pi/Agent loop turn events and delegate callbacks.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - public AgentRun workspace/composer/mailbox/cancel routes.
- `crates/agentdash-api/src/routes/sessions.rs` - RuntimeSession detail/runtime-control/NDJSON routes.
- `crates/agentdash-contracts/src/runtime/workflow.rs` - `AgentRunWorkspaceView`、`AgentConversationSnapshot`、`SessionRuntimeControlView` DTO。
- `crates/agentdash-contracts/src/agent/run_mailbox.rs` - mailbox DTO surface.
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` - frontend AgentRun workspace page boundary.
- `packages/app-web/src/pages/AgentRunWorkspacePage.conversationCommandState.ts` - frontend command state adapter from conversation snapshot.
- `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts` - chat props boundary.
- `packages/app-web/src/features/session/ui/SessionChatView.tsx` - chat stream/composer/status/mailbox rendering boundary.

### 1. 模块/子模块清单与一句话职责

- Session launch pipeline: 把来源意图变成 connector accepted turn，并把事件流接入 runtime processing；`LaunchCommand` 在 `command.rs:20`，stage 边界分布在 `ingestion.rs:7`、`connector_start.rs:11`、`preparation.rs:31`、`commit.rs:19`。
- Session runtime hub: 装配 connector、hook provider、runtime registry、turn supervisor、store、VFS、tool/MCP、anchor、AgentFrame、mailbox deps；`SessionRuntimeInner` 字段集中在 `hub/mod.rs:47` 到 `hub/mod.rs:95`。
- Frame construction: 从 `RuntimeSessionExecutionAnchor` 反查 run/agent/frame 并产出 non-optional `FrameLaunchEnvelope`；入口在 `frame/construction/mod.rs:90`，缺 anchor 直接拒绝 launch 在 `frame/construction/mod.rs:96` 到 `frame/construction/mod.rs:104`。
- Runtime launch types: `FrameRuntimeSurface` 只来自持久化 `AgentFrame`，`FrameLaunchIntent` 只来自 command/extras，`FrameLaunchEnvelope` 是 construction 到 planner 的唯一 handoff；见 `frame/runtime_launch.rs:31`、`frame/runtime_launch.rs:74`、`frame/runtime_launch.rs:191`。
- RuntimeSession / Backbone stream: `BackboneEnvelope` 是 session event notification，NDJSON 事件行直接携带 `notification`；spec 定义在 `backbone-protocol.md`，stream shape 在 `streaming-protocol.md`。
- Lifecycle / Workflow dispatch: `LifecycleDispatchService` 创建 / 复用 `LifecycleRun`、`LifecycleAgent`、`AgentFrame`、RuntimeSession、association、gate、lineage；结构在 `lifecycle/dispatch_service.rs:103`，start 入口在 `lifecycle/dispatch_service.rs:332`。
- Orchestration runtime: `apply_orchestration_event_to_run` 是 orchestration node runtime fact reducer，并刷新 run status；见 `workflow/orchestration/runtime.rs:266` 到 `workflow/orchestration/runtime.rs:279`。
- AgentRun workspace query: 从 run/agent 出发聚合 latest delivery runtime、session meta、frame/VFS、execution state、mailbox、conversation snapshot；主入口在 `agent_run/workspace/query.rs:64`。
- AgentConversationSnapshot: workspace chat/control 的 execution、commands、mailbox、model config、resource surface 投影；resolver 在 `agent_run/conversation_snapshot.rs:267`。
- AgentRun mailbox: durable message intake、command receipt、scheduler、launch/steer delivery；service 在 `agent_run/mailbox.rs:171`，user intake 在 `agent_run/mailbox.rs:219`，scheduler 在 `agent_run/mailbox.rs:981`。
- AgentRun message delivery: mailbox launch 消费通过 `LaunchCommand::lifecycle_agent_user_message_input` 回到 `SessionLaunchService`；见 `agent_run/message_delivery.rs:38` 到 `agent_run/message_delivery.rs:55`。
- Agent loop: Pi/Agent loop 发出 `TurnStart` / `TurnEnd` 并调用 runtime delegate 的 `after_turn` / `before_stop`；见 `agent_loop.rs:135`、`agent_loop.rs:270`、`agent_loop.rs:323`、`agent_loop.rs:351`。
- AgentRuntimeDelegate: 单一 trait 同时覆盖 compaction、context transform、tool policy、turn boundary、provider observer；方法列在 `agentdash-agent-types/src/runtime/delegate.rs:25` 到 `agentdash-agent-types/src/runtime/delegate.rs:78`。
- Hook delegate: 实现 broad delegate，把 hook runtime 接到 compaction/context/tool/turn/provider callbacks；实现入口在 `session/hook_delegate.rs:264`。
- Mailbox delegate: 包装 inner hook delegate，把 `after_turn` / `before_stop` 产生的 steering/follow-up 转成 mailbox envelope，并在 boundary drain mailbox；见 `session/mailbox_delegate.rs:233`、`session/mailbox_delegate.rs:300`、`session/mailbox_delegate.rs:345`。
- AgentRun API: `/agent-runs/{run_id}/agents/{agent_id}/workspace`、`composer-submit`、mailbox CRUD/control、cancel 是 public workspace command surface；路由在 `routes/lifecycle_agents.rs:63` 到 `routes/lifecycle_agents.rs:96`。
- RuntimeSession API: `/sessions/{id}/runtime-control` 从 runtime session id 反查 anchor/run/agent/frame，只读输出 trace/detail backlink；路由在 `routes/sessions.rs:85`，handler 在 `routes/sessions.rs:145`。
- Contracts: `AgentRunWorkspaceView` 包含 shell、delivery refs、control plane、conversation；`AgentConversationSnapshot` 包含 execution/model/commands/mailbox/resource surface；`SessionRuntimeControlView` 包含 runtime ref、meta、control plane、anchor/run/agent/frame；见 `workflow.rs:1081`、`workflow.rs:1098`、`workflow.rs:1328`。
- Frontend boundary: `AgentRunWorkspacePage` 是用户工作台，从 AgentRun workspace projection 驱动 chat + workspace panel；说明和 imports 在 `AgentRunWorkspacePage.tsx:2` 到 `AgentRunWorkspacePage.tsx:25`，`SessionChatView` 接收 conversation command state 与 mailbox snapshot 在 `SessionChatViewTypes.ts:30` 到 `SessionChatViewTypes.ts:89`。

### 2. 主链路拓扑：ExecutionIntent / launch -> RuntimeSession -> Backbone stream -> AgentRun workspace / mailbox / control -> frontend chat

当前目标拓扑可以按两条入口合流理解。

第一条是 lifecycle / project-agent 创建入口：

```text
ExecutionIntent / ProjectAgent draft start
  -> LifecycleDispatchService / ProjectAgentRunStartService
  -> LifecycleRun + LifecycleAgent + AgentFrame + RuntimeSession
  -> RuntimeSessionExecutionAnchor(run_id, agent_id, frame_id, runtime_session_id)
  -> initial mailbox envelope or later LaunchCommand
```

证据：

- `LifecycleDispatchService` 在 dispatch 后 upsert `RuntimeSessionExecutionAnchor::new_dispatch(session_id, run.id, frame.id, agent.id)`，见 `lifecycle/dispatch_service.rs:592` 到 `lifecycle/dispatch_service.rs:599`。
- task/story subject association 会用 agent-scoped association，见 `lifecycle/dispatch_service.rs:686` 到 `lifecycle/dispatch_service.rs:688`。
- ProjectAgent start 的初始用户输入不是直接 launch，而是转成 `ProjectAgentRunInitialMailboxCommand`，其 `into_mailbox_command` 使用 `MailboxMessageSource::DraftStart` 且 `schedule_on_submit: false`，见 `project_agent_start.rs:47` 到 `project_agent_start.rs:65`；实际接受初始 mailbox message 在 `project_agent_start.rs:423` 到 `project_agent_start.rs:431`。

第二条是 session launch / turn 执行入口：

```text
LaunchCommand
  -> FrameConstructionService.construct_launch_envelope
  -> FrameLaunchEnvelope
  -> LaunchPlanner / TurnPreparer
  -> ExecutionContext(session frame + turn frame)
  -> ConnectorStarter
  -> CommittedTurn
  -> StreamIngestionAttacher / SessionTurnProcessor
  -> PersistedSessionEvent.notification = BackboneEnvelope
  -> /sessions/{id}/stream/ndjson
```

证据：

- `LaunchCommand` 的来源 constructors 包括 HTTP、hook auto resume、companion、routine、local relay，见 `session/launch/command.rs:134` 到 `session/launch/command.rs:195`。
- `FrameLaunchEnvelope` 明确包含 `surface`、`surface_draft`、`launch_surface`、`pending_frame`、`intent`、`working_directory`、`context_bundle`、`resolution_trace`，见 `frame/runtime_launch.rs:191` 到 `frame/runtime_launch.rs:203`。
- `TurnPreparer` 把 `launch_plan.context` 组装为 connector context，并调用 runtime tool provider / MCP discovery，见 `session/launch/preparation.rs:98` 到 `session/launch/preparation.rs:100` 与 `session/tool_assembly.rs:6` 到 `session/tool_assembly.rs:24`。
- `ConnectorAcceptedTurn` 表达 connector.prompt accepted boundary，见 `session/launch/connector_start.rs:11` 到 `session/launch/connector_start.rs:28`。
- `AttachedTurn` 表达 stream 已接入 processor / adapter supervision，见 `session/launch/ingestion.rs:7` 到 `session/launch/ingestion.rs:20`。
- Runtime stream contract 中 session event 行携带 `notification: BackboneEnvelope`，见 `.trellis/spec/backend/session/streaming-protocol.md`；Backbone 持久化字段是 `PersistedSessionEvent.notification`，见 `.trellis/spec/cross-layer/backbone-protocol.md`。

AgentRun workspace / mailbox control 入口以 run+agent 为 public identity：

```text
GET /agent-runs/{run_id}/agents/{agent_id}/workspace
  -> AgentRunWorkspaceQueryService.resolve
  -> latest delivery RuntimeSession via RuntimeSessionExecutionAnchor
  -> SessionExecutionState + AgentFrame/VFS + mailbox messages/state
  -> AgentConversationSnapshot
  -> AgentRunWorkspaceView
  -> frontend AgentRunWorkspacePage -> SessionChatView
```

证据：

- workspace query 先解析 `delivery_runtime_session_for_agent_run`，再读 session meta、execution state、steering support、mailbox messages/state，见 `agent_run/workspace/query.rs:70`、`agent_run/workspace/query.rs:125` 到 `agent_run/workspace/query.rs:143`、`agent_run/workspace/query.rs:151` 到 `agent_run/workspace/query.rs:181`。
- conversation snapshot 根据 `SessionExecutionState` 派生 `DeliveryMissing`、`StartingClaimed`、`RunningActive`、`Cancelling`、`Ready` 等状态，见 `agent_run/conversation_snapshot.rs:341` 到 `agent_run/conversation_snapshot.rs:377`。
- conversation commands 中 submit/cancel/mailbox resume 的 enablement 来自 snapshot 输入，见 `agent_run/conversation_snapshot.rs:390` 到 `agent_run/conversation_snapshot.rs:501`。
- API mapper 把 snapshot mailbox state/messages 放回 `conversation.mailbox`，并从 conversation 派生 top-level `control_plane`，见 `routes/lifecycle_agents.rs:977` 到 `routes/lifecycle_agents.rs:992`。
- frontend 由 `useAgentRunWorkspaceState` 读 workspace projection，构造 `chatCommandState` 和 `conversationMailbox`，再传给 `SessionChatView`，见 `AgentRunWorkspacePage.tsx:167`、`AgentRunWorkspacePage.tsx:359`、`AgentRunWorkspacePage.tsx:384`、`AgentRunWorkspacePage.tsx:762` 到 `AgentRunWorkspacePage.tsx:778`。

Mailbox command / scheduling 主链路：

```text
composer-submit / mailbox promote-delete-resume / hook/system message
  -> command policy / command receipt
  -> AgentRunMailboxMessage durable envelope
  -> AgentRunMailboxService.schedule
  -> launch via SessionTurnMessageDeliveryPort
     OR steer via SessionControlService.steer_session
  -> AgentLoop after_turn / before_stop boundary drains mailbox
  -> Backbone event + mailbox_state_changed refresh
```

证据：

- API `composer-submit` 先跑 `AgentRunWorkspaceCommandPolicyService.ensure_composer_submit_allowed`，再调用 `AgentRunMailboxService.accept_user_message`，见 `routes/lifecycle_agents.rs:387` 到 `routes/lifecycle_agents.rs:448`。
- mailbox service 创建 user message 后 attach command receipt 并 schedule，见 `agent_run/mailbox.rs:312` 到 `agent_run/mailbox.rs:422`。
- scheduler entry `schedule` / `schedule_for_target` 在 `agent_run/mailbox.rs:981` 到 `agent_run/mailbox.rs:1006`。
- launch 消费用 `SessionTurnMessageDeliveryPort::deliver_user_message`，最终构造 lifecycle-agent `LaunchCommand` 并调用 `launch_command_in_task`，见 `agent_run/message_delivery.rs:38` 到 `agent_run/message_delivery.rs:55`。
- steer 消费会检查 active state / steering support，并调用 `steer_session(SessionTurnSteerCommand { ... })`，见 `agent_run/mailbox.rs:1503` 到 `agent_run/mailbox.rs:1557`。
- mailbox delegate 在 `after_turn` 把 hook steering/follow_up 写成 mailbox delivery message 并触发 AgentLoopTurn boundary schedule，见 `session/mailbox_delegate.rs:300` 到 `session/mailbox_delegate.rs:339`。
- mailbox delegate 在 `before_stop` drain AgentRunTurn boundary，必要时以 `StopDecision::Continue` 注入 steering，见 `session/mailbox_delegate.rs:345` 到 `session/mailbox_delegate.rs:421`。

### 3. 与其它模块的耦合点：只列边界

- Workflow / Lifecycle: `LifecycleDispatchService` 和 `OrchestrationExecutorLauncher` 是 run/agent/frame/runtime anchor 的创建与 orchestration node start/completion 边界；`apply_orchestration_event_to_run` 是 workflow runtime node fact reducer，见 `workflow/orchestration/runtime.rs:266` 到 `workflow/orchestration/runtime.rs:279`。
- Permission: API route 入口用 `ProjectPermission::View/Edit` 做 project permission gate；例如 workspace view 用 View，composer/mailbox/cancel 用 Edit，见 `routes/lifecycle_agents.rs:264` 到 `routes/lifecycle_agents.rs:278`、`routes/lifecycle_agents.rs:387` 到 `routes/lifecycle_agents.rs:428`。本研究未深挖 grant/permission service 内部。
- VFS / tools / MCP: Frame construction 产出 `FrameLaunchSurface { capability_state, vfs, mcp_servers, execution_profile }`，见 `frame/runtime_launch.rs:90`；launch preparation 用 runtime tool provider + MCP discovery 组装 tools，见 `session/tool_assembly.rs:6` 到 `session/tool_assembly.rs:34`；live hot update 从 active turn 或 anchor/current frame 读取 MCP/capability，见 `session/hub/tool_builder.rs:23` 到 `session/hub/tool_builder.rs:79`。
- Hooks: `HookRuntimeDelegate` 实现 broad `AgentRuntimeDelegate`，覆盖 compaction、context、tool、turn、provider observer，见 `session/hook_delegate.rs:264` 到 `session/hook_delegate.rs:742`；`AgentRunMailboxRuntimeDelegate` 是 hook delegate 到 mailbox boundary 的 adapter，见 `session/mailbox_delegate.rs:233` 到 `session/mailbox_delegate.rs:427`。
- Contracts: `AgentConversationSnapshot` 是 chat command/mailbox contract；`AgentRunWorkspaceView` 是 workspace shell + delivery refs + conversation contract；`SessionRuntimeControlView` 是 runtime trace/detail backlink contract，见 `agentdash-contracts/src/runtime/workflow.rs:1081`、`:1098`、`:1328`。
- Frontend: AgentRun page 只应从 AgentRun workspace projection 进入 command/control，`SessionChatView` 作为 chat UI 接收 `commandState` 与 `mailboxSnapshot`，见 `AgentRunWorkspacePage.tsx:762` 到 `AgentRunWorkspacePage.tsx:778`、`SessionChatViewTypes.ts:30` 到 `SessionChatViewTypes.ts:89`。
- RuntimeSession detail: `/sessions/{id}/runtime-control` 从 runtime trace identity 反查 anchor/run/agent/frame，当前 DTO 不再包含 mailbox/action list；handler 输出 `anchor/run/agent/frame_runtime/subject_associations`，见 `routes/sessions.rs:145` 到 `routes/sessions.rs:279`。
- Agent loop: `agentdash-agent` 内部仍保留 steering/follow_up queues 与 delegate callbacks，mailbox 通过 runtime delegate boundary 介入，而不是改 agent loop core；见 `agent_loop.rs:87` 到 `agent_loop.rs:117`、`agent_loop.rs:399` 到 `agent_loop.rs:439`。

### 4. 值得下一轮深挖的 review 问题

#### P0

- P0-1: AgentRun command/control 的唯一事实源是否已经真正收敛到 `AgentConversationSnapshot.commands + conversation.mailbox + AgentRunMailboxService`？
  - 证据：workspace query 先生成 conversation snapshot，API mapper 又从 conversation 派生 top-level `control_plane`，见 `routes/lifecycle_agents.rs:989` 到 `routes/lifecycle_agents.rs:992`；command policy 仍独立读取 execution state、steering support、mailbox visible count/state 并重新 resolve conversation snapshot，见 `agent_run/workspace/command_policy.rs:141` 到 `agent_run/workspace/command_policy.rs:180`。
  - 下一轮应确认：command availability 是否只有一个 resolver；route policy 是 durable precondition 校验，还是仍在复制 UI command state。

- P0-2: RuntimeSession trace/detail 是否保持只读 backlink，不再重新成为 AgentRun workspace 控制入口？
  - 证据：当前 `SessionRuntimeControlView` 只有 runtime ref、meta、control plane、anchor、run、agent、frame_runtime、subject_associations，见 `workflow.rs:1328` 到 `workflow.rs:1342`；handler 从 runtime id 反查 anchor 并构造 status，见 `routes/sessions.rs:156` 到 `routes/sessions.rs:270`。
  - 下一轮应确认：前端是否仍有从 `/sessions/{id}/runtime-control` 发 command/control 的路径；若有，应迁回 `/agent-runs/{run_id}/agents/{agent_id}/...`。

- P0-3: `RuntimeSessionExecutionAnchor` 的 latest delivery runtime 选择是否能覆盖 frame revision、replacement session、graph node attempt 的事实一致性？
  - 证据：workspace query 从 `execution_anchor_repo.list_by_run(run_id)` 过滤 agent 得到 delivery runtime，见 `agent_run/workspace/query.rs:314` 到 `agent_run/workspace/query.rs:322`；frame construction 又要求 runtime session 必须有 anchor，见 `frame/construction/mod.rs:96` 到 `frame/construction/mod.rs:104`。
  - 下一轮应确认：同一 agent 多 runtime session、多 frame revision、orchestration node attempt 时，workspace、mailbox、hook runtime、terminal callback 是否都选同一个 active delivery coordinate。

- P0-4: mailbox scheduler 是否是唯一 launch/steer 判定边界，direct steer/service 路径是否仍可能绕过 durable envelope？
  - 证据：`AgentRunMailboxService` launch/steer 消费都从 durable message 出发，见 `agent_run/mailbox.rs:1415` 到 `agent_run/mailbox.rs:1557`；但 `agent_run/steering.rs` 仍存在 direct `AgentRunSteeringService::steer`，会从 runtime session id 直接投递 `SessionTurnSteerCommand`。
  - 下一轮应确认：direct steer 是否只剩测试/内部迁移代码；产品/API 路径是否全部走 mailbox。

#### P1

- P1-1: `AgentRuntimeDelegate` 是否需要拆分，避免 hook、mailbox boundary、compaction、tool policy、provider observer 被同一个宽 trait 绑定？
  - 证据：trait 方法横跨 compaction、context transform、tool before/after、after_turn、before_stop、provider request observer，见 `agentdash-agent-types/src/runtime/delegate.rs:25` 到 `agentdash-agent-types/src/runtime/delegate.rs:78`；mailbox delegate 为了 turn boundary 必须转发其它方法，见 `session/mailbox_delegate.rs:233` 到 `session/mailbox_delegate.rs:299`。

- P1-2: `AgentRunMailboxRuntimeDelegate` 的 direct fallback 是否仍会形成第二条 hook steering 路径？
  - 证据：`route_hook_delivery_messages` 写 mailbox 失败且错误是 NotFound 时返回 `direct_messages`，见 `session/mailbox_delegate.rs:190` 到 `session/mailbox_delegate.rs:221`。
  - 下一轮应确认：NotFound fallback 是 standalone/unanchored trace 的必要边界，还是会让 anchored AgentRun 绕过 durable mailbox。

- P1-3: `SessionRuntimeInner` 的装配边界是否仍过宽，还是已足够通过 validation 保证主链路依赖完整？
  - 证据：`SessionRuntimeInner` 仍集中 connector、hook provider、registry、turn supervisor、stores、VFS、tool/MCP、backend placement、frame/anchor/agent/mailbox/gate deps，见 `hub/mod.rs:47` 到 `hub/mod.rs:95`；但 factory 现在会在 mailbox boundary 注入和 validate 时检查缺失依赖，见 `hub/factory.rs:196` 到 `hub/factory.rs:214`、`hub/factory.rs:326` 到 `hub/factory.rs:333`。
  - 下一轮应聚焦“是否仍职责过宽”，不要重复 06-14 的“静默 no-op”旧结论。

- P1-4: Frame construction / tool hot update 是否把 `AgentFrame` 保持为 capability/VFS/MCP 唯一事实源？
  - 证据：`FrameLaunchSurface` launch-ready gate 要求 capability/vfs/mcp/execution profile non-optional，见 `frame/runtime_launch.rs:90`；hot update 写入 `AgentFrame` revision 后再更新 active turn cache 和 connector tools，见 `session/hub/tool_builder.rs:159` 到 `session/hub/tool_builder.rs:239`。
  - 下一轮应确认：runtime command store、capability transition、tool assembly、hook runtime capability update 是否存在并列事实源。

- P1-5: Terminal effect / hook auto-resume / orchestration node terminal 的事实顺序是否一致？
  - 证据：terminal effects 先入 outbox，再由 dispatcher replay terminal callback / hook effects / auto resume，见 `session/terminal_effects.rs:57` 到 `session/terminal_effects.rs:67`、`session/terminal_effects.rs:170` 到 `session/terminal_effects.rs:292`；orchestration node terminal 应走 runtime reducer，见 `workflow/orchestration/runtime.rs:186`、`:266`。
  - 下一轮应确认：terminal event、active turn cleanup、mailbox boundary drain、orchestration `NodeCompleted/Failed`、Task artifact/status side effect 的顺序和幂等坐标。

- P1-6: Workspace projection、conversation snapshot、API mapper、frontend adapter 是否已经只保留必要层次？
  - 证据：workspace query 构造 snapshot，API mapper再合并 mailbox state/messages 到 `conversation.mailbox` 并派生 control plane，见 `agent_run/workspace/query.rs:203` 到 `agent_run/workspace/query.rs:242`、`routes/lifecycle_agents.rs:977` 到 `routes/lifecycle_agents.rs:992`；frontend 还用 `AgentRunWorkspacePage.conversationCommandState.ts` 把 conversation 转成 `SessionChatCommandState`，见 `AgentRunWorkspacePage.conversationCommandState.ts:240`。
  - 下一轮应确认：这些层是 DTO mapping / UI adapter，还是仍在复制 command state 规则。

#### P2

- P2-1: `AgentRunMailboxService` 内部职责是否需要拆成 intake、scheduler、delivery executor、receipt projector？
  - 证据：单 service 覆盖 accept user/hook/system、delete/promote/resume、schedule、claim、launch、steer、receipt replay、payload cleanup，见 `agent_run/mailbox.rs:219`、`:511`、`:579`、`:663`、`:759`、`:981`、`:1415`、`:1503`、`:1963`。

- P2-2: Frontend `AgentRunWorkspacePage` 是否只是边界适配，还是过度承载 workspace/resource/hook/chat/mailbox/command 组合？
  - 证据：页面同时接入 workspace state、hook runtime refresh、workspace commands、session chat、workspace panel、module presentation、stream event refresh，见 `AgentRunWorkspacePage.tsx:167`、`:407`、`:470`、`:762`。

- P2-3: Contracts 命名与路径是否需要在 review 中同步更新，避免旧范围名误导？
  - 证据：用户范围中的 `workflow/agent_run_*` 当前实际在 `crates/agentdash-application/src/agent_run/*`；contracts 中 AgentRun workspace 位于 `crates/agentdash-contracts/src/runtime/workflow.rs`，mailbox DTO 位于 `crates/agentdash-contracts/src/agent/run_mailbox.rs`。

### 5. 不应重复 review 的内容

- 不应重复论证 `RuntimeSessionExecutionAnchor` 本身是否需要存在。06-14 已明确它是正确的 trace-to-control-plane backlink，问题在一致使用 anchor/node 坐标。
- 不应重复论证 AgentRun mailbox 是否需要存在。06-14 已明确 mailbox 作为 durable message intake/scheduler/recovery fact source 是正确方向；本轮只需要检查是否还有绕过 mailbox 的 launch/steer/control 路径。
- 不应重复深挖 Lifecycle cancel reducer、Task projection absence -> Failed、PermissionGrant 与 companion grant 双事实源、VFS/Local/Relay/Extension 过宽等已经在 06-14 报告中列为独立清理队列的点；本文件只把它们作为边界引用。
- 不应把 graphless lifecycle run 当作问题。06-14 已指出 graphless lifecycle 是 Agent runtime 的正常拓扑。
- 不应基于旧 06-14 结论继续写“SessionRuntimeInner mailbox boundary 缺依赖会静默 no-op”。当前 `with_agent_run_mailbox_boundary` 和 `validate` 已显式返回错误，见 `session/hub/factory.rs:196` 到 `session/hub/factory.rs:214`、`:326` 到 `:333`。

## Code Patterns

- Anchor-first delivery backlink: runtime session 必须通过 `RuntimeSessionExecutionAnchor` 回到 run/agent/frame，见 `frame/construction/mod.rs:96` 到 `frame/construction/mod.rs:115`。
- Intent/surface 分离: `FrameLaunchIntent` 不含 frame surface，`FrameRuntimeSurface` 只来自 persisted `AgentFrame`，`FrameLaunchEnvelope` 才是 launch-ready closure，见 `frame/runtime_launch.rs:31`、`:74`、`:191`。
- Conversation snapshot as UI command surface: `ConversationCommandSetView`、`ConversationMailboxSnapshotView`、`AgentConversationSnapshot` 在 contracts 中连成 chat control surface，见 `workflow.rs:1010`、`:1033`、`:1081`。
- Mailbox durable before delivery: `accept_user_message` 先创建 mailbox message / receipt，再 schedule 并返回 scheduler outcome，见 `agent_run/mailbox.rs:312` 到 `agent_run/mailbox.rs:436`。
- Agent loop boundary scheduling: `after_turn` / `before_stop` 是 mailbox drain hook，而不是 route-local branching，见 `session/mailbox_delegate.rs:300` 到 `session/mailbox_delegate.rs:421`。
- RuntimeSession API read-only posture: `SessionRuntimeControlView` 不包含 mailbox/action fields，见 `workflow.rs:1328` 到 `workflow.rs:1342`。

## External References

- None. 本研究只使用仓库内 spec、task artifact 和代码。

## Related Specs

- `.trellis/spec/project-overview.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/session/streaming-protocol.md`
- `.trellis/spec/cross-layer/backbone-protocol.md`
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件按用户显式给出的 task path 写入，未猜测其它任务目录。
- 用户范围中的 `crates/agentdash-application/src/workflow/agent_run_*` 在当前仓库未找到；对应代码已位于 `crates/agentdash-application/src/agent_run/*`，workflow orchestration runtime 仍位于 `src/workflow/orchestration/*`。
- 本轮是只读架构 research：没有运行测试，没有改业务代码，没有执行 git 操作。
- Frontend 只做边界引用；未展开检查 `useAgentRunWorkspaceState`、`useAgentRunWorkspaceCommands`、service client 的全部内部实现。
- VFS/tools/permission/contracts 只列 Session/AgentRun 主链路相邻边界；深度问题已由 06-14 overdesign review 覆盖或应另开专题。
