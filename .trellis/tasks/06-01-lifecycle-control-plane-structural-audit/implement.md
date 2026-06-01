# 执行计划：彻底解决结构性问题

## Purpose

本文不是机械修改列表。它规定后续修复顺序：先修事实源与封装边界，再删旧路径和补测试。任何实现任务都应从这里拆分子任务。

## Phase 1: 固化运行闭环不变量

- [x] 设计 `ActivityRuntimeAssociationResolver`。
  - 输入 runtime terminal / session / turn provenance。
  - 输出 stable assignment / graph instance / activity attempt refs。
  - 不依赖 current frame id 等易变 revision。
- [x] 明确 `AgentAssignment` 与 `AgentFrame` revision 的关系。
  - assignment 绑定执行证据。
  - frame revision 绑定有效 runtime surface。
  - terminal resolution 必须能跨 frame revision 回到 assignment。
- [x] 把相关 invariant 写成测试。

### Gate

- [x] 单元/集成测试证明：同一个 RuntimeSession 绑定的 AgentFrame 发生 revision 后，terminal callback 仍能解析到原 `AgentAssignment` 与 `ActivityAttemptState`。
- [x] 失败场景测试证明：无法解析 assignment 时返回结构化 domain/application error，不静默跳过 Activity advancement。
- [x] `rg "assignment.frame_id == frame.id"` 不再是 terminal resolution 的唯一判定条件。
- [x] 代码审查证明 terminal resolver 不读取 read model，也不依赖前端/route-local DTO。

### 落地记录

2026-06-02 的 Phase 1 slice 已关闭 terminal association gate：

- `ActivityRuntimeAssociationResolver` 以 `RuntimeSession -> current AgentFrame -> LifecycleAgent -> AgentAssignment -> LifecycleRun` 解析 terminal/advance provenance；找不到 runtime frame 时表示非 lifecycle session，已找到 frame/agent 但缺 assignment 时返回 `ActivityRuntimeAssociationError::MissingAssignment`。
- `select_assignment_for_runtime_frame` 先接受 exact launch frame evidence，再按 current frame 的 `graph_instance_id + activity_key` scope 回到原 assignment；同一 scope 或无 scope 下多 active assignment 会返回 `AmbiguousAssignments`。
- `resolve_activity_session_association` 保持结构化 application error，orchestrator 只在外层 `String` boundary 格式化；terminal path 不再把 assignment 缺失静默当作 `Ok(None)`。
- terminal resolver 只读取 frame / agent / assignment / run repositories，不读取 read model、route DTO 或前端 shape。`assignment.frame_id == frame.id` 仍用于 launch-frame exact evidence，但不再是 revision 后 terminal resolution 的唯一条件。
- Activity state 由 Phase 2 的 graph-instance owner gate 接续验证：association 输出 `graph_instance_id + activity_key + attempt`，`ActivityLifecycleRunService` 用 graph instance id 推进 `ActivityLifecycleRunState`。

验证记录：

- `cargo test -p agentdash-application workflow::session_association --lib -- --format terse`
- `cargo test -p agentdash-application workflow::projection --lib -- --format terse`
- `cargo test -p agentdash-application workflow::orchestrator --lib -- --format terse`
- `cargo check -p agentdash-application`

## Phase 2: 拆分 LifecycleRun 与 WorkflowGraphInstance ownership

- [x] 让 `WorkflowGraphInstance` 成为 activity_state 的 owner。
- [x] Engine / scheduler / orchestrator 接收 graph instance execution context。
- [x] `LifecycleRun` 只聚合 graph instances、agents、subjects、events、artifacts、gates。
- [x] 验证同一 run 多 graph instance 不覆盖状态。

### Gate

- [x] 测试证明同一 `LifecycleRun` 下两个 `WorkflowGraphInstance` 使用相同 `activity_key` 时，attempt / claim / assignment 状态互不污染。
- [x] Engine / scheduler / orchestrator 的主推进接口接收 graph instance context 或 graph instance id。
- [x] `LifecycleRun.activity_state` 不再是新写入的主事实源；若仍保留，只能作为迁移/投影缓存并有明确 source。
- [x] `WorkflowGraphInstance` repository 支持读写 activity state，且关键查询覆盖 `run_id + graph_instance_id`。

### 落地记录

2026-06-02 的 Phase 2 slice 已关闭 graph-instance ownership gate：

- `WorkflowGraphInstance` 持有 typed `ActivityLifecycleRunState`，`replace_activity_state` 校验 `graph_instance_id` 与 instance id 一致。
- `ActivityLifecycleRunService::apply_event(graph_instance_id, event)` 与 `launch_ready_attempts(graph_instance_id, launcher)` 以 graph instance id 作为推进入口；workflow orchestrator terminal/advance 路径先解析 assignment association，再用 `association.graph_instance_id` 推进。
- `LifecycleRun` 不再持有 `activity_state`；`active_node_keys` 是由 graph instance state 派生的 run-level projection，并带有 `graph_instance_id:activity_key` 前缀。
- Postgres `lifecycle_workflow_instances.activity_state_json` 读写 typed state，`WorkflowGraphInstanceRepository::get_by_run_and_id(run_id, graph_instance_id)` 覆盖关键查询；migration `0086_drop_lifecycle_run_activity_state.sql` 删除 run 级 `activity_state`。
- 前端 `ContextOverviewTab` 改为消费 generated `LifecycleRunView.workflow_graph_instances[].activities[].attempts`，`WorkflowRun` 类型与 mapper 不再暴露 `activity_state` / `active_node_keys` 作为 UI 运行态入口。

验证记录：

- `cargo test -p agentdash-application workflow::activity_run --lib -- --format terse`
- `pnpm --filter app-web test -- ContextOverviewTab.projection.test.tsx`
- `pnpm --filter app-web run typecheck`
- `rg -n "ActivityLifecycleRunState|ActivityAttemptState|activity_state|active_node_keys|WorkflowRun\\[\\]|run\\.id === activeWorkflow|mapActivityLifecycleRunState" packages/app-web/src/types packages/app-web/src/services packages/app-web/src/features/workspace-panel packages/app-web/src/features/workspace-runtime` 无命中。

## Phase 3: 重塑 Dispatch Intent Taxonomy

- [x] 拆清 `AgentLaunchIntent`、`SubjectExecutionIntent`、`LifecycleRunStartIntent`。
- [x] `ExecutionDispatchResult` 改为 discriminated result，避免全 optional refs。
- [x] Subject execution 必须返回 assignment ref 或 pending assignment ref。
- [ ] ProjectAgent open、Task start、Companion dispatch、Routine fire、manual run、Story root/freeform 都通过统一 dispatch 分类进入。
  - ProjectAgent open、Task start/continue、Companion sub dispatch、Routine fire、manual run 已进入 typed dispatch；Story root/freeform 写侧 launch 尚未关闭。
- [x] 修复 `WorkflowGraphRef::ByKey`，将 graph resolution 移出 dispatch 的临时 helper。

### Gate

- [x] 类型层面不再依赖一个全 optional `ExecutionDispatchResult` 表达所有 intent；不同 result variant 有明确 required refs。
- [x] `WorkflowGraphRef::ByKey` 解析失败有测试，并返回错误而不是生成随机 graph/lifecycle id。
- [ ] API route 检查证明 ProjectAgent、Task、Companion、Routine、manual run、Story root/freeform 都进入 typed dispatch。
- [x] Subject execution 测试证明返回 assignment 或 pending assignment ref，且 SubjectRef 能追溯到 ActivityAttemptState。

### 落地记录

2026-06-02 的 Phase 3 slice 已关闭 dispatch taxonomy 的核心语义 gate，但整阶段仍因 Story root/freeform 写侧入口未迁入 dispatch 保持 partial：

- `ExecutionIntent` / `ExecutionDispatchResult` 已是 discriminated taxonomy；`AgentLaunchDispatchResult` 不再携带 required `assignment_ref`，ProjectAgent launch response 也不再把 agent surface launch 伪装成 ActivityAttempt assignment。
- `LifecycleDispatchService` 通过 `bind_entry_assignment` 显式区分 pure agent launch 与会绑定 ActivityAttempt 的 subject / interaction execution；`AgentLaunchIntent` 只创建 run / graph instance / agent / frame / runtime surface，`SubjectExecutionIntent` 与 `InteractionDispatchIntent` 才创建 entry assignment。
- dispatch 创建或复用 `WorkflowGraphInstance` 时会初始化 `ActivityLifecycleRunState` 并同步 run projection；`SubjectExecutionDispatchResult.assignment_ref` 对应的 assignment 现在有同一 `graph_instance_id + activity_key + attempt` 的 ActivityAttemptState。
- runtime terminal resolver 对没有 `graph_instance_id + activity_key` scope 的 AgentFrame 返回 `Ok(None)`，把纯 agent surface runtime 视为非 Activity runtime；有 activity scope 但缺 assignment 仍返回结构化 error。
- `WorkflowGraphResolver` 已作为 dispatch 前置边界解析 `ById` / `ByKey`，missing key 测试证明不会创建随机 run / graph / assignment。

验证记录：

- `cargo test -p agentdash-domain workflow::dispatch --lib -- --format terse`
- `cargo test -p agentdash-application workflow::dispatch_service --lib -- --format terse`
- `cargo test -p agentdash-application workflow::session_association --lib -- --format terse`
- `cargo test -p agentdash-application routine::dispatch --lib -- --format terse`
- `cargo check -p agentdash-domain -p agentdash-application -p agentdash-api -p agentdash-contracts`

## Phase 4: 收束 AgentFrame 作为 runtime surface owner

- [ ] 将 `StepActivation` 纳入 `AgentFrameBuilder` 内部阶段。
- [x] 拆分 `AgentFrameTransition` 与 `RuntimeDeliveryCommand`。
- [ ] Hook/capability command primary target 改为 agent/frame/assignment。
  - Hook runtime/provider entry 已引入 `HookControlTarget` 与 `RuntimeAdapterProvenance`，并要求 provider 显式处理 frame query；`StepActivation` live apply 与 canvas capability sync 已改为接收 `AgentFrameRuntimeTarget`。`SessionHookService` 已提供 target-aware hook runtime getter/ensure，并迁移 workflow/canvas capability caller。完整 gate 仍取决于 companion notification、hook SPI/session facade 与 ContinueRoot policy 入口迁到 target-first service。
- [ ] `session_id` 仅作为 runtime adapter provenance。
- [ ] `ContinueRoot` 改为 AgentReusePolicy + RuntimeSessionPolicy 的组合。
- [x] 明确多 RuntimeSession selection policy。

### Gate

- [x] `AgentFrameBuilder` 测试覆盖 procedure、context、capability、VFS/MCP、runtime refs 的同源 frame revision 输出。
- [x] Runtime command 表或接口只表达 delivery；frame transition 有独立事实源或明确 repository。
- [ ] Hook/capability control command 的 primary target 是 agent/frame/assignment，只有 runtime adapter 接收 raw session id。
- [ ] Static gate：`rg -n "SessionHookSnapshotQuery|SessionHookRefreshQuery|HookEvaluationQuery \\{|ensure_hook_runtime\\(|get_hook_runtime\\(|resolve_runtime_session_frame_id\\(" crates/agentdash-application/src` 只允许命中 runtime adapter、tests 或显式 provenance/trace sink。
- [ ] Hook gate：测试证明 hook snapshot load / refresh / evaluate 可从 `frame_id + assignment_id` 执行，不需要 raw runtime session id 作为 owner。
- [ ] Capability gate：PhaseNode 与 canvas live update 测试直接传入 `AgentFrameRuntimeTarget`，workflow/canvas control logic 不调用 `resolve_runtime_session_frame_id`。
  - 2026-06-02：PhaseNode live apply 已改为 `apply_to_frame_runtime_target`；canvas capability sync 已拆出 runtime-delivery adapter 与 target-first apply helper，不再直接调用 `resolve_runtime_session_frame_id`；workflow/canvas 获取 hook runtime 时会校验 hook runtime target 与 `AgentFrameRuntimeTarget` 一致。gate 仍未关闭，因为 companion notification、hook SPI/session facade 与 ContinueRoot policy 入口还没有统一迁到 frame/assignment target。
- [ ] Companion gate：companion parent request / result notification 以 parent frame/assignment 为 target，parent runtime session 只进入 trace/delivery payload。
  - [x] Parent result return 已由 `CompanionGateControlService` resolve gate 后交给 delivery adapter 投递。
  - [ ] Parent request initial notification 与 hook evaluation 仍需迁到 parent frame/assignment target。
- [ ] Delivery gate：保留 mismatched frame/session rejection 与 pending payload 测试，并补充 delivery runtime session 属于另一 frame 时失败。
- [x] 多 RuntimeSession ref selection 有显式 policy 测试，禁止默认 `first()` 选择。

### 落地记录

2026-06-02 的 AgentFrame surface slice 已关闭同源 frame revision 输出 gate，但 Phase 4 整体仍因 Hook/capability target、`session_id` provenance 与 ContinueRoot policy split 保持 partial：

- `AgentFrameBuilder` 新增 `AgentFrameSurfaceInput`，由 frame builder 模块统一吸收 capability state、VFS、MCP servers、execution profile 与 context bundle summary；`SessionAssemblyBuilder::project_assembly_to_frame` 不再逐列拼写 AgentFrame surface。
- `build_lifecycle_activation_surface` 成为 lifecycle `StepActivation` → frame surface 的封装边界，负责把 base VFS、activation lifecycle VFS、mount directives、MCP servers 与 capability state 归一化成同一份 frame-owned surface。
- `SessionAssemblyBuilder::apply_lifecycle_activation` 只消费该 frame-owned surface，避免在 session assembly 层复制 capability/VFS/MCP 归一化规则。
- `workflow::frame_builder` 单测新增同源 revision gate，证明同一次 activation surface 能在同一 AgentFrame revision 中同时落下 procedure、context、capability、VFS/MCP、runtime refs 与 graph activity scope。
- 该 slice 不关闭「将 `StepActivation` 纳入 `AgentFrameBuilder` 内部阶段」主项，因为 `StepActivation` live apply 虽已 target-first，仍和 companion skill projection、ContinueRoot 一样在 builder 外部消费 activation。

2026-06-02 的 StepActivation live apply slice 已关闭 workflow 内部“applier 自行 session -> frame lookup”的局部缺口，但 Phase 4 capability gate 仍保持 partial：

- `SessionCapabilityService::resolve_runtime_session_target` 成为 runtime adapter 到 `AgentFrameRuntimeTarget` 的解析入口；workflow 调用点不再直接拿 `target_frame_id` 字符串。
- `StepActivation::apply_to_frame_runtime_target` 改为接收 `AgentFrameRuntimeTarget` 与调用方提供的 base capability surface，只负责把 activation 归一化成目标 surface 并执行 runtime context transition。
- `AgentActivityExecutor` 的 live 与 pending transition 均传递显式 `target_frame_id + delivery_runtime_session_id`，不再让 StepActivation applier 自行反查 frame。
- 该 slice 不关闭 capability gate，因为 canvas live update 当时仍可从 raw session id 解析 frame，ContinueRoot 仍以 `root_runtime_session_id` 同时表达 reuse policy 与 delivery target，companion parent request initial notification / hook control 也尚未迁入 frame/assignment target。

2026-06-02 的 canvas capability sync slice 已关闭 canvas 直接 frame lookup 缺口：

- `expose_canvas_to_session` 只调用 `sync_canvas_mount_capability_state_for_runtime_delivery` adapter；该 adapter 在确认存在 base capability state 与 hook runtime 后，把 delivery runtime session 解析为 `AgentFrameRuntimeTarget`。
- `sync_canvas_mount_capability_state` 改为接收 `AgentFrameRuntimeTarget`、base capability state 与 hook runtime，再调用 `apply_live_vfs_capability_state`；canvas apply helper 不再知道 `resolve_runtime_session_frame_id`。
- Static check 中 `resolve_runtime_session_frame_id(` 在 application src 只剩 `SessionCapabilityService` 与 hub adapter 定义/调用。
- 该 slice 仍不关闭完整 Phase 4 gate，因为 hook SPI/session facade、ContinueRoot policy 与 companion parent request initial notification / hook control 仍未迁成 frame/assignment target。

2026-06-02 的 hook runtime target-aware caller slice 已关闭 workflow/canvas capability caller 直接使用 session-first hook getter 的缺口：

- `SessionHookService::ensure_hook_runtime_for_target` / `get_hook_runtime_for_target` 以 `AgentFrameRuntimeTarget` 为输入，内部仍使用 delivery runtime session adapter，但会校验 hook runtime 的 `session_id` 与 `control_target.frame_id` 均匹配 target。
- `AgentActivityExecutor` 先解析 `AgentFrameRuntimeTarget`，再通过 target-aware ensure 获取 hook runtime；ContinueRoot live apply 不再裸调 `ensure_hook_runtime(root_runtime_session_id)`。
- canvas capability sync 先解析 target，再通过 `get_hook_runtime_for_target` 获取 hook runtime；不再裸调 `get_hook_runtime(session_id)`。
- 该 slice 仍不关闭 Hook gate，因为 snapshot load / refresh / evaluate 的 SPI/session facade、hub lazy rebuild 与 companion parent hook evaluation 仍存在 session-shaped入口。

2026-06-02 的 ContinueRoot target resolution slice 已关闭 start/apply 内部反复分发 root runtime session 的缺口，但不关闭 `AgentReusePolicy + RuntimeSessionPolicy` 主项：

- `AgentActivitySessionPort::resolve_continue_root_runtime_target` 成为 ContinueRoot 从 root runtime session adapter 到 `AgentFrameRuntimeTarget` 的封装边界。
- `start_continue_root` 先解析 `root_target`，后续 executor config、assignment、runtime context apply 与返回的 delivery ref 都使用 target 的 `delivery_runtime_session_id`。
- `apply_continue_root_activity` 改为接收 `AgentFrameRuntimeTarget`，live/pending 两支不再自行解析 root runtime session。
- 该 slice 仍不关闭主项，因为 `AgentActivityLaunchContext` 仍以 `root_runtime_session_id` 作为 policy 输入，尚未由 lifecycle agent/frame reuse policy 与 runtime session selection policy 共同表达。

验证记录：

- `cargo test -p agentdash-application workflow::frame_builder --lib -- --format terse`
- `cargo test -p agentdash-application workflow::step_activation --lib -- --format terse`
- `cargo test -p agentdash-application workflow::agent_executor --lib -- --format terse`
- `cargo test -p agentdash-application canvas::tools::tests::present_canvas_updates_meta_capability_skill_and_events --lib -- --format terse`
- `cargo check -p agentdash-application`
- `cargo fmt --all --check`
- `git diff --check`

## Phase 5: 收束业务入口与 interaction/gate

- [ ] Story root/freeform launch 进入 dispatch，创建 Story subject association。
- [ ] Task execution command 使用 SubjectExecution contract，Task 只保留 business spec；执行偏好迁到 dispatch policy 或 SubjectExecutionPreference。
- [x] Task cancel 改为 CancelSubjectExecutionCommand，runtime cancel 只是 delivery。
- [x] Task view status vocabulary 区分 Cancelled 与 Failed；取消投影不再伪装成失败业务状态。
- [ ] CompanionChannel / LifecycleGate / RuntimeNotification 分层。
  - Human request/respond 已迁到 gate-first：请求创建 `LifecycleGate(frame_id)`，API 通过 `gate_id` resolve，runtime notification 只是 delivery adapter。
  - Parent result return 已迁到 gate-first：child completion resolve child-owned `LifecycleGate`，parent/child notification 只是 delivery adapter。
  - 主项仍未关闭，因为 parent request pending action 与 parent hook/control 仍以 runtime session / hook runtime 为 owner。
- [x] Routine Reuse 通过 LifecycleAgentReuseResolver 查询，不借 parent_run_id 兜底。
- [ ] Permission 明确 source runtime session 只是 provenance，effect owner 是 frame。

### Gate

- [x] Task start/continue/cancel 的 active execution path 测试证明 command target 是 `SubjectRef` / assignment / frame，而不是 raw RuntimeSession。
- [x] Task wait/gate scope audit 证明当前 Task execution 不创建、不等待 `LifecycleGate`；Task cancel gate 不应硬塞进 `SubjectExecutionControlService`。未来若引入 Subject wait gate，必须先建立 subject gate owner/index，再要求 cancel 同事务关闭 gate truth。
- [ ] Companion wait/resume 测试证明 durable `LifecycleGate` 是 truth，runtime notification 只是 delivery。
  - [x] Human gate：`companion_request(target=human)` 创建 frame-owned `LifecycleGate`，`POST /companion-gates/{gate_id}/respond` 先 resolve gate，再由 delivery adapter 注入 runtime notification。
  - [x] Parent result return gate：`companion_respond` 从 child runtime session 解析 child frame，只 resolve child-owned open `LifecycleGate.correlation_id`，parent/child runtime event 由 `CompanionGateNotificationDelivery` 产生。
  - [ ] Parent request gate：parent pending action / hook evaluation 仍需从 hook runtime truth 迁入 gate/channel truth。
- [x] Routine reuse 测试证明按 routine/entity/subject association 复用 agent，而不是无 parent_run_id 时新建 run。
- [ ] Permission query 测试证明 frame/run/subject 是主查询入口，session 只作为审计 provenance filter。

### 落地记录

2026-06-02 的 Phase 5 Routine reuse slice 已关闭 Routine gate，但 Phase 5 整体仍因 Task cancel、Companion 分层和 Permission provenance 保持 partial：

- 新增 `LifecycleAgentReuseResolver` 作为 Routine Reuse / PerEntity 的唯一查询封装；它按 routine execution 历史、entity key、dispatch refs、LifecycleRun、LifecycleAgent、AgentFrame、AgentAssignment 与 `LifecycleSubjectAssociation` 校验复用 anchor。
- `DispatchStrategy::Reuse` 在没有可复用 active lifecycle agent anchor 时返回 `ApplicationError::Conflict`，不再让 `RunPolicy::ReuseExisting` 缺 `parent_run_id` 时静默创建新 run。
- `DispatchStrategy::PerEntity` 必须从 payload 路径解析非空 `entity_key`；已有 entity target 时复用同一 `run_id + agent_id`，首次 entity 触发则显式走 `CreateLinkedRun + Create` 创建新的 per-entity anchor。
- `SubjectExecutionIntent` 现在从 resolver target 显式填入 `parent_run_id + parent_agent_id`；`LifecycleDispatchService` 在 `Reuse` / `Resume` 收到 `parent_agent_id` 时校验 agent 属于目标 run/project 且 active，同一 run 多 active agent 时只复用指定 agent。
- `RunPolicy::ReuseExisting` / `AppendGraph` 缺 `parent_run_id` 会直接 bad request，底层 dispatch policy 不再用“没有 anchor 就创建新 run”兜底。

验证记录：

- `cargo test -p agentdash-application routine::reuse_resolver --lib -- --format terse`
- `cargo test -p agentdash-application routine::dispatch --lib -- --format terse`
- `cargo test -p agentdash-application workflow::dispatch_service --lib -- --format terse`
- `cargo fmt --all --check`
- `cargo check -p agentdash-application`

2026-06-02 的 Phase 5 Task cancel slice 已关闭 active-assignment cancel lifecycle command gate；后续 scope audit 证明当前 Task execution 没有真实 `LifecycleGate` wait path，因此 Task gate cancellation 不作为当前实现 gate。Phase 5 整体仍因 Task view status vocabulary、Companion 分层和 Permission provenance 保持 partial：

- 新增 `SubjectExecutionControlService` 作为 subject execution control boundary；`CancelSubjectExecutionCommand` 以 `SubjectRef` 为输入，解析 active subject association、LifecycleAgent、AgentAssignment、AgentFrame，并校验 frame 与 assignment 的 `graph_instance_id + activity_key` 一致。
- Workflow engine 新增 durable `ActivityCancelled` event；cancel 只接受 cancellable attempt status，写入 `ActivityAttemptStatus::Cancelled`、完成时间与 summary，并让 cancelled run 在 graph status 推导中压过 pending successor。
- Task cancel facade 只负责把 Task 转成 `SubjectRef("task", task_id)` 并委派给 `SubjectExecutionControlService`；runtime cancel 被降为 `RuntimeCancelDeliveryCommand`，由 host dispatcher 投递。
- cancel command 完成后会释放 active claim、release active assignment，并把 Task view 从 lifecycle attempt status 投影；当前 Task domain 词表仍将 `ActivityAttemptStatus::Cancelled` 映射为 `TaskStatus::Failed`，这暴露出 Task 业务状态与 execution lifecycle 状态还需要继续拆分。
- `TerminalCancelCoordinator` 复用 `prepare_runtime_cancel_delivery` 解析 delivery target；业务终态触发的 runtime stop 不再手写解析 `runtime_session_refs_json`，也不写入新的 lifecycle cancel truth。
- API cancel response 返回 Task projection 与 run/graph/agent/frame/assignment/subject execution/runtime delivery refs；frontend 暂时只解包 `task` 保持既有 store 形状，generated contract 进入 Phase 6。

验证记录：

- `cargo test -p agentdash-application workflow::engine --lib -- --format terse`
- `cargo test -p agentdash-application workflow::dispatch_service --lib -- --format terse`
- `cargo test -p agentdash-application workflow::subject_execution_control --lib -- --format terse`
- `cargo test -p agentdash-application task::service --lib -- --format terse`（0 tests；仅证明过滤器无覆盖，不能作为 gate 证据）
- `cargo check -p agentdash-api`
- `pnpm --filter app-web run typecheck`
- `cargo fmt --all --check`
- `git diff --check`

2026-06-02 的 Phase 5 Task projection vocabulary slice 已关闭 P1-19A；Phase 5 整体仍因 Task wait/gate cancellation、Companion 分层和 Permission provenance 保持 partial：

- `TaskStatus` 新增 `Cancelled`，`TaskExecutionProjection` 成为 workflow attempt outcome 到 Task view status 的封装；`Task` entity 不再直接消费 workflow attempt enum。
- `ActivityAttemptStatus::Cancelled` 投影到 `TaskStatus::Cancelled`，不再合并为 `Failed`；terminal cancel 判断同步把 Task `Cancelled` 视为终态。
- `agentdash-contracts` 与 generated `core-contracts.ts` 已包含 Task `cancelled` union；前端 mapper、Task badge、Story task review label 与 Task execution panel action guard 同步识别 cancelled。
- MCP Task status 更新说明包含 `cancelled`，避免工具侧 schema 落后于 domain 词表。

验证记录：

- `cargo test -p agentdash-domain story::entity --lib -- --format terse`
- `cargo test -p agentdash-application task::view_projector --lib -- --format terse`
- `cargo check -p agentdash-api`
- `pnpm --filter app-web run typecheck`
- `pnpm run contracts:check`

2026-06-02 的 Phase 5 Companion human gate slice 已关闭 human request/respond 的 session-first 缺口；Phase 5 整体仍因 parent gate/channel、Story root/freeform、Task execution preference 和 Permission provenance 保持 partial：

- 新增 `CompanionGateControlService` 作为 gate-first respond boundary；它依赖 `LifecycleGateRepository`、`AgentFrameRepository`、`AgentLineageRepository` 与 `CompanionGateNotificationDelivery`，先 resolve durable gate truth，再把 runtime notification 作为 delivery 副作用。
- `companion_request(target=human)` 的 wait 与 non-wait 请求都会创建 frame-owned `LifecycleGate`，通知 payload 暴露 `gate_id`；non-wait 不阻塞 agent，但用户后续回应仍有 durable interaction owner。
- `companion_respond` 的 child-to-parent completion 迁入 `CompanionGateControlService::complete_child_result_to_parent`：service 以 child runtime session 解析 child frame，按 child agent open gate 的 `correlation_id` resolve durable gate，再把 parent/child session event 作为 delivery 投递。
- API 删除 `/sessions/{id}/companion-requests/{request_id}/respond`，新增 `/companion-gates/{gate_id}/respond`，鉴权先从 gate owner 解析到 LifecycleAgent/Project，再写 gate。
- `agentdash-contracts` 新增 `companion-contracts.ts`；前端 `SessionCompanionRequestCard` 不再把 `sessionId` 传给 respond service，而是提交 `gateId + payload`。
- `SessionControlService` 不再依赖 `LifecycleGateRepository`，session control 重新收束为 runtime session / tool approval 控制面。
- 该 slice 不关闭完整 P1-20，因为 parent request 仍通过 parent hook runtime pending action 与 parent session notification 表达部分 resume/control truth。

验证记录：

- `cargo test -p agentdash-application companion::gate_control --lib -- --format terse`
- `cargo test -p agentdash-application companion::tools --lib -- --format terse`
- `cargo test -p agentdash-application workflow::dispatch_service --lib -- --format terse`
- `cargo check -p agentdash-api`

## Phase 6: 建立稳定 Read Models

- [ ] 新增 `ProjectActiveAgentsView` generated contract / API / service / tests。
- [ ] 所有 `LifecycleRunView` 由唯一 builder 组装，story-specific route 不复制字段。
- [ ] Task execution API response 进入 generated contracts。
- [ ] `ExecutorRunRef` 中 raw `session_id` 转为 `RuntimeSessionRefDto` / runtime trace ref。
- [ ] `/session/:id` 页面收束为纯 RuntimeSessionTraceView，下钻回链到 agent/frame/subject。

### Gate

- [ ] `ProjectActiveAgentsView` 有 Rust contract、generated TS、API route、frontend service、store selector 和 test。
- [ ] 所有 route 返回 `LifecycleRunView` 时都调用同一个 builder；story-specific route 不手写核心字段。
- [ ] `pnpm run contracts:check` 覆盖 Task execution result 和 ProjectActiveAgentsView。
- [ ] Frontend tests 证明 project active list 按 project scoped view 渲染，不从 global lifecycle store 自行拼装。
- [ ] RuntimeSession trace page 测试证明它只消费 `RuntimeSessionTraceView`，控制面信息通过 refs 回链。

## Phase 7: 清理命名与入口旧语义

- [ ] `WorkflowContract` 重命名为 `AgentProcedureContract` 或最终确定的 procedure contract 名。
- [ ] 删除 shared-library legacy `entry_step_key / steps / edges` 自动兼容入口。
- [ ] 清理 route-local lifecycle / task / story session shape。
- [ ] 删除不再需要的 owner_type / session-first UI types。

### Gate

- [ ] `rg "WorkflowContract|entry_step_key|legacy_step_to_activity|TaskSessionPayload|SessionBindingResponse|runsBySessionId"` 只允许出现在迁移说明或测试快照中，核心代码无命中。
- [ ] Shared Library import/update 测试拒绝旧 step payload，或只通过显式 migration command 接受。
- [ ] UI 文案中 Workflow 仅指 graph config，AgentProcedure 仅指单 Agent Activity contract。
- [ ] route-local lifecycle / subject / agent / frame DTO 清零，跨层 DTO 全部进入 `agentdash-contracts`。

## Phase 8: 架构级验证

- [ ] schema assertion 检查目标列、旧列删除、索引和约束。
- [ ] `pnpm run contracts:check`。
- [ ] backend unit tests：terminal resolver、graph instance state、dispatch taxonomy、AgentFrame transitions。
- [ ] frontend tests：ProjectActiveAgentsView、SubjectExecution panel、RuntimeSessionTraceView。
- [ ] critical E2E：ProjectAgent、Story root、Task SubjectExecution、Companion gate、Routine reuse。
- [ ] 最终 `pnpm run check`。

### Gate

- [ ] clean database migration 和 existing developer database migration 都通过 schema invariant assertion。
- [ ] `pnpm run contracts:check`、backend targeted tests、frontend targeted tests、critical E2E 全部通过。
- [ ] 每个 P0/P1/P2 checklist 项都有对应代码证据或测试证据证明关闭。
- [ ] 最终审计文档记录每个 gate 的命令、结果和仍需 follow-up；没有“未验证但看起来对”的完成声明。

## Implementation Rules

- 不以 grep 旧字段消失作为完成标准。
- 不允许新增 route-local lifecycle/subject/agent/frame DTO。
- 不允许 command path 读取 read-model view 后再写事实源。
- 不允许业务模块直接构造 RuntimeSession launch payload。
- 不允许前端从 global lifecycle store 拼装 project runtime truth。
- 每个新增 service 必须说明自己拥有的事实源、不变量、事务边界或外部依赖隔离价值。
