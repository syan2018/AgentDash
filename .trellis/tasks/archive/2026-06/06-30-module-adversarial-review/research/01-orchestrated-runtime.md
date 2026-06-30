# Research: Orchestrated Work Surface + Agent Runtime Session Surface

- Query: 对抗性架构审查 Orchestrated Work Surface 与 Agent Runtime Session Surface，覆盖 Workflow / Lifecycle / Orchestration / Task / Companion / Routine gates，以及 AgentRun / RuntimeSession / RuntimeGateway / mailbox / conversation control / frame construction。
- Scope: internal
- Date: 2026-06-30

## Findings

### Summary

当前代码已经收束了 06-14 baseline 中几条最危险的问题：Task boot projection 不再从缺失 runtime fact 推断失败；SubjectExecutionView 已从 subject association / agent / execution anchor / runtime node 派生 runtime attempts 与 artifacts；RuntimeSession runtime-control 已从 mailbox/action 控制面退回 trace/detail/read-only；subject cancel 已改为提交 `OrchestrationRuntimeEvent::NodeCancelled`。

剩余风险不再主要是“没有 reducer / 没有 mailbox”，而是旧定义层 DTO、宽 runtime delegate、过厚 dispatch facade 与 durable gate 的等待语义仍在跨模块泄漏。建议正式分工仍拆成两个 surface，但需要一个交叉审查点：Companion / Routine / Human gate 与 AgentRun mailbox 的边界。

### Files Found

- `.trellis/spec/backend/workflow/architecture.md` - Lifecycle / Workflow / Orchestration / AgentFrame runtime coordinate 不变量。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - Orchestration reducer、executor launcher、runtime node event contract。
- `.trellis/spec/backend/story-task-runtime.md` - Story / Task / SubjectExecutionView / RuntimeSession trace 边界。
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession、AgentFrame、AgentRun mailbox 与 frame surface 的职责边界。
- `.trellis/spec/backend/session/agentrun-mailbox.md` - AgentRun mailbox envelope、scheduler、turn boundary contract。
- `.trellis/spec/backend/session/runtime-execution-state.md` - runtime registry / active turn / mailbox control actions / runtime-control 边界。
- `.trellis/spec/backend/session/session-startup-pipeline.md` - LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> turn stages。
- `.trellis/spec/backend/runtime-gateway.md` - RuntimeGateway actor/context admission 与 runtime surface query 边界。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 旧 baseline 综合问题索引。
- `.trellis/tasks/06-14-module-overdesign-review/research/01-lifecycle-workflow-task.md` - 旧 Lifecycle / Workflow / Task 问题证据。
- `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` - 旧 AgentRun / Session / RuntimeGateway 问题证据。
- `crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs` - Subject cancel control，现在通过 orchestration reducer 写 cancel。
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs` - LifecycleRunView / SubjectExecutionView read model builder。
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` - Lifecycle dispatch facade，创建 run / agent / frame / session / association / gate / lineage。
- `crates/agentdash-application-lifecycle/src/lifecycle/projection.rs` - active workflow projection，仍含 PlanNode -> ActivityDefinition adapter。
- `crates/agentdash-application-lifecycle/src/lifecycle/gate_service.rs` - generic LifecycleGate create/wait/resolve service。
- `crates/agentdash-application-workflow/src/orchestration/runtime.rs` - OrchestrationRuntimeEvent reducer。
- `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs` - ready node drain 与 semantic executor launch。
- `crates/agentdash-domain/src/workflow/entity.rs` - LifecycleRun aggregate、task facts、run status aggregation。
- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs` - durable gate domain entity。
- `crates/agentdash-api/src/routes/workflows.rs` - lifecycle run create / create-and-continue / continue / drain routes。
- `crates/agentdash-api/src/routes/sessions.rs` - RuntimeSession trace/detail runtime-control route。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - AgentRun workspace route and contract mapping。
- `crates/agentdash-api/src/routes/companion_gates.rs` - human companion gate respond route。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs` - AgentRun workspace snapshot query。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs` - AgentRun workspace shell/projection state helper。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs` - backend command stale guard / availability policy。
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs` - conversation execution / commands / mailbox snapshot resolver。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs` - mailbox facade and split module exports。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs` - mailbox runtime delegate wrapper around AgentRuntimeDelegate。
- `crates/agentdash-application-runtime-session/src/session/hub/mod.rs` - SessionRuntimeInner dependency holder。
- `crates/agentdash-application-runtime-session/src/session/hub/factory.rs` - ready gate and late-injected session dependencies。
- `crates/agentdash-application-runtime-session/src/session/launch/planner.rs` - hook delegate + mailbox runtime delegate composition。
- `crates/agentdash-agent-types/src/runtime/delegate.rs` - AgentRuntimeDelegate trait surface。
- `crates/agentdash-application/src/companion/tools.rs` - companion_request / companion_respond tool behavior and wait polling。
- `crates/agentdash-application/src/companion/gate_control.rs` - companion gate resolve and mailbox delivery。
- `crates/agentdash-application/src/routine/executor.rs` - routine execution dispatch and mailbox reuse path。
- `crates/agentdash-application/src/routine/dispatch.rs` - Routine dispatch strategy -> SubjectExecutionIntent mapping。
- `crates/agentdash-application/src/routine/reuse_resolver.rs` - routine reuse target resolution。
- `crates/agentdash-contracts/src/runtime/workflow.rs` - AgentRunWorkspaceView / AgentConversationSnapshot / SubjectExecutionView / SessionRuntimeControlView contract。
- `packages/app-web/src/generated/workflow-contracts.ts` - generated frontend view shape confirming current wire contract。

### Issue 1: runtime 仍把 `PlanNode` 投影回 `ActivityDefinition`，并对无 executor 节点伪造 `BashExec("true")`

- Priority: P1
- Problem type: 抽象泄漏 / 命名或职责漂移 / residual adapter。
- Baseline status: residual。06-14 `01-lifecycle-workflow-task.md` 的 “PlanNode -> ActivityDefinition 旧 DTO 适配” 仍存在，只是路径从 `agentdash-application/src/workflow/projection.rs` 移到 `agentdash-application-lifecycle` crate。
- Code evidence:
  - `crates/agentdash-application-lifecycle/src/lifecycle/projection.rs:73` 明确把 runtime plan node 转成 activation 需要的 activity shape。
  - `crates/agentdash-application-lifecycle/src/lifecycle/projection.rs:93` 到 `:101` 对 `LocalEffect`、`ExtensionAction` 和 `None` 生成 `ActivityExecutorSpec::Function(BashExec { command: "true" })`。
  - `crates/agentdash-application-lifecycle/src/lifecycle/projection.rs:281` 在 active workflow projection 中消费这个 adapter。
  - `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:441` 在 workflow agent node materialization 中也通过该 adapter 生成 `activity`。
  - `crates/agentdash-application-lifecycle/src/lifecycle/surface/mount.rs:194` 到 `:204` 的测试 fixture 继续用 BashExec 表达“无 agent workflow 绑定 / manual node”。
- First-principles problem:
  - `OrchestrationPlanSnapshot` 已经是 runtime 前冻结的语义计划，runtime activation / frame compose 不应再借 definition-era `ActivityDefinition` 表示当前节点。
  - fake BashExec 让 “没有 executor / local effect / extension action” 被投影成可执行 function executor，后续任何权限、VFS、prompt、surface mount 逻辑读取 executor 时都会看到错误事实。
- Impact:
  - Workflow node frame construction、active workflow projection、lifecycle VFS mount、future extension/local effect executor 都要理解这个兼容 adapter。
  - 新增 `PlanNodeKind` 或 executor spec 时，必须在 runtime adapter 和 definition model 两处同步，继续扩大定义层与运行层耦合。
- Suggested boundary:
  - 删除 runtime path 上的 `ActivityDefinition` adapter，改为窄 DTO，例如 `LifecycleNodeActivationSpec { node_path, attempt, label, ports, completion_policy, procedure_contract?, lifecycle_identity }`，或直接消费 `PlanNode`。
  - `LocalEffect` / `ExtensionAction` / `None` 保持自身 executor absence / typed executor fact，不得填充 BashExec sentinel。
  - `ActivityDefinition` 保留在 WorkflowGraph definition/template 层。

### Issue 2: `LifecycleDispatchService` 仍是跨 run / orchestration / agent / frame / session / association / gate / lineage 的 transaction script

- Priority: P1
- Problem type: 模块过厚 / 横向耦合。
- Baseline status: residual。06-14 已指出 dispatch facade 过厚；当前 crate 拆分后职责注释更清晰，但服务仍承载同一批跨模块 side effects。
- Code evidence:
  - `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:93` 到 `:104` 的注释列出该 service 同时复用/创建 LifecycleRun、OrchestrationInstance、SubjectAssociation、LifecycleAgent、AgentFrame、LifecycleGate、AgentLineage。
  - `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:105` 到 `:121` 的 struct 持有 run、graph、agent、frame、association、gate、lineage、anchor、runtime session creator、frame construction、workflow node materialization、graph planner。
  - `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:378` 到 `:408` 负责 `start_lifecycle_run` 的 graph planning、run create、root orchestration materialization。
  - `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:411` 到 `:450` 的 `materialize_workflow_agent_node` 读取 plan node、构造 lifecycle identity/activity、创建 agent。
  - `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:583` 到 `:607` 写 gate、anchor 并更新 agent delivery binding。
  - `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:615` 到 `:629` 又提交 `NodeStarted` reducer event 并回写 run。
  - 当前文件 `dispatch_service.rs` 为 2174 行。
- First-principles problem:
  - 这个 service 不只是 facade；它拥有多个事实闭包的写入顺序：run/orchestration state、agent identity、frame surface、runtime session trace、subject association、gate、lineage、node started。
  - 任一子域变化都会触碰同一 transaction script，模块边界会按“需要一起写库”而不是“事实源归属”继续漂移。
- Impact:
  - Workflow AgentCall launcher、Routine dispatch、Companion child dispatch、ProjectAgent start 都容易把自己的 gate/lineage/session 特例塞回同一 service。
  - 后续拆 Routine gates 或 Companion gates 时，很难判断 gate 应由 orchestration runtime、AgentRun mailbox，还是 dispatch use case 拥有。
- Suggested boundary:
  - 保留外层 `LifecycleDispatchService` 作为 use-case facade，但内部强制拆成：
    - `RunOrchestrationStarter`: graph/script compile + LifecycleRun/orchestration creation。
    - `AgentRuntimeIdentityAllocator`: LifecycleAgent / AgentFrame / RuntimeSession / anchor / current delivery binding。
    - `SubjectAssociationWriter`: subject/source/control-scope association。
    - `InteractionGateWriter`: LifecycleGate 与 lineage side effect。
    - `WorkflowNodeMaterializer`: workflow AgentCall node materialization output，不拥有 reducer transition。
  - `NodeStarted` 继续只通过 orchestration reducer 写入，materializer 只返回 evidence。

### Issue 3: `create-and-continue` 仍绕过可观察 Ready gate

- Priority: P1
- Problem type: Workflow lifecycle command 语义耦合 / residual convenience path。
- Baseline status: residual, partially resolved。06-14 的 “lifecycle start API 混合 create 和 drain” 已通过 `/lifecycle-runs` create 与 `/{id}/continue` 拆开，但仍保留显式 `create-and-continue` 命令。
- Code evidence:
  - `crates/agentdash-api/src/routes/workflows.rs:136` 注册 `POST /lifecycle-runs` 到 `create_lifecycle_run`。
  - `crates/agentdash-api/src/routes/workflows.rs:138` 到 `:139` 同时注册 `POST /lifecycle-runs/commands/create-and-continue`。
  - `crates/agentdash-api/src/routes/workflows.rs:146` 到 `:151` 注册 `/{id}/continue` 和 `/{id}/drain` 到同一 handler。
  - `crates/agentdash-application-lifecycle/src/lifecycle/run_command_service.rs:48` 到 `:72` 的 create 只调用 `start_lifecycle_run` 并加载 run。
  - `crates/agentdash-application-lifecycle/src/lifecycle/run_command_service.rs:74` 到 `:85` 的 continue 调用 `OrchestrationExecutorLauncher::drain_ready_nodes`。
  - `crates/agentdash-application-lifecycle/src/lifecycle/run_command_service.rs:88` 到 `:94` 的 `create_and_continue_lifecycle_run` 直接串联 create + continue。
- First-principles problem:
  - Ready orchestration 是 human/platform approval、preflight inspection、scheduler admission、batch drain 的明确控制面状态。公开 create-and-continue command 让调用方仍能跳过该状态。
  - 预研阶段不需要保留便利兼容入口；正确形态应让 create 与 continue 是两个可审计 command。
- Impact:
  - Function / HumanGate / AgentCall entry node 可以在一次 create request 内产生 side effect，调用方无法稳定观察 Ready 状态。
  - 人类 gate 和 routine scheduler 后续如果需要显式 admission，会被 create-and-continue 入口绕过。
- Suggested boundary:
  - 删除公开 `create-and-continue` route，或把它降为内部测试 helper；产品 API 只保留 create Ready 与 explicit continue/drain。
  - `drain` 与 `continue` 若语义一致，保留一个命名，另一个不要作为公开 alias。

### Issue 4: Companion durable gate 被工具调用轮询等待，和 AgentRun mailbox / orchestration gate 形成第二套 wait control

- Priority: P1
- Problem type: 重复事实源 / 抽象泄漏 / gate lifecycle 职责漂移。
- Baseline status: resurfaced。06-14 的 companion / permission 双事实源问题已不在本范围内直接复现，但同类风险在 `LifecycleGate` + companion wait + mailbox delivery 中重新出现：平台/人类交互结果同时表现为 gate payload、mailbox delivery 和 tool call blocking wait。
- Code evidence:
  - `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:5` 到 `:21` 将 `LifecycleGate` 定义为 durable wait/review/resume 点，但 `status`、`gate_kind`、`correlation_id` 都是 string，payload 是 `serde_json::Value`。
  - `crates/agentdash-application-lifecycle/src/lifecycle/gate_service.rs:59` 到 `:90` 的 generic `wait_for_gate` 用 500ms poll 和 300s timeout 等 gate resolve。
  - `crates/agentdash-application/src/companion/tools.rs:928` 到 `:935` 的 sub companion `wait=true` 在 Agent tool execution 内调用 `poll_gate_until_resolved`。
  - `crates/agentdash-application/src/companion/tools.rs:1026` 到 `:1048` 的 `poll_gate_until_resolved` 无限轮询直到 cancel。
  - `crates/agentdash-application/src/companion/tools.rs:1228` 到 `:1280` 的 human wait path 直接创建 `LifecycleGate` 并在 tool 内设置 300s deadline。
  - `crates/agentdash-application/src/companion/gate_control.rs:485` 到 `:527` 的 human response 先投递 mailbox，再把 mailbox delivery payload 写回 gate 并 `resolve("companion_respond")`。
  - `crates/agentdash-api/src/routes/companion_gates.rs:51` 到 `:68` 的 API route 为 respond 临时组装 `CompanionGateControlService` 与 mailbox delivery。
- First-principles problem:
  - durable wait 的价值是进程可恢复和可由 scheduler 继续推进；工具函数内 poll 让 wait 生命周期依赖当前 Agent turn 是否仍活着。
  - gate payload 同时保存请求元数据、响应结果和 mailbox delivery result，导致“用户回应事实”和“父 AgentRun 收到回应的 delivery fact”混在同一个 JSON payload。
  - HumanGate 已经在 orchestration runtime 中通过 `NodeStarted(HumanDecision)` / `NodeCompleted` 表达；companion gate 则绕到 Agent tool + mailbox + gate payload，形成第二套 gate advancement。
- Impact:
  - 进程重启、connector cancel、长时间人工等待会让 gate 仍 open，但原 tool call 已经不可恢复地失去等待上下文。
  - UI/API 看到 gate resolved 不等价于 parent AgentRun 已成功消费回应，因为 mailbox delivery failure 也写在 gate payload。
  - Routine / Companion / HumanGate 以后会各自实现 wait/resume，而不是共享一个 control-plane gate boundary。
- Suggested boundary:
  - `LifecycleGate` 只保存 durable gate fact：typed gate kind、owner run/agent/frame、request payload、resolution payload、resolution status。
  - 等待推进不要在 Agent tool 内 poll；应转为 AgentRun mailbox envelope / scheduler continuation，或在 workflow_graph topology 下映射为 `RuntimeNodeState::Blocked/Running(HumanDecision)` 后由 reducer terminal event 推进。
  - mailbox delivery result 应作为 AgentRun command/delivery fact，gate 只引用 delivery receipt id 或 response envelope id，不内嵌完整 delivery payload。
  - Companion human/sub/parent gate 与 orchestration HumanGate 需要同一个 gate resolution port，区别只在 request/response adapter。

### Issue 5: `AgentRuntimeDelegate` 仍是过宽单 trait，mailbox turn boundary 被迫包装 hook / tool / compaction / provider observer

- Priority: P1
- Problem type: 过度抽象 / 过宽边界 / 横向耦合。
- Baseline status: residual。06-14 的 AgentRuntimeDelegate 过宽问题仍存在；mailbox service 已拆模块，但 runtime delegate 组合方式没有根本变化。
- Code evidence:
  - `crates/agentdash-agent-types/src/runtime/delegate.rs:25` 到 `:85` 一个 trait 同时定义 compaction、context transform、tool policy、after_turn、before_stop、provider request observer。
  - `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:147` 到 `:165` 先创建 `HookRuntimeDelegate`，再通过 mailbox runtime port 包装成单个 `DynAgentRuntimeDelegate`。
  - `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:145` 到 `:148` 的 `AgentRunMailboxRuntimeDelegate` 持有 `inner: Option<Arc<dyn AgentRuntimeDelegate>>`。
  - `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:356` 到 `:422` mailbox delegate 必须转发 compaction、context transform、tool hooks。
  - `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:424` 到 `:467` 才是 mailbox after-turn scheduling 逻辑。
  - `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:469` 到 `:500` 才是 before-stop mailbox drain / hook delivery routing。
  - `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs` 当前为 2216 行。
- First-principles problem:
  - Mailbox 只拥有 AgentRun turn-boundary delivery/scheduler 事实；它不应知道 compaction、tool policy、provider observer 方法集合。
  - Hook runtime 是 context/tool/provider/turn observer 的组合；mailbox 是 turn-boundary scheduler。二者通过 decorator order 组合，会把“谁先处理 after_turn/before_stop”变成 launch planner 的隐式事实。
- Impact:
  - 每新增一个 agent loop extension point，mailbox wrapper 必须继续转发，turn-boundary concern 被横向耦合到其它 runtime concern。
  - Hook follow-up、mailbox steering、BeforeStop continuation 的优先级不在类型系统中表达，只能读 wrapper 顺序。
- Suggested boundary:
  - 拆为小 trait 或 delegate set：`ContextTransformDelegate`、`ToolCallPolicyDelegate`、`CompactionDelegate`、`TurnBoundaryDelegate`、`ProviderRequestObserver`。
  - Agent loop 接收结构化 `RuntimeDelegateSet`；mailbox 只实现 `TurnBoundaryDelegate`，hook runtime 按需实现多个 trait。
  - LaunchPlan 显式记录 hook delegate 与 mailbox delegate 的组合顺序，而不是 `Arc<dyn AgentRuntimeDelegate>` decorator。

### Issue 6: AgentRun workspace 内部仍保留 shell projection 与 conversation command projection 两套状态语言

- Priority: P2
- Problem type: 命名或职责漂移 / residual read-model duplication。
- Baseline status: residual but mostly resolved。06-14 的 top-level actions/mailbox/control 重复已经从 wire contract 大幅收束；当前剩余是 application 内部 projection state 与 conversation snapshot 并存。
- Code evidence:
  - `crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:19` 到 `:34` 的 `AgentRunWorkspaceSnapshot` 同时带 `projection`、`mailbox`、`mailbox_messages`、`conversation`。
  - `crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:219` 到 `:225` 的 `AgentRunWorkspaceProjectionModel` 仍表达 `state_code`、`active_turn_id`、`last_turn_id`、`delivery_status`。
  - `crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:12` 到 `:23` 从 `SessionExecutionState` 派生 workspace projection。
  - `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:457` 到 `:487` 通过 `ConversationCommandAvailabilityResolver` 派生 `execution_status`、snapshot id、active turn 和 commands。
  - `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:158` 到 `:160` 先派生 workspace projection；`:213` 到 `:225` 又用同一 execution state 构造 `AgentConversationSnapshot`。
  - `crates/agentdash-api/src/routes/lifecycle_agents.rs:1021` 到 `:1023` 当前 contract 的 control plane 已从 conversation 派生，而不是从 `snapshot.projection` 派生。
  - `crates/agentdash-contracts/src/runtime/workflow.rs:1167` 到 `:1203` 的 `AgentRunWorkspaceView` 不再包含 top-level `actions` 或 `mailbox_messages`；command/mailbox 位于 `conversation`。
- First-principles problem:
  - 如果 conversation 是唯一 command/mailbox control surface，workspace projection 不应继续表达状态语言；否则内部调用者可能重新把 `state_code` 当可执行状态事实源。
  - 当前 projection 主要服务 shell display / list status，命名却仍像 workspace runtime control model。
- Impact:
  - 新 command 或新的 cancelling/starting 状态可能需要同步 `AgentRunWorkspaceProjection` 与 conversation execution status。
  - 前端 contract 已收束，但 application 内部还保留旧控制面词汇，未来易反向扩回 API。
- Suggested boundary:
  - 将 `AgentRunWorkspaceProjectionModel` 重命名/收窄为 `AgentRunWorkspaceShellStatus`，只输出 display/list 所需 `delivery_status`、`last_turn_id`。
  - command availability、mailbox user attention、control plane status 只从 `AgentConversationSnapshot` / `ConversationCommandAvailabilityResolver` 派生。
  - `AgentRunWorkspaceSnapshot` 不再携带独立 `projection`，route 如需 `control_plane` 继续从 conversation 派生。

### Issue 7: `SessionRuntimeInner` ready gate 已硬化，但仍是跨 session launch / hook / runtime transition / tool adoption 的服务定位器

- Priority: P2
- Problem type: 模块过厚 / residual composition hub。
- Baseline status: residual with resolved sub-risk。06-14 的 mailbox boundary 静默 no-op 已解决；当前 ready gate 检查 mailbox/runtime surface/hook target 等依赖。但 inner 仍承载过多运行子域。
- Code evidence:
  - `crates/agentdash-application-runtime-session/src/session/hub/mod.rs:1` 到 `:12` 文件注释承认该模块是“尚待下沉”的 session 内部实现，并列出 facade/factory/tool_builder/hook_dispatch/runtime_context_transition。
  - `crates/agentdash-application-runtime-session/src/session/hub/mod.rs:47` 到 `:103` 的 `SessionRuntimeInner` 持有 connector、hook provider、runtime registry、turn supervisor、stores、persistence、VFS、skill discovery、terminal callback、frame launch envelope provider、accepted commit port、audit bus、settings、runtime tool provider、MCP discovery、backend execution、AgentFrame repo、anchor repo、runtime surface query、LifecycleAgent repo、permission grant repo、capability/hook/mailbox ports、LifecycleGate repo。
  - `crates/agentdash-application-runtime-session/src/session/hub/factory.rs:317` 到 `:366` ready gate 现在显式检查 runtime_tool_provider、mcp_tool_discovery、terminal_callback、frame_launch_envelope_provider、accepted_launch_commit_port、backend lease、agent frame、anchor、runtime surface、lifecycle agent、effective capability、hook target、mailbox runtime port。
- First-principles problem:
  - Ready gate 解决了“缺依赖静默运行”，但 `SessionRuntimeInner` 仍让 launch、hook dispatch、tool refresh、runtime context transition、surface adoption 共享一个大依赖对象。
  - 这会让 RuntimeSession layer 继续吸收 AgentRun frame/surface/gate 依赖，和 “RuntimeSession 是 delivery / trace substrate” 的目标边界相冲突。
- Impact:
  - 新的 runtime surface update、gate continuation 或 hook auto-resume 容易继续往 hub 塞 port。
  - 测试和 bootstrap 可以验证 ready，但很难从类型上限制具体 service 只能看到自己需要的依赖。
- Suggested boundary:
  - 把 inner 拆成明确依赖包：`SessionDeliveryRuntimeDeps`、`SessionLaunchDeps`、`HookRuntimeDeps`、`RuntimeSurfaceAdoptionDeps`、`AgentRunTurnBoundaryDeps`。
  - `SessionRuntimeInner` 只保留 composition root / ready gate；业务 service 构造时接收窄 deps，而不是 clone 整个 inner。

### Resolved / Superseded Baseline Checks

- resolved: cancel 绕过 reducer。当前 `subject_execution_control.rs:243` 到 `:253` 调用 `apply_orchestration_event_to_run(... NodeCancelled ...)`，不再直接改 `RuntimeNodeState`。
- resolved: Task boot projection 错误 association scope / absence -> Failed fallback。当前 `task/view_projector.rs:45` 到 `:58` 只记录跳过 boot projection，声明 runtime state 由 SubjectExecutionView 派生。
- resolved: SubjectExecutionView 空 runtime node / artifacts。当前 `run_view_builder.rs:106` 到 `:149` 从 `list_by_subject`、runtime attempts、latest node、artifacts 构建 view；`:307` 到 `:345` 从 execution anchor 定位 orchestration node。
- resolved: `/tasks/{id}/execution` 平行轻量 DTO。当前未找到 `task_execution.rs` 路由文件；`rg "TaskExecutionView|/tasks/{id}/execution|task_execution"` 未发现旧产品路径。
- resolved: RuntimeSession runtime-control 作为第二个 AgentRun action/mailbox 入口。当前 `sessions.rs:153` 到 `:191` 只返回 session meta、control plane、anchor、run、agent、frame runtime 和 associations；`SessionRuntimeControlView` contract 在 `workflow.rs:1419` 到 `:1437` 不含 actions/mailbox/messages。
- superseded: AgentRun direct steer service。旧产品路径未见 `AgentRunSteeringService`；当前只在 `crates/agentdash-application/src/test_support/agent_run_steering.rs` 保留测试支撑，产品输入进入 mailbox。
- resolved: SessionRuntime mailbox boundary 静默 no-op。当前 `factory.rs:363` 到 `:365` ready gate 要求 `mailbox_runtime_port` 存在。

### Code Patterns

- Correct pattern: runtime node terminal/cancel facts go through reducer.
  - `crates/agentdash-application-workflow/src/orchestration/runtime.rs:266` to `:283` applies event to run then refreshes lifecycle status.
  - `crates/agentdash-application-lifecycle/src/lifecycle/subject_execution_control.rs:243` to `:253` uses `NodeCancelled` reducer event.
  - `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:366` to `:368` persists executor events through `apply_orchestration_event_to_run`.
- Correct pattern: task/story runtime read model starts from subject.
  - `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:110` to `:115` calls `list_by_subject` and loads related runs.
  - `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:224` to `:235` traverses execution anchors for runtime attempts.
- Risk pattern: definition DTO used in runtime.
  - `crates/agentdash-application-lifecycle/src/lifecycle/projection.rs:77` converts `PlanNode` to `ActivityDefinition`.
  - `crates/agentdash-application-lifecycle/src/lifecycle/projection.rs:93` to `:101` inserts fake BashExec for non-function nodes.
- Risk pattern: durable gate waits via polling in tool call.
  - `crates/agentdash-application/src/companion/tools.rs:1026` to `:1048` loops on lifecycle_gate_repo.
  - `crates/agentdash-application/src/companion/tools.rs:1277` to `:1316` duplicates the human wait loop with timeout.
- Risk pattern: wide runtime delegate forces wrapper forwarding.
  - `crates/agentdash-agent-types/src/runtime/delegate.rs:25` to `:85` defines the wide trait.
  - `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:356` to `:422` forwards unrelated methods.

### External References

- None. This review used repository code and local Trellis specs only.

### Related Specs

- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/runtime-gateway.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`

### Boundary Recommendation

Keep the official review split as two surfaces, but do not merge them:

- Orchestrated Work Surface should own `WorkflowGraph -> OrchestrationPlanSnapshot -> LifecycleRun.orchestrations[] -> RuntimeNodeState` and explicit lifecycle commands (`create`, `continue`, `cancel`, `human decision`).
- Agent Runtime Session Surface should own `AgentRun workspace -> mailbox envelope -> scheduler -> RuntimeSession delivery/trace -> frame surface`.

Add one cross-surface review slice for gates:

- `LifecycleGate`, HumanGate, companion human/sub/parent request, routine dispatch/reuse, and mailbox continuation should be reviewed together because their current overlap is not visible from either surface alone.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell. This file uses the explicit user-provided task path and write target.
- No business code, tests, specs, config, or git state were modified.
- No full test suite was run, per request.
- This review did not inspect Extension / Workspace Module Runtime Surface, Authority & Capability Runtime, VFS / Local / Relay, or broader frontend contracts except where they touched the requested surfaces.
