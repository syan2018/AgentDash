# Research: backend-core-gap

- Query: 后端 core/control-plane 依赖扫描，聚焦 `agentdash-domain` 与 `agentdash-application` 中 lifecycle / workflow / session / runtime / hook / capability / context 模块当前对运行谓词的依赖与 gap。
- Scope: internal
- Date: 2026-06-01

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-domain/src/workflow/entity.rs` | 定义 `WorkflowDefinition`、`ActivityLifecycleDefinition`、`ActivityExecutionClaim`、`LifecycleRun`，当前生命周期运行主实体。 |
| `crates/agentdash-domain/src/workflow/value_objects/run_state.rs` | 定义 `ActivityAttemptState`、`ActivityLifecycleRunState`、`ExecutorRunRef`、`LifecycleExecutionEntry`。 |
| `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs` | 定义 activity 图节点、executor spec、`AgentSessionPolicy`。 |
| `crates/agentdash-domain/src/workflow/value_objects/contract.rs` | 定义 workflow contract、hook/capability/output/input ports 与 `EffectiveSessionContract`。 |
| `crates/agentdash-domain/src/workflow/value_objects/lifecycle_def.rs` | 定义 `LifecycleNodeType`，当前仍把 node type 与 session 策略耦合。 |
| `crates/agentdash-domain/src/workflow/run_link.rs` | 定义 `LifecycleRunLink`，把 run 与 Story/Task/Project 等 subject 关联。 |
| `crates/agentdash-domain/src/task/entity.rs` | Task 作为用户视图/投影对象，但仍保留 lifecycle step 与 artifact 投影接口。 |
| `crates/agentdash-domain/src/context_source.rs` | 定义 context source 引用。 |
| `crates/agentdash-domain/src/session_composition.rs` | 定义 session composition，命名上仍混入 `workflow_steps`。 |
| `crates/agentdash-domain/src/context_container.rs` | 定义 context container/mount 能力声明。 |
| `crates/agentdash-application/src/workflow/engine.rs` | `ActivityEvent -> LifecycleEngine`，Activity run state 的纯推进器。 |
| `crates/agentdash-application/src/workflow/scheduler.rs` | claim/launch ready attempt，写入 `ActivityExecutionClaim` 与 `ExecutorRunRef`。 |
| `crates/agentdash-application/src/workflow/activity_run.rs` | 启动 lifecycle run，但入参和冲突判断仍以 `session_id` 为锚。 |
| `crates/agentdash-application/src/workflow/orchestrator.rs` | session terminal 与 advance tool 到 activity event 的桥接器。 |
| `crates/agentdash-application/src/workflow/agent_executor.rs` | activity executor 到 session/runtime/capability 的桥接，负责 SpawnChild / ContinueRoot。 |
| `crates/agentdash-application/src/workflow/session_association.rs` | session 到 lifecycle activity attempt 的关联解析。 |
| `crates/agentdash-application/src/workflow/projection.rs` | 按 session 解析 active workflow projection。 |
| `crates/agentdash-application/src/workflow/session_run_context_resolver.rs` | 从 session 找 run，再由 run links 推导 `SessionRunContext`/CapabilityScope。 |
| `crates/agentdash-application/src/workflow/freeform.rs` | 为 freeform session 创建默认 lifecycle run。 |
| `crates/agentdash-application/src/workflow/run_link_service.rs` | run 与 subject 的应用服务。 |
| `crates/agentdash-application/src/workflow/step_activation.rs` | activity step 激活后计算 capability/MCP/VFS/runtime context transition。 |
| `crates/agentdash-application/src/session/construction.rs` | `SessionConstructionPlan` 聚合 owner/workspace/context/capability/runtime projection。 |
| `crates/agentdash-application/src/session/construction_provider.rs` | launch construction provider 边界，输入仍以 session/meta/requested commands 为中心。 |
| `crates/agentdash-application/src/session/construction_use_case.rs` | session launch construction 主流程，Task launch path 当前未接入。 |
| `crates/agentdash-application/src/session/construction_planner.rs` | Task/Story/Project context query planner，也会按 session resolve active workflow。 |
| `crates/agentdash-application/src/session/launch/command.rs` | `LaunchCommand` 和 launch source，含 TaskService/WorkflowOrchestrator/HookAutoResume。 |
| `crates/agentdash-application/src/session/launch/plan.rs` | `LaunchPlan`，把 construction/capability/hook/context 投影成 `ExecutionContext`。 |
| `crates/agentdash-application/src/session/launch/preparation.rs` | turn 准备阶段写入 `SessionRuntime`/`TurnExecution` 并投递 context frames。 |
| `crates/agentdash-application/src/session/hub_support.rs` | in-memory `SessionRuntime`、`SessionProfile`、`TurnExecution`。 |
| `crates/agentdash-application/src/session/runtime_registry.rs` | 以 `session_id` 为 key 的 runtime registry。 |
| `crates/agentdash-application/src/session/runtime_control.rs` | cancel/recover interrupted session 的 runtime 控制。 |
| `crates/agentdash-application/src/session/persistence.rs` | session meta/events/runtime commands/projections/lineage store 聚合。 |
| `crates/agentdash-application/src/session/eventing.rs` | session event/context frame 持久化与 projection 更新。 |
| `crates/agentdash-application/src/session/context_projector.rs` | 按 session events 构建 model context。 |
| `crates/agentdash-application/src/session/capability_state.rs` | runtime capability transition、dimension registry、pending transition replay。 |
| `crates/agentdash-application/src/session/capability_service.rs` | session capability facade，读取/更新 live 或 pending capability state。 |
| `crates/agentdash-application/src/session/capability_projection.rs` | 由 VFS/MCP/skill/guidelines 派生 session capability projection。 |
| `crates/agentdash-application/src/session/context_frame.rs` | 构建/排队 context frame。 |
| `crates/agentdash-application/src/session/hook_runtime.rs` | session 级 hook runtime 快照、trace、pending actions、token stats、capabilities。 |
| `crates/agentdash-application/src/session/hook_delegate.rs` | connector/agent runtime delegate，执行 hook 并把 injection 写回 turn audit。 |
| `crates/agentdash-application/src/session/hub/hook_dispatch.rs` | hub 侧 hook trigger、snapshot 重建、runtime context injection、auto-resume。 |
| `crates/agentdash-application/src/hooks/provider.rs` | app hook provider，从 active workflow/run links 构建 hook snapshot。 |
| `crates/agentdash-application/src/hooks/workflow_snapshot.rs` | 按 session 解析 active workflow projection，并 flush execution log。 |
| `crates/agentdash-application/src/hooks/owner_resolver.rs` | 从 `LifecycleRunLink` 构建 `SessionRunContext`。 |
| `crates/agentdash-application/src/hooks/rules.rs` | global/contract/owner hook rules 汇总执行。 |
| `crates/agentdash-application/src/capability/resolver.rs` | capability resolver，输入 owner/context/contributions/grants。 |
| `crates/agentdash-application/src/capability/session_workflow_context.rs` | 从 ProjectAgent/default lifecycle 派生 session workflow capability contribution。 |
| `crates/agentdash-application/src/context/builder.rs` | session context bundle 纯构建器。 |
| `crates/agentdash-application/src/context/workflow_bindings.rs` | workflow binding context contribution，仍标记 legacy source。 |
| `crates/agentdash-application/src/context/rendering/workflow_injection.rs` | workflow injection markdown 渲染公共路径。 |
| `crates/agentdash-spi/src/session_persistence.rs` | session persistence DTO，包括 pending/runtime capability command 与 `SessionMeta`。 |
| `crates/agentdash-spi/src/hooks/mod.rs` | hook SPI DTO，包括 `SessionRunContext`、`SessionHookSnapshot`、`ActiveWorkflowMeta`、`ContextFrame`。 |

### Code Patterns

#### 1. Domain workflow/lifecycle/run state

- `WorkflowDefinition` 当前是“单 Agent 工作流/contract 定义”实体，字段含 `binding_kinds`、`contract`、`source`、`installed_source`、`version` 等，定义点在 `crates/agentdash-domain/src/workflow/entity.rs:16`。它不是 activity graph 本体，但名称是 Workflow。
- `ActivityLifecycleDefinition` 当前承载 graph 配置，字段含 `entry_activity_key`、`activities`、`transitions`，定义点在 `crates/agentdash-domain/src/workflow/entity.rs:77`。这与目标“Workflow 是 Lifecycle 下单个 Activity 演化用可执行图配置”接近，但当前仍叫 ActivityLifecycleDefinition。
- `LifecycleRun` 是当前 run 控制面实体，字段含 `lifecycle_id`、`session_id: Option<String>`、`status`、`active_node_keys`、`execution_log`、`activity_state`，定义点在 `crates/agentdash-domain/src/workflow/entity.rs:197` 和 `crates/agentdash-domain/src/workflow/entity.rs:205`。`bind_runtime_session` 直接写 session id，见 `crates/agentdash-domain/src/workflow/entity.rs:255`。
- `ActivityAttemptState` 保留 activity 执行证据，字段含 `activity_key`、`attempt`、`status`、`executor_run`、started/completed/summary，定义点在 `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:24`。`ExecutorRunRef::AgentSession { session_id }` 把 attempt 与 session 直接相连，见 `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:86`。
- `ActivityExecutionClaim` 是 durable claim，含 `run_id`、`activity_key`、`attempt`、`executor_kind`、`executor_run_ref`、idempotency key，定义点在 `crates/agentdash-domain/src/workflow/entity.rs:97`。这适合作为 attempt 执行证据，不适合作为 actor/runtime 事实源。
- `LifecycleRunLink` 是 run 到 subject 的关系表，字段含 `run_id`、`subject_kind`、`subject_id`、`role`，定义点在 `crates/agentdash-domain/src/workflow/run_link.rs:13`；`RunLinkSubjectKind::Task` 仍存在于 `crates/agentdash-domain/src/workflow/run_link.rs:50`，`RunLinkRole` 包含 Source/Subject/ProjectionTarget/ControlScope/SpawnedBy，见 `crates/agentdash-domain/src/workflow/run_link.rs:86`。

Gap:

- 当前 LifecycleRun 同时承担生命周期状态、activity state、execution log 和“关联 runtime session”入口。目标应保留 LifecycleRun 作为 Lifecycle control-plane run；session/runtime 事实迁到 Actor/ActorFrame，`LifecycleRun.session_id` 不应继续作为当前 activity/runtime 的主锚。
- `WorkflowDefinition` 与 `ActivityLifecycleDefinition` 命名反向接近目标语义：当前 graph 叫 ActivityLifecycleDefinition，当前 per-agent contract 叫 WorkflowDefinition。建议在概念层拆开“Lifecycle/Activity graph config”和“Actor/Activity procedure contract”，避免把 Workflow 同时理解成 graph、agent prompt contract、active workflow projection。
- `LifecycleNodeType` 当前含 AgentNode/PhaseNode，并在注释中绑定“创建独立 session / 切换已有 session contract”，定义点 `crates/agentdash-domain/src/workflow/value_objects/lifecycle_def.rs:6`。这把 lifecycle topology 与 runtime session 策略绑死，应迁到 ActorFrame activation/RuntimeSession binding policy。

#### 2. Domain Task/context

- Task 注释明确 Task 是用户工作项视图，真实执行在 Session/LifecycleRun，定义点 `crates/agentdash-domain/src/task/entity.rs:23`。Task 字段仍有 `lifecycle_step_key`，见 `crates/agentdash-domain/src/task/entity.rs:34`；状态由 `apply_projection` 从 attempt 投影，见 `crates/agentdash-domain/src/task/entity.rs:89`；artifact 仍有公共 `push_artifact` 入口，见 `crates/agentdash-domain/src/task/entity.rs:119`。
- `WorkflowBindingKind` 已无 Task 变体，见 `crates/agentdash-domain/src/workflow/value_objects/binding.rs:10`，说明 Task 不应作为 workflow binding owner。
- `ContextSourceRef` 是 context 输入源引用，定义点 `crates/agentdash-domain/src/context_source.rs:38`；`SessionComposition` 仍有 `workflow_steps` 命名，见 `crates/agentdash-domain/src/session_composition.rs:4`。

Gap:

- Task 的运行时语义已被削弱，但 `lifecycle_step_key`、artifact push、TaskService launch path 仍容易被误读成 Task 拥有 execution/session。目标里 Task 应作为 SubjectRef / Activity payload / projection target，不做 runtime owner。
- Context domain 的 source/container 可以保留为事实声明；session composition 命名需要向 ActorFrame composition/context plan 收束，避免“session == actor”的默认推断。

#### 3. Application workflow engine/scheduler

- `ActivityEvent` 是 activity run state 的唯一推进输入，定义点 `crates/agentdash-application/src/workflow/engine.rs:13`；`LifecycleEngine` 在 `initialize` 创建 ready/pending attempts，见 `crates/agentdash-application/src/workflow/engine.rs:108` 和 `crates/agentdash-application/src/workflow/engine.rs:111`；`apply_event` 更新 state，见 `crates/agentdash-application/src/workflow/engine.rs:157`。
- `complete_attempt` 校验 completion policy 并写 output artifacts，见 `crates/agentdash-application/src/workflow/engine.rs:256`；`advance_successors` 与 `create_ready_attempt` 负责 successor activation，见 `crates/agentdash-application/src/workflow/engine.rs:337` 和 `crates/agentdash-application/src/workflow/engine.rs:377`。
- `ActivityExecutorScheduler` claim ready attempt，见 `crates/agentdash-application/src/workflow/scheduler.rs:12`；`ActivityExecutorStartResult` 含 `executor_run`，见 `crates/agentdash-application/src/workflow/scheduler.rs:46`；`record_executor_started` 将 executor ref 写回 attempt/claim，见 `crates/agentdash-application/src/workflow/scheduler.rs:194`。

Gap:

- Engine/scheduler 作为 activity attempt evidence 层相对干净，适合保留 `ActivityAttemptState` 名称。迁移目标是让 scheduler 产出或关联 Actor/ActorFrame assignment，而不是让 `executor_run_ref.session_id` 成为长期运行锚。
- `ActivityEvent::ExecutorStarted` 与 `ExecutorRunRef` 需要表达 Actor/RuntimeSession 两级：activity attempt 记录“哪个 Actor/ActorFrame 执行过”，RuntimeSession 是执行证据的一部分，不是 LifecycleRun 的主属性。

#### 4. Application activity run/orchestrator/association

- `StartActivityLifecycleRunCommand` 必填 `session_id`，见 `crates/agentdash-application/src/workflow/activity_run.rs:22` 和 `crates/agentdash-application/src/workflow/activity_run.rs:26`。`start_run` 通过 `list_by_session` 拒绝同 session 活跃 run，见 `crates/agentdash-application/src/workflow/activity_run.rs:48`，并用 `LifecycleRun::new_activity(... Some(cmd.session_id) ...)` 创建 run，见 `crates/agentdash-application/src/workflow/activity_run.rs:67`。
- `LifecycleOrchestrator` 是 session terminal/advance tool 到 activity event 的桥，定义点 `crates/agentdash-application/src/workflow/orchestrator.rs:83`。它在 `on_session_terminal`/`advance_current_activity` 中都调用 session association，见 `crates/agentdash-application/src/workflow/orchestrator.rs:118`、`crates/agentdash-application/src/workflow/orchestrator.rs:137`、`crates/agentdash-application/src/workflow/orchestrator.rs:178`。
- `launch_ready_activity_attempts` 把 `run.session_id.clone().unwrap_or_default()` 作为 `root_session_id` 传给 agent executor，见 `crates/agentdash-application/src/workflow/orchestrator.rs:287` 和 `crates/agentdash-application/src/workflow/orchestrator.rs:300`。
- `session_association` 已有 label 格式 `lifecycle_activity:{run_id}:{activity_key}#{attempt}`，见 `crates/agentdash-application/src/workflow/session_association.rs:6` 和 `crates/agentdash-application/src/workflow/session_association.rs:28`，但 resolver 注释和实现实际直接走 `LifecycleRunRepository.list_by_session`，见 `crates/agentdash-application/src/workflow/session_association.rs:48`、`crates/agentdash-application/src/workflow/session_association.rs:50`、`crates/agentdash-application/src/workflow/session_association.rs:55`。

Gap:

- 这里是 session vs actor 最大混淆点：单个 `LifecycleRun.session_id` 被用作 root session、activity terminal 归属、advance tool 归属、active workflow projection 入口。目标体系应改为 `LifecycleRun -> ActivityAttempt -> Actor/ActorFrame assignment -> RuntimeSession`，terminal/advance 根据 ActorFrame/RuntimeSession 的 binding 反查 attempt。
- 当前 label builder 存在但未成为 resolver 主路径，说明系统已经意识到 attempt-level association 需要独立身份，但实际仍被 `list_by_session` 吸回 session 级。
- 多 activity 并行、SpawnChild、多 RuntimeSession 同属一个 Actor 的场景会被单 `session_id` 模型卡住。迁移风险集中在 terminal event、advance tool、active workflow projection、hook snapshot 四条都依赖同一旧路径。

#### 5. Application agent executor/step activation

- `AgentActivityLaunchContext` 含 `project_id`、`lifecycle_key`、`root_session_id`，见 `crates/agentdash-application/src/workflow/agent_executor.rs:36` 和 `crates/agentdash-application/src/workflow/agent_executor.rs:39`。
- ContinueRoot 会 `ensure_hook_session_runtime(root_session_id)`、读取 runtime MCP/capability、激活 step，然后 live apply 或 enqueue pending runtime transition，见 `crates/agentdash-application/src/workflow/agent_executor.rs:192`、`crates/agentdash-application/src/workflow/agent_executor.rs:230`、`crates/agentdash-application/src/workflow/agent_executor.rs:240`、`crates/agentdash-application/src/workflow/agent_executor.rs:282`、`crates/agentdash-application/src/workflow/agent_executor.rs:343`、`crates/agentdash-application/src/workflow/agent_executor.rs:346`。
- SpawnChild 创建新 session，并返回 `ExecutorRunRef::AgentSession { session_id }`，见 `crates/agentdash-application/src/workflow/agent_executor.rs:448` 和 `crates/agentdash-application/src/workflow/agent_executor.rs:485`。ContinueRoot 返回 root session id，见 `crates/agentdash-application/src/workflow/agent_executor.rs:489`、`crates/agentdash-application/src/workflow/agent_executor.rs:527`。
- `StepActivationInput` 以 owner_ctx、active_activity、workflow、run_id、lifecycle_key 等计算 activation，见 `crates/agentdash-application/src/workflow/step_activation.rs:44`；`activate_step_with_platform` 是纯计算，见 `crates/agentdash-application/src/workflow/step_activation.rs:121`；`apply_to_running_session` 直接对 hook session id 读取/写入 current capability，见 `crates/agentdash-application/src/workflow/step_activation.rs:283`、`crates/agentdash-application/src/workflow/step_activation.rs:293`、`crates/agentdash-application/src/workflow/step_activation.rs:302`。

Gap:

- SpawnChild/ContinueRoot 是 RuntimeSession 策略，不应作为 Lifecycle node 类型或 LifecycleRun 字段派生。目标应由 ActorFrame 管理 capability/context/runtime session 事实，activity attempt 只记录 assignment 和 execution evidence。
- `root_session_id` 作为 lifecycle root actor 的替代品使用；Actor 高层封装缺失导致 root/child/continue 的关系只能靠 session id 和 executor config 传递。
- Runtime capability transition 当前写入 session pending command，迁移后应变成 ActorFrame revision/transition；RuntimeSession 只承载连接器执行实例。

#### 6. Active workflow projection/hook snapshot

- `ActiveWorkflowProjection` 绑定 run、lifecycle、active_activity、node type、active workflow key、primary workflow，定义点 `crates/agentdash-application/src/workflow/projection.rs:19`。
- `derive_node_facts` 从 `AgentSessionPolicy` 推导 `LifecycleNodeType`，见 `crates/agentdash-application/src/workflow/projection.rs:50`；`resolve_active_workflow_projection_for_session` 通过 session association 解析，见 `crates/agentdash-application/src/workflow/projection.rs:68` 和 `crates/agentdash-application/src/workflow/projection.rs:75`。
- `WorkflowSnapshotBuilder.resolve_active_workflow` 继续以 session id 为入口，见 `crates/agentdash-application/src/hooks/workflow_snapshot.rs:47` 和 `crates/agentdash-application/src/hooks/workflow_snapshot.rs:51`。
- `AppExecutionHookProvider.load_session_snapshot` 构建 `SessionHookSnapshot`，若命中 active workflow，就设置 `snapshot.run_context`、`metadata.active_workflow`、workflow tags/injections，见 `crates/agentdash-application/src/hooks/provider.rs:95`、`crates/agentdash-application/src/hooks/provider.rs:123`、`crates/agentdash-application/src/hooks/provider.rs:136`、`crates/agentdash-application/src/hooks/provider.rs:185`、`crates/agentdash-application/src/hooks/provider.rs:202`。
- SPI 中 `SessionRunContext` 明确写着“通过 `LifecycleRun.session_id -> run -> links -> subjects` 路径获取”，定义点 `crates/agentdash-spi/src/hooks/mod.rs:39`；`SessionHookSnapshot` 直接含 `session_id` 和 `run_context`，见 `crates/agentdash-spi/src/hooks/mod.rs:83`；`ActiveWorkflowMeta` 位于 hook snapshot metadata，见 `crates/agentdash-spi/src/hooks/mod.rs:123`。

Gap:

- Hook snapshot 是目前 active workflow/lifecycle/run context 最集中的 projection，但它仍是 `SessionHookSnapshot`，即 ActorFrame 应有的 context/capability/workflow facts 被挂在 session runtime。
- `ActiveWorkflowMeta` 把 lifecycle id/key、run status、step、workflow key、effective contract、ports 都投到 hook metadata。目标里这些应由 ActorFrame 从 Lifecycle/Workflow/ActivityAttempt 事实投影，而不是 hook provider 每次按 session 解析。
- `WorkflowSnapshotBuilder.append_execution_log` 会把 hook pending execution log flush 回 LifecycleRun，见 `crates/agentdash-application/src/hooks/workflow_snapshot.rs:61`。这条可以保留为 ActivityAttempt/Lifecycle 证据写入，但不应由 session hook runtime 独占。

#### 7. Session construction/launch/runtime

- `SessionConstructionPlan` 当前聚合 session id、owner、workspace、execution profile、surface、context、prompt、identity、effects、projections、resolution、trace，定义点 `crates/agentdash-application/src/session/construction.rs:67`。`ConstructionProjections` 含 `capability_state`、session capabilities、guidelines、extension runtime，见 `crates/agentdash-application/src/session/construction.rs:139`；`ConstructionResolutionPlan` 含 vfs/mcp/capability/executor/working-directory source 与 runtime base capability state，见 `crates/agentdash-application/src/session/construction.rs:148`。
- `validate_for_launch` 要求 final execution facts 完整且 capability/vfs/skill projection 一致，见 `crates/agentdash-application/src/session/construction.rs:286`。
- `build_session_construction_for_launch` 当前参数含 `_task_input`，但该路径未使用，见 `crates/agentdash-application/src/session/construction_use_case.rs:80` 和 `crates/agentdash-application/src/session/construction_use_case.rs:84`。主流程先 `resolve_session_scope`，再 finalize projection，见 `crates/agentdash-application/src/session/construction_use_case.rs:110`、`crates/agentdash-application/src/session/construction_use_case.rs:204`、`crates/agentdash-application/src/session/construction_use_case.rs:221`。
- `build_task_owner_prompt_request` 存在但为死路径，见 `crates/agentdash-application/src/session/construction_use_case.rs:907`。
- `LaunchCommand` 支持 `WorkflowOrchestrator` 和 `TaskService` source，见 `crates/agentdash-application/src/session/launch/command.rs:10`、`crates/agentdash-application/src/session/launch/command.rs:158`、`crates/agentdash-application/src/session/launch/command.rs:177`。
- `LaunchPlan` 含 `LifecycleLaunchPlan`，但这里的 lifecycle 是 `SessionPromptLifecycle`，定义点 `crates/agentdash-application/src/session/launch/plan.rs:59`；`LaunchPlan` 在 `ExecutionSessionFrame`/`ExecutionTurnFrame` 中塞入 hook session、capability state、runtime delegate、context frames、tools，见 `crates/agentdash-application/src/session/launch/plan.rs:107`、`crates/agentdash-application/src/session/launch/plan.rs:255`、`crates/agentdash-application/src/session/launch/plan.rs:270`。
- `PreparedTurn` 含 accepted context frames、pending transition application、hook session，见 `crates/agentdash-application/src/session/launch/preparation.rs:37`。prepare 阶段创建 `TurnExecution::new` 并投递 initial capability/context frames，见 `crates/agentdash-application/src/session/launch/preparation.rs:147`、`crates/agentdash-application/src/session/launch/preparation.rs:226`、`crates/agentdash-application/src/session/launch/preparation.rs:263`。
- `SessionRuntime`/`SessionProfile`/`TurnExecution` 是当前 in-memory runtime fact source：`SessionProfile` 持完整 `CapabilityState`，`SessionRuntime` 持 hook runtime、turn state、profile、auto-resume count，`TurnExecution` 持 session frame、turn capability、runtime injection fragments、audit ids、cancel/processor/abort，见 `crates/agentdash-application/src/session/hub_support.rs:165`、`crates/agentdash-application/src/session/hub_support.rs:212`、`crates/agentdash-application/src/session/hub_support.rs:214`、`crates/agentdash-application/src/session/hub_support.rs:232`。
- `SessionRuntimeRegistry` 以 `HashMap<String, SessionRuntime>` keyed by session id 维护 runtime，见 `crates/agentdash-application/src/session/runtime_registry.rs:12`。

Gap:

- `SessionConstructionPlan` 是事实聚合器，形态上最接近未来 ActorFrame construction；但当前入口和输出都叫 session，且 owner scope 仍从 session meta/run link 推导。建议迁移为 `ActorFrameConstructionPlan` 或等价控制面，SessionConstruction 只负责最终 RuntimeSession connector launch projection。
- `LaunchPlan.lifecycle` 命名与业务 Lifecycle 冲突。它实际是 prompt lifecycle/rehydrate/bootstrap 状态，应改为 runtime prompt lifecycle 或 RuntimeSessionLaunchLifecycle，避免与 control-plane Lifecycle 混淆。
- `TaskService` launch source 与死掉的 task construction path 同时存在，容易让 Task 再次获得 runtime owner 语义。目标应让 TaskService 只创建/选择 Lifecycle subject/activity payload，再由 Lifecycle/Actor 创建执行。
- `SessionRuntime` 现在同时是 ActorFrame cache 和 RuntimeSession live connection。迁移时应拆：ActorFrame 持 capability/context/hook snapshot/pending actions/revisions；RuntimeSession 持 connector stream/turn/cancel/transport。

#### 8. Capability/runtime context

- `RuntimeContextTransition` 描述 workflow/runtime context switch，字段含 phase_node、run_id、lifecycle_key、before/after capability、capability keys、event payload，定义点 `crates/agentdash-application/src/session/capability_state.rs:32`。`to_pending_capability_state_transition` 要求 run_id/lifecycle_key 才能生成 pending transition，见 `crates/agentdash-application/src/session/capability_state.rs:134`。
- `CapabilityDimensionRegistry` 负责维度 validate/compile/replay/normalize，定义点 `crates/agentdash-application/src/session/capability_state.rs:209`；runtime transitions replay 在 `crates/agentdash-application/src/session/capability_state.rs:631`。
- `SessionCapabilityService` 提供 current/latest capability、requested runtime commands、pending/live transition、live VFS update，见 `crates/agentdash-application/src/session/capability_service.rs:21`、`crates/agentdash-application/src/session/capability_service.rs:34`、`crates/agentdash-application/src/session/capability_service.rs:38`、`crates/agentdash-application/src/session/capability_service.rs:42`、`crates/agentdash-application/src/session/capability_service.rs:64`、`crates/agentdash-application/src/session/capability_service.rs:78`、`crates/agentdash-application/src/session/capability_service.rs:93`。
- `PendingCapabilityStateTransition` 和 runtime command persistence 是 session scoped：`SessionStoreSet` 聚合 runtime commands，见 `crates/agentdash-application/src/session/persistence.rs:20`；`upsert_runtime_command_request(session_id, transition)` 见 `crates/agentdash-application/src/session/persistence.rs:164`。
- Capability resolver 输入有 `owner_ctx`、contributions、MCP candidates、`CapabilityContext`，其中 `granted_capability_keys` 可过滤显式授权，见 `crates/agentdash-application/src/capability/resolver.rs:73`、`crates/agentdash-application/src/capability/resolver.rs:84`、`crates/agentdash-application/src/capability/resolver.rs:216`。
- `resolve_session_workflow_context` 从 ProjectAgent/default lifecycle/entry activity/WorkflowDefinition 派生 tool contribution，见 `crates/agentdash-application/src/capability/session_workflow_context.rs:30` 和 `crates/agentdash-application/src/capability/session_workflow_context.rs:60`。

Gap:

- CapabilityState 当前事实源散在 `SessionConstructionPlan.projections`、`SessionProfile`、`TurnExecution`、pending runtime commands、hook runtime capabilities。目标应收束为 ActorFrame capability projection/revision，RuntimeSession 只拿某一 revision 的执行快照。
- Runtime command 当前按 session 持久化，而 transition 语义又带 run_id/lifecycle_key。它实际描述 ActorFrame 在某个 Lifecycle/Activity 下的 capability mutation，应该挂到 ActorFrame 或 ActorFrame revision，再投影给 RuntimeSession。
- `resolve_session_workflow_context` 把 ProjectAgent/default lifecycle 当 session workflow context 来源，属于 lifecycle/workflow/capability 的相似路径。目标应由 Lifecycle/Workflow 明确下发 ActorFrame capability contribution，不由 session owner planner 反向猜。

#### 9. Context/model context/frame

- `ContextBuildPhase` 包括 ProjectAgent、TaskStart、TaskContinue、StoryOwner、OwnerBootstrap、LifecycleNode、Companion、RepositoryRehydrate，见 `crates/agentdash-application/src/context/builder.rs:30`；`build_session_context_bundle` 是纯 context bundle 构建，见 `crates/agentdash-application/src/context/builder.rs:103`。
- `contribute_workflow_binding` 的 source 仍为 `legacy:contributor:workflow_bindings`，见 `crates/agentdash-application/src/context/workflow_bindings.rs:11` 和 `crates/agentdash-application/src/context/workflow_bindings.rs:21`。
- `render_workflow_injection` 注释说明该 helper 收拢了 task path / lifecycle node path / companion workflow path 的重复渲染，定义点 `crates/agentdash-application/src/context/rendering/workflow_injection.rs:37`。
- `ContextFrame` SPI 是可投递给 runtime 的 frame，定义点 `crates/agentdash-spi/src/hooks/mod.rs:234`；application helper `build_context_frame`/`enqueue_context_frame` 在 session hook runtime 上排队，见 `crates/agentdash-application/src/session/context_frame.rs:31` 和 `crates/agentdash-application/src/session/context_frame.rs:48`。
- `ContextProjector.build_model_context` 以 session events 构建 model context，见 `crates/agentdash-application/src/session/context_projector.rs:20` 和 `crates/agentdash-application/src/session/context_projector.rs:29`。

Gap:

- Context bundle builder 本身可以复用，但 `session_context_bundle`、`SessionContextQueryOwnerFacts`、session event projector 都强化了 session 作为上下文事实源。目标应由 ActorFrame 管理 capability/context/session runtime 事实源，RuntimeSession 的 model context 是 ActorFrame/RuntimeSession event projection。
- `workflow_bindings` 的 legacy source 是明显旧路径信号。应由 Workflow/Lifecycle/ActorFrame projection 统一生成 assignment/context fragments，而不是 task/workflow/companion 三条路径各自注入。
- `ContextFrame` 现在同时承载 capability delta、assignment context、pending action、compaction summary、hook injection 等。迁移时需要区分 ActorFrame durable context frame 与 RuntimeSession delivery frame，否则审计、重放、UI trace 仍会混在 session event stream。

#### 10. Hook runtime

- `HookSessionRuntime` 字段含 `session_id`、provider、snapshot、diagnostics、trace、pending_actions、turn_start_notices、token_stats、capabilities、revision，定义点 `crates/agentdash-application/src/session/hook_runtime.rs:18` 到 `crates/agentdash-application/src/session/hook_runtime.rs:28`。
- `runtime_snapshot` 返回 `HookSessionRuntimeSnapshot`，见 `crates/agentdash-application/src/session/hook_runtime.rs:156`；`evaluate` 自动注入 token stats、执行 provider hook、处理 pending advance 与 execution log flush，见 `crates/agentdash-application/src/session/hook_runtime.rs:204`；`replace_snapshot`/`update_capabilities` 更新 session hook runtime revision，见 `crates/agentdash-application/src/session/hook_runtime.rs:247` 和 `crates/agentdash-application/src/session/hook_runtime.rs:281`。
- SPI `HookSessionRuntimeAccess` 以 session 为接口核心，见 `crates/agentdash-spi/src/hooks/mod.rs:464`；`HookSessionRuntimeSnapshot` 含 session id、revision、snapshot、diagnostics、trace、pending actions，见 `crates/agentdash-spi/src/hooks/mod.rs:166`。
- Hub 侧 `emit_session_hook_trigger` 以 session id/turn id 调用 hook、trace、injections，见 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:57`；`collect_runtime_context_update_injections` 把 runtime context update 的 snapshot injections 写回 bundle/audit，见 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:154`；`ensure_hook_session_runtime` 按 session 懒重建 snapshot runtime，见 `crates/agentdash-application/src/session/hub/hook_dispatch.rs:181`。
- `HookRuntimeDelegate` 在 connector boundary 执行 hook，并把 runtime injections 写入 active turn 的 `runtime_injection_fragments`，见 `crates/agentdash-application/src/session/hook_delegate.rs:29`、`crates/agentdash-application/src/session/hook_delegate.rs:174`、`crates/agentdash-application/src/session/hook_delegate.rs:104`；trace 记录点在 `crates/agentdash-application/src/session/hook_delegate.rs:296`。
- `rules.rs` 会叠加 global rules、active workflow contract rules、owner default rules，见 `crates/agentdash-application/src/hooks/rules.rs:31` 和 `crates/agentdash-application/src/hooks/rules.rs:49`。

Gap:

- Hook runtime 是 ActorFrame 最像的现有结构：它有 snapshot、revision、trace、pending actions、capability keys、turn-start notices。但它被命名和索引为 session，导致 Lifecycle/Activity/Capability/Context facts 全被投影到 `SessionHookSnapshot`。
- `pending_advance` 从 hook resolution 进入 provider advance workflow step，随后 flush execution log 到 LifecycleRun。这条链路说明 hook 不只是 runtime callback，还在驱动 ActivityAttempt/Lifecycle 进展；目标里 hook decision 应属于 ActorFrame execution policy，advance 是 Lifecycle Activity event，不应隐藏在 session hook runtime 内。
- `SessionStart` 同时是 hook trigger、prompt lifecycle bootstrap、context injection 起点。迁移时需要把 ActorFrame activation 与 RuntimeSession start 分开，否则 phase 切换、rehydrate、auto-resume 会继续共享一个 session lifecycle 名词。

### Dependency/GAP Matrix by Module

| Module | Current responsibility | Current dependencies | Runtime facts currently attached to | Target home | Split / dedupe suggestion | Risk |
| --- | --- | --- | --- | --- | --- | --- |
| Domain `workflow/entity.rs` | lifecycle/workflow definitions, run, claims | `LifecycleRun.session_id`, `ActivityLifecycleRunState`, `WorkflowContract` | `LifecycleRun.session_id`, `activity_state`, `execution_log` | Lifecycle stores run/state/log; Actor/ActorFrame stores runtime binding | Keep `ActivityAttemptState`; remove runtime session as run anchor; rename graph/procedure concepts | Existing `list_by_session` callers break until ActorFrame association exists |
| Domain `workflow/run_state.rs` | attempt evidence and executor ref | `ExecutorRunRef::AgentSession` | Attempt executor evidence | ActivityAttempt evidence links to Actor/RuntimeSession ref | Add Actor/ActorFrame ref alongside RuntimeSession ref conceptually | Terminal mapping currently assumes session id is enough |
| Domain `task/entity.rs` | user-facing task projection | attempt status, artifacts, lifecycle step key | Task status/artifacts projection | Task as SubjectRef/payload/projection target | Keep projection-only writes from lifecycle/activity; remove runtime launch ownership semantics | TaskService launch path can reintroduce runtime owner |
| Workflow engine/scheduler | deterministic activity state advancement | `ActivityEvent`, `ActivityExecutionClaim`, `ExecutorRunRef` | Attempt state and claim repo | Lifecycle/ActivityAttempt | Keep engine pure; scheduler allocates Actor assignment before RuntimeSession | Parallel/multi-actor attempt mapping needs explicit id |
| Workflow orchestrator/association | session terminal/advance to activity event | `resolve_activity_session_association(session_id)`, `run.session_id` | Session id as activity anchor | RuntimeSession -> ActorFrame -> ActivityAttempt association | Replace `list_by_session` with actor/frame binding lookup; use lifecycle activity label as evidence label | Highest blast radius: terminal, advance, hook snapshot, active projection |
| Agent executor/step activation | spawn/continue agent execution and capability transition | `root_session_id`, hook runtime, capability service | root session / child session | Actor controls RuntimeSession; ActorFrame controls capability/context | Treat SpawnChild/ContinueRoot as RuntimeSession binding policies under ActorFrame | ContinueRoot live transition and pending transition must share revision model |
| Session construction | assemble launch facts | owner scope, context bundle, capability projection, requested runtime commands | `SessionConstructionPlan`, `SessionMeta`, session runtime command store | ActorFrame construction, then RuntimeSession launch projection | Rename/split construction into ActorFrame plan + RuntimeSession plan | Task input currently dead; owner default project may hide missing subject |
| Session runtime registry | live in-process runtime state | session id, hook runtime, profile, turn execution | `HashMap<session_id, SessionRuntime>` | RuntimeSession registry; ActorFrame cache separate | Move `SessionProfile`/hook snapshot/pending actions to ActorFrame; leave transport/turn/cancel in RuntimeSession | Need event/order migration for active turn resume |
| Capability service/state | capability projection, live/pending transitions | session id, run_id/lifecycle_key in transition | SessionProfile, TurnExecution, pending runtime commands, hook runtime capabilities | ActorFrame capability projection/revision | Persist transition by ActorFrame id/revision; RuntimeSession reads revision | Replaying session-scoped pending commands can mutate wrong actor frame |
| Context builder/projector/frame | build context bundle and deliver frames | session bundle, session events, hook turn notices | Session event stream and hook runtime queues | ActorFrame context projection; RuntimeSession delivery log | Separate durable context facts from delivery frames | Compaction/model context currently assumes session event stream |
| Hook provider/runtime/delegate | active workflow snapshot, hook rules, trace/effects/actions | session id, active workflow projection, run links, token stats | `SessionHookSnapshot`, `HookSessionRuntime` | ActorFrame policy snapshot/runtime facts; RuntimeSession callback channel | Convert snapshot to ActorFrame snapshot; hook delegate receives RuntimeSession + ActorFrame refs | Hook advance/logging currently mutates Lifecycle through provider |
| Capability resolver/session workflow context | derive visible capability contributions | owner_ctx, workflow/default lifecycle, granted keys | Session construction capability projection | Lifecycle/Workflow contributes ActorFrame capability | Move workflow contribution into Lifecycle/ActorFrame activation | Current ProjectAgent/default lifecycle path hides graph-vs-contract boundary |

### Concept Confusions and Similar Paths

1. Lifecycle vs workflow

- `ActivityLifecycleDefinition` is the graph, while `WorkflowDefinition` is the primary agent contract. Target says Workflow should be the graph config under Lifecycle, so current names invert the mental model.
- `ActiveWorkflowProjection` combines lifecycle run, activity node, node type, workflow key, and primary workflow. It is useful as a projection, but it should become an ActorFrame/Lifecycle projection, not the canonical source.
- `LifecycleLaunchPlan` in session launch is not business Lifecycle; it wraps `SessionPromptLifecycle`. This naming collision will keep causing control-plane drift.

2. Session vs actor

- `LifecycleRun.session_id`, `root_session_id`, `SessionRuntimeRegistry`, `SessionHookSnapshot.session_id`, `ContextProjector.build_model_context(session_id)` all treat session as both identity, runtime, and control anchor.
- Actor is missing as the stable high-level execution subject. ActorFrame is missing as the revisioned fact source for capability/context/hook snapshot. RuntimeSession is currently doing all three jobs.

3. Activity attempt vs state anchor

- `ActivityAttemptState` is correctly shaped as evidence, but `resolve_activity_session_association` promotes the current session to the association anchor. Attempt state should not be used to infer ownership by scanning run activity state plus `list_by_session`.
- `ExecutorRunRef::AgentSession` should remain evidence of a concrete runtime execution, but ActivityAttempt should link to Actor/ActorFrame assignment first.

4. Capability/context fact source

- Capability facts are duplicated across construction projection, runtime profile, turn execution, pending commands, hook capabilities, and SPI frames.
- Context facts are duplicated across context bundle, hook injections, turn runtime injection fragments, context frames, session events, and model context projection.
- Both should converge on ActorFrame as durable fact/revision source; RuntimeSession should receive delivery frames and execution snapshots.

5. Task as runtime owner

- Domain Task comments and binding kind removal already point away from runtime semantics. Application still exposes `LaunchSource::TaskService` and a dead task construction builder. This should be resolved by making TaskService create or reference Lifecycle subject/activity payload, not by launching a Task-owned session.

### Target Migration Shape

- `Lifecycle`: owns lifecycle run, activity graph state, execution log, ActivityAttempt evidence, run links to SubjectRef.
- `Workflow`: executable graph/config under Lifecycle. The current per-agent `WorkflowDefinition` contract likely needs a separate name or nested role so it no longer competes with graph Workflow.
- `Actor`: high-level wrapper for agent participation in a lifecycle/activity. It should model root/child/continue relationships now expressed by `root_session_id`, `ExecutorRunRef::AgentSession`, and session labels.
- `ActorFrame`: revisioned fact source for capability state, context bundle/projection, hook snapshot/runtime policy, active assignment, pending actions, runtime command transitions.
- `RuntimeSession`: connector/live execution instance: turn stream, cancel, process state, transport, event delivery, runtime trace. It references ActorFrame revision instead of owning capability/context facts.
- `ActivityAttemptState`: keep as activity execution evidence. Store attempt status, claim, executor evidence, and pointer to Actor/ActorFrame/RuntimeSession evidence.

### Related Specs

- `.trellis/workflow.md`: Trellis development and research workflow.
- `.trellis/spec/backend/index.md`: backend spec index.
- `.trellis/spec/backend/session/architecture.md`: session launch pipeline, construction plan, runtime command replay, ExecutionContext projection.
- `.trellis/spec/backend/workflow/architecture.md`: workflow/activity lifecycle runtime, event-driven advancement, session binding label.
- `.trellis/spec/backend/capability/architecture.md`: capability declaration/runtime transition/projection and CapabilityScope direction.
- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/prd.md`: current task objective and target predicate system.

### External References

- None. This scan used internal source/spec files only.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task/source in this shell. The output path was taken from the user-specified Trellis task: `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment`.
- No code or spec files were edited. Only this research file was written under the task `research/` directory.
- I did not perform full test execution because this is a source dependency scan, not an implementation slice.
- The design artifact expected by the PRD was not present as a standalone design file during this scan; conclusions are based on the PRD, backend specs, and source.
