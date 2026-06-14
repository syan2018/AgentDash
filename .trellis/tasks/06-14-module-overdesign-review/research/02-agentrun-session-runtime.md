# Research: AgentRun / Session / Runtime Gateway overdesign review

- Query: AgentRun / Session / Runtime Gateway 链路是否存在过度设计、过厚边界、重复事实源、跨层耦合或职责漂移
- Scope: internal
- Date: 2026-06-14

## Findings

### 摘要判断

AgentRun 目标边界在 spec 中是清晰的：AgentRun workspace 的用户命令、mailbox、scheduler outcome 是 control-plane 事实源；RuntimeSession 只是 delivery / trace substrate；RuntimeGateway 是 runtime action 调用入口。当前实现大体朝这个方向走，但有三类明显过厚：

1. AgentRun workspace 同一响应里同时携带 workspace projection、conversation snapshot、mailbox state/messages、runtime command state，且 command policy 又从同一批事实重新派生一次状态，形成多套控制状态投影。
2. SessionHub / SessionRuntimeInner 已经成为运行时装配中心、hook/cache/tool/transition/mailbox boundary 的服务定位器，且 mailbox boundary 注入有顺序依赖和静默失效。
3. AgentRuntimeDelegate 是过宽 agent loop 扩展点，HookRuntimeDelegate 与 AgentRunMailboxRuntimeDelegate 被迫通过嵌套 delegate 和大量转发共享一个边界。

RuntimeGateway 本身暂不构成主要问题：它的 `invoke` 校验和 provider 访问方向基本符合 spec；更值得后续单独看的是 surface manifest 是否要展示 dynamic extension action。

### Files Found

- `.trellis/spec/backend/session/architecture.md` — Session / AgentRun / RuntimeSession 分层总契约。
- `.trellis/spec/backend/session/runtime-execution-state.md` — runtime registry、active turn、workspace mailbox action 边界。
- `.trellis/spec/backend/session/agentrun-mailbox.md` — mailbox envelope、scheduler、command receipt、turn boundary 契约。
- `.trellis/spec/backend/session/execution-context-frames.md` — connector-facing `ExecutionContext` 投影边界。
- `.trellis/spec/backend/session/streaming-protocol.md` — RuntimeSession NDJSON trace stream 契约。
- `.trellis/spec/backend/runtime-gateway.md` — RuntimeGateway actor/context/action 调用边界。
- `crates/agentdash-application/src/workflow/agent_run_workspace/types.rs` — AgentRun workspace application projection model。
- `crates/agentdash-application/src/workflow/agent_run_workspace/query.rs` — AgentRun workspace snapshot 查询与聚合。
- `crates/agentdash-application/src/workflow/conversation_snapshot.rs` — conversation execution、commands、mailbox snapshot 派生。
- `crates/agentdash-application/src/workflow/agent_run_workspace/command_policy.rs` — AgentRun command stale guard / policy 校验。
- `crates/agentdash-application/src/workflow/agent_run_mailbox.rs` — mailbox intake、scheduler、delivery、receipt 聚合服务。
- `crates/agentdash-application/src/workflow/agent_steering.rs` — 旧式 direct AgentRun steer 服务。
- `crates/agentdash-application/src/session/hub/*` — SessionRuntimeInner 装配、tool/hook/transition 内部实现。
- `crates/agentdash-application/src/session/mailbox_delegate.rs` — AgentRun mailbox runtime delegate。
- `crates/agentdash-application/src/session/hook_delegate.rs` — Hook runtime delegate。
- `crates/agentdash-agent-types/src/runtime/delegate.rs` — Agent runtime delegate trait。
- `crates/agentdash-agent/src/agent_loop*.rs` — agent loop 调用 delegate 的位置。
- `crates/agentdash-application/src/runtime_gateway/*` — RuntimeGateway、providers、session/setup/extension action。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` — AgentRun workspace / mailbox command routes。
- `crates/agentdash-api/src/routes/sessions.rs` — RuntimeSession runtime-control / trace endpoints。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts` — AgentRun workspace command hook。
- `packages/app-web/src/features/session/ui/SessionChatView*.ts*` — session chat UI command/mailbox consumption。

### 1. P1 - 重复事实源：AgentRun workspace 存在多套控制状态投影

- Priority: P1
- Problem type: 重复事实源 / 过度投影 / 前后端耦合
- Evidence paths:
  - `crates/agentdash-application/src/workflow/agent_run_workspace/types.rs:240`
  - `crates/agentdash-application/src/workflow/agent_run_workspace/query.rs:139`
  - `crates/agentdash-application/src/workflow/conversation_snapshot.rs:336`
  - `crates/agentdash-application/src/workflow/conversation_snapshot.rs:388`
  - `crates/agentdash-application/src/workflow/agent_run_workspace/command_policy.rs:41`
  - `crates/agentdash-contracts/src/workflow.rs:1221`
  - `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts:93`
- Concrete code evidence:
  - `AgentRunWorkspaceProjectionModel` 同时包含 `state_code`、`active_turn_id`、`delivery_status`、`control_plane`、`actions`、`runtime_command_state` 和 `replacement_command`。
  - `AgentRunWorkspaceQueryService::resolve` 在 `query.rs:117-145` 先从 `SessionExecutionState` 派生 workspace projection，又在 `query.rs:199-215` 用同一批 execution/runtime/frame/mailbox facts 构造 `AgentConversationSnapshot`。
  - `conversation_execution_view` 在 `conversation_snapshot.rs:336-386` 再次派生 ready/running/cancelling/frame_missing/delivery_missing；`conversation_commands` 在 `conversation_snapshot.rs:388-507` 再次派生 submit/cancel/promote/delete/resume 的 enablement。
  - `AgentRunWorkspaceCommandPolicyService::ensure_command_allowed` 在 `command_policy.rs:41-75` 又读取 execution state、frame ref 并重新调用 `AgentRunWorkspaceProjection::derive` 来校验 stale guard。
  - wire contract 的 `AgentRunWorkspaceView` 同时输出 top-level `control_plane`、`actions`、`mailbox`、`mailbox_messages` 和 `conversation`。
  - 前端 `SessionChatViewProps` 同时接收 `mailboxMessages`、`mailboxSnapshot`、`mailboxState`，注释分别指向 runtimeControl、conversation、workspace 根状态。
- Impact:
  - 同一个用户动作是否可执行，至少可由 top-level `actions`、conversation `commands`、command policy、mailbox row `can_*` 四处表达；这些字段任一漂移都会导致 UI 状态、后端校验和 scheduler outcome 不一致。
  - 新增 command 时需要同时改 projection、conversation snapshot、policy、contract、frontend command hook，成本偏高。
- Suggested cleanup direction:
  - 把 `AgentConversationSnapshot` 收敛为 AgentRun workspace 的唯一可执行 chat/control projection：`execution`、`commands`、`model_config`、`mailbox user_attention/resume_command` 都从这里输出。
  - top-level `AgentRunWorkspaceView` 保留 shell/list/detail 所需事实，例如 identity、display title、delivery refs、frame/resource surface；移除或降级 top-level `actions` / `replacement_command` 这种 chat command 投影。
  - command policy 不应重新派生另一套 workspace projection；应复用同一个 conversation snapshot resolver 的 command stale guard / command availability 计算，后端校验只做 command precondition 与 durable facts 的一致性判断。

### 2. P1 - 职责漂移：RuntimeSession runtime-control 变成第二个 AgentRun workspace 投影入口

- Priority: P1
- Problem type: 职责漂移 / 重复事实源 / 跨层耦合
- Evidence paths:
  - `crates/agentdash-api/src/routes/sessions.rs:149`
  - `crates/agentdash-api/src/routes/sessions.rs:246`
  - `crates/agentdash-api/src/routes/sessions.rs:295`
  - `crates/agentdash-contracts/src/workflow.rs:1528`
  - `crates/agentdash-api/src/routes/lifecycle_agents.rs:621`
- Concrete code evidence:
  - `get_session_runtime_control` 从 runtime session id 反查 `RuntimeSessionExecutionAnchor`，再加载 run、agent、current frame、subject associations。
  - `sessions.rs:246-293` 基于 `SessionExecutionState` 和 agent status 独立派生 `SessionRuntimeControlPlaneView` 与 `SessionRuntimeActionSetView`。
  - `sessions.rs:295-324` 再次读取 `agent_run_mailbox_repo`，构造 `MailboxStateView` 与 `MailboxMessageView[]`。
  - `SessionRuntimeControlView` contract 包含 `run`、`agent`、`frame_runtime`、`subject_associations`、`actions`、`mailbox`、`mailbox_messages`。
  - `lifecycle_agents.rs:621-668` 的 `agent_run_workspace_view` 已经输出 AgentRun workspace 的 control plane、actions、mailbox、messages、resource surface、conversation。
- Impact:
  - `/sessions/{id}/runtime-control` 和 `/agent-runs/{run_id}/agents/{agent_id}/workspace` 会分别表达 anchored control state，形成 trace identity 与 workspace identity 两条可展示/可决策入口。
  - RuntimeSession detail 被拉入 AgentRun command/mailbox 领域，和 spec 中“RuntimeSession trace endpoint 使用 trace metadata，只读 trace/feed/debug；AgentRun workspace 负责 command/control”的边界冲突。
- Suggested cleanup direction:
  - RuntimeSession runtime-control 收窄为 trace/detail/read-only：session shell、trace meta、anchor backlink、workspace ref、frame ref；不要投影 mailbox/action。
  - 需要 command/control 时，前端从 anchor 跳转或请求 AgentRun workspace endpoint；不要在 RuntimeSession route 内复制 workspace query。
  - 若 runtime detail 必须展示 mailbox 摘要，只引用 AgentRun workspace snapshot 的只读摘要对象，不保留独立 action set。

### 3. P1 - 模块过厚：SessionRuntimeInner 是 order-sensitive 服务定位器，mailbox boundary 可静默失效

- Priority: P1
- Problem type: 模块过厚 / 隐式依赖 / 初始化耦合
- Evidence paths:
  - `crates/agentdash-application/src/session/hub/mod.rs:47`
  - `crates/agentdash-application/src/session/hub/factory.rs:196`
  - `crates/agentdash-application/src/session/hub/factory.rs:291`
  - `crates/agentdash-application/src/session/launch/deps.rs:30`
  - `crates/agentdash-application/src/session/launch/planner.rs:146`
  - `crates/agentdash-api/src/bootstrap/session.rs:131`
- Concrete code evidence:
  - `SessionRuntimeInner` 持有 connector、hook provider、runtime registry、turn supervisor、store set、VFS、skill discovery、terminal callback、construction provider、audit bus、settings、runtime tool provider、MCP discovery、backend placement、AgentFrame repo、anchor repo、LifecycleAgent repo、AgentRun mailbox deps、LifecycleGate repo。
  - `with_agent_run_mailbox_boundary` 在 `factory.rs:202-210` 如果 `lifecycle_agent_repo`、`agent_frame_repo` 或 `execution_anchor_repo` 未先注入，会直接 `return self`，没有错误。
  - `assert_ready_for_app_state` 只校验 runtime tool、MCP discovery、terminal callback、hook effect registry、construction provider、audit bus、backend placement，未校验 AgentFrame/anchor/LifecycleAgent/mailbox boundary。
  - `LaunchPlanningDeps` 继续携带 `agent_run_mailbox_boundary_deps`，`SessionLaunchPlanner` 在 `planner.rs:160-167` 根据 Option 决定是否包 `AgentRunMailboxRuntimeDelegate`。
  - bootstrap 当前依赖 `with_agent_frame_repo`、`with_execution_anchor_repo`、`with_lifecycle_agent_repo`、`with_agent_run_mailbox_boundary` 的调用顺序。
- Impact:
  - 预研期代码不需要兼容“缺依赖也运行”的形态；当前静默 no-op 会把 AgentRun mailbox turn boundary 从运行时 delegate 中移除，表现为系统看似可启动但 mailbox boundary 不消费。
  - SessionHub 的所有服务都从同一个 inner clone 派生，使 tool/hook/transition/mailbox/launch 的 ownership 难以一眼看清。
- Suggested cleanup direction:
  - 把 AppState 正式运行态所需依赖改为显式 `AppSessionRuntimeDeps`，构造时一次性校验，失败即返回错误。
  - `with_agent_run_mailbox_boundary` 不应静默返回；缺依赖直接 panic/Result error，或者把 AgentRunRuntimeBindings 作为完整结构注入。
  - 拆出 `SessionDeliveryRuntime`、`SessionLaunchRuntime`、`AgentRunRuntimeBoundary` 三个内部依赖包，减少任意服务拿到整个 `SessionRuntimeInner`。

### 4. P1 - 过宽接口：AgentRuntimeDelegate 把 hook、tool policy、context transform、turn boundary 和 provider observer 混成一个 trait

- Priority: P1
- Problem type: 过度抽象 / 过宽边界 / 装饰器泄漏
- Evidence paths:
  - `crates/agentdash-agent-types/src/runtime/delegate.rs:25`
  - `crates/agentdash-agent/src/agent_loop/streaming.rs:117`
  - `crates/agentdash-agent/src/agent_loop/tool_call.rs:324`
  - `crates/agentdash-agent/src/agent_loop.rs:318`
  - `crates/agentdash-application/src/session/mailbox_delegate.rs:173`
  - `crates/agentdash-application/src/session/launch/planner.rs:146`
- Concrete code evidence:
  - `AgentRuntimeDelegate` 定义 `evaluate_compaction`、`after_compaction`、`after_compaction_failed`、`transform_context`、`before_tool_call`、`after_tool_call`、`after_turn`、`before_stop`、`on_before_provider_request`。
  - agent loop 在 streaming 前调用 `transform_context`、`evaluate_compaction`、`on_before_provider_request`，工具调用前后调用 `before_tool_call` / `after_tool_call`，turn 边界调用 `after_turn` / `before_stop`。
  - `AgentRunMailboxRuntimeDelegate` 只真正需要 after-turn / before-stop mailbox scheduling，却必须实现所有方法并在 `mailbox_delegate.rs:174-239`、`207-239`、`339-347` 大量转发 inner delegate。
  - `SessionLaunchPlanner` 先构造 `HookRuntimeDelegate`，再用 `AgentRunMailboxRuntimeDelegate` 包住它。
- Impact:
  - 每新增一个 agent loop delegate method，mailbox wrapper 都必须同步转发；一个 turn-boundary concern 被迫知道 compaction/tool/provider observer 的存在。
  - Hook runtime 与 mailbox scheduling 的组合顺序隐藏在 launch planner 中，业务读代码时不容易判断哪些消息走 mailbox，哪些保留 direct fallback。
- Suggested cleanup direction:
  - 拆分 trait：`ContextTransformDelegate`、`ToolCallPolicyDelegate`、`CompactionDelegate`、`TurnBoundaryDelegate`、`ProviderRequestObserver`。
  - agent loop 接收一个显式 composite，mailbox 只实现 `TurnBoundaryDelegate`，hook runtime 按需实现多个小 trait。
  - `LaunchPlan` 中的 runtime delegate 字段改为结构化 delegate set，避免“Arc<dyn AgentRuntimeDelegate> 包一层”的隐式组合。

### 5. P2 - 模块过厚：AgentRunMailboxService 同时承担 intake、scheduler、delivery、receipt、projection result

- Priority: P2
- Problem type: 模块过厚 / 职责集中 / 重复 active-turn 判断
- Evidence paths:
  - `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:106`
  - `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:751`
  - `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:884`
  - `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:951`
  - `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:1073`
  - `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:1149`
  - `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:1652`
- Concrete code evidence:
  - `AgentRunMailboxService` 构造函数注入 lifecycle run/agent、AgentFrame、anchor、command receipt、mailbox repo、session core/control/eventing/launch。
  - `schedule` 根据 trigger、runtime state、barrier/drain mode 决定 claim。
  - `claim_and_consume` 负责 claim lease 与逐条消费。
  - `consume_as_delegate_steering` 和 `consume_as_steering` 都检查 active turn、expected turn mismatch、写 `UserInputSubmitted`、mark `Steered`、complete receipt、cleanup payload。
  - `consume_as_launch` 和 `consume_as_resume_launch_source` 都负责 launch delivery、accepted refs、mark `Dispatched`、complete receipt、cleanup payload。
  - `user_message_policy` 同时决定 delivery class、barrier、drain mode、queued/expected turn ids。
- Impact:
  - 统一 mailbox 事实源是正确的，但服务类已经覆盖太多生命周期阶段；后续调整 queue policy、receipt replay、steer delivery 或 hook boundary 都会触碰同一个大文件。
  - delegate steering 与 route/scheduler steering 两条消费实现相似，容易在事件写入、accepted refs 或 payload cleanup 上漂移。
- Suggested cleanup direction:
  - 保留 durable mailbox envelope / scheduler 作为唯一事实源。
  - 内部拆为 `MailboxIntake`、`MailboxScheduler`、`MailboxDeliveryExecutor`、`MailboxReceiptProjector`，由薄 orchestrator 串联。
  - 把 `consume_as_delegate_steering` 和 `consume_as_steering` 收敛到同一个 steer delivery executor，delegate path 只决定“返回 AgentMessage 给 loop”这个结果形态。

### 6. P2 - 清理候选：AgentRunSteeringService 是未被产品路径使用的 direct steer 平行实现

- Priority: P2
- Problem type: 死路径 / 平行控制面 / 重复 delivery 逻辑
- Evidence paths:
  - `crates/agentdash-application/src/workflow/agent_steering.rs:30`
  - `crates/agentdash-application/src/workflow/agent_steering.rs:76`
  - `crates/agentdash-application/src/workflow/agent_steering.rs:135`
  - `crates/agentdash-application/src/workflow/agent_run_mailbox.rs:1149`
  - `crates/agentdash-application/src/workflow/mod.rs:49`
  - `crates/agentdash-application/src/session/hub/tests.rs:940`
- Concrete code evidence:
  - `AgentRunSteeringService::steer` 通过 runtime session anchor 找 run/agent/frame，校验 terminal agent、active turn、steering support，然后直接调用 `SessionControlService::steer_session` 并写 `UserInputSubmitted`。
  - `AgentRunMailboxService::consume_as_steering` 对同一组事实做校验后执行 steer、写事件、mark mailbox message、complete receipt。
  - `rg` 结果显示 `AgentRunSteeringService` 只在 `workflow/mod.rs` 导出和 `session/hub/tests.rs` 使用，未见 routes/frontend 产品路径使用。
- Impact:
  - direct steer 服务保留了 mailbox 之前的控制语义，和 spec 要求的“composer-submit -> command receipt -> mailbox envelope -> scheduler”并列。
  - 即使当前未被调用，导出 API 和测试会让未来实现误用这条平行路径。
- Suggested cleanup direction:
  - 删除 `AgentRunSteeringService` 或下沉为 mailbox steer executor 的私有 helper。
  - 相关测试改到 mailbox command / scheduler / current AgentFrame resolution 上，不再测试裸 direct steer service。

### Code Patterns

- `SessionExecutionState` 是非持久化查询投影，定义在 `crates/agentdash-application/src/session/types.rs:216`，本身不是问题；问题在于它被多处重复派生不同 command/control view。
- `SessionCoreService::inspect_session_execution_state` 在 `crates/agentdash-application/src/session/core.rs:150` 按 runtime registry 优先、meta fallback 的顺序查询，符合 runtime map / active turn / terminal meta 分层。
- `AgentRunWorkspaceQueryService::delivery_runtime_session_for_agent_run` 和 `resolve_agent_run_frame_vfs` 都通过 anchor/current frame 找 delivery/runtime surface，见 `query.rs:244`、`query.rs:257`；方向符合 spec，但调用被多个投影入口重复。
- `RuntimeGateway::invoke` 在 `crates/agentdash-application/src/runtime_gateway/gateway.rs:82` 先找 provider，再 `validate_request` 和 `provider.supports`；actor/context 校验在 `gateway.rs:147` 之后，符合 spec。
- `RuntimeSessionMcpAccess` 由 `SessionCapabilityService` 实现，见 `crates/agentdash-application/src/session/capability_service.rs:265`，实际 MCP surface 仍通过 SessionHub/AgentFrame surface 发现，符合 runtime-gateway spec。

### External References

- 未使用外部资料。本次为只读内部代码与 Trellis spec review。

### Related Specs

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/session/streaming-protocol.md`
- `.trellis/spec/backend/runtime-gateway.md`

### Not Problems / Boundaries Not Recommended To Change Now

- AgentRun Mailbox 作为 durable message intake / scheduler / recovery 的事实源不是问题。要清理的是服务内部职责过厚，不是移除 mailbox。
- `RuntimeSessionExecutionAnchor` 作为 RuntimeSession trace 到 AgentFrame/Lifecycle control-plane 的 backlink 不是问题。要避免的是 RuntimeSession endpoint 自己承载 AgentRun command projection。
- `ExecutionContext` 作为 connector-facing projection 不是问题。它不应回写为 application facts，当前 spec 与主要代码方向一致。
- `SessionExecutionState` 区分 Idle / Running / Cancelling / Completed / Failed / Interrupted 不是问题。它适合作为查询投影，问题是多个 view 以它为输入重复派生 command/action。
- RuntimeGateway 的 `invoke` / provider boundary 暂不建议和本轮 AgentRun 清理一起动。它对 setup/session/extension action 的统一调用有明确价值；dynamic surface manifest 可另立小任务。
- 前端 `SessionChatView` 作为可复用聊天组件不需要立即拆。优先从后端收敛 command/mailbox projection，前端再减少 `mailboxSnapshot` / `mailboxState` / `mailboxMessages` 的多源输入。

### Follow-up Task Candidates

1. 收敛 AgentRun workspace projection：以 `AgentConversationSnapshot` 作为唯一 chat command/mailbox control surface，删除或降级 top-level `actions` / `replacement_command`。
2. 瘦身 RuntimeSession runtime-control：保留 trace/meta/anchor/workspace ref，移除独立 mailbox/action 投影。
3. 硬化 SessionRuntimeBuilder：把 AgentRun mailbox boundary 依赖改为显式必需依赖，禁止 `with_agent_run_mailbox_boundary` 静默 no-op。
4. 拆分 AgentRuntimeDelegate：把 turn boundary 与 hook/tool/compaction/provider observer 分成小 trait，并让 mailbox delegate 只实现 turn boundary。
5. 拆分 AgentRunMailboxService 内部职责：scheduler、delivery executor、receipt projector、intake 分文件/分端口。
6. 删除或私有化 `AgentRunSteeringService` direct steer 路径，测试迁移到 mailbox steer/scheduler。
7. 单独评估 RuntimeGateway surface manifest：决定 dynamic extension action 是否应出现在 `surface_for_actor` 返回中。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本报告按用户显式指定的 `.trellis/tasks/06-14-module-overdesign-review` 与唯一允许写入路径执行。
- 本次没有运行测试、没有修改业务代码、没有修改 spec。
- 未发现 RuntimeGateway action invocation 本身形成 AgentRun 重复事实源；它更像可保留基础设施边界。
- 未见 `AgentRunSteeringService` 产品路径调用；当前判断为清理候选，删除前仍需跑一次全仓 `rg` 与编译确认导出影响。
