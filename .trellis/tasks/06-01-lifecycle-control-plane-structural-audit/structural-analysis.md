# Lifecycle 控制面结构性分析

## Purpose

本文按 `raw-exposed-issues-checklist.md` 的顺序逐项分析。每个问题都先保留原始现象，再判断它背后是否暴露模型过度耦合，最后提出应该落地的封装方向。

判断标准不是“能不能补一个 if”，而是：这个缺口是否说明 facts ownership、事务边界、查询边界、projection 边界或命名边界仍然没有内聚。

## P0-01 Terminal event 反查 assignment / attempt 被 frame revision 切断

### 原始问题

Runtime terminal 目标上已经走 `RuntimeSession -> AgentFrame -> LifecycleAgent -> AgentAssignment`，但当前 resolver 只匹配 `assignment.frame_id == frame.id`。session launch 后如果写入新的 `AgentFrame` revision，`find_by_runtime_session` 返回最新 frame，terminal callback 可能找不到创建 assignment 时使用的旧 frame。

### 结构性分析

这是高风险运行闭环问题，不是一个简单查询条件遗漏。它说明当前模型把两个不同问题耦在了一起：

- `AgentAssignment` 需要回答“哪个 Agent 承接了哪个 ActivityAttemptState”。
- `AgentFrame` revision 需要回答“该 Agent 在某个时间点拥有哪个有效运行表面”。

当 terminal resolver 用最新 frame id 去找 assignment 时，实际上把“当前有效 frame”误当成“attempt 执行证据 frame”。这会让 frame revision 变成 brittle identity，而不是可演化状态。

系统性问题：**AgentFrame revision 与 AgentAssignment 的证据边界没有被显式封装**。

### 解决方案

引入专门的 `ActivityRuntimeAssociationResolver` 或等价封装，输入 `runtime_session_id / turn_id / terminal_event`，输出稳定的 `{run_id, graph_instance_id, activity_key, attempt, agent_id, assignment_id, frame_id_at_launch, current_frame_id}`。

规则：

- assignment 绑定的是 `agent_id + graph_instance_id + activity_key + attempt`，不是 current frame identity。
- frame id at launch 是 evidence；current frame 是状态投影。
- runtime session ref 需要携带或能解析到 launch frame / assignment provenance。
- terminal resolution 可以先按 exact launch frame 找，失败后按 runtime session ref -> agent -> active assignment / graph instance fallback，但 fallback 必须显式审计。

这比在现有 resolver 里加 OR 条件更重要，因为它把 terminal association 从 hook/session 边界中抽出来，成为 lifecycle execution 的独立查询边界。

### 落地记录

2026-06-02 已完成 terminal association resolver 收口：

- `ActivityRuntimeAssociationResolver` 返回稳定的 run / assignment / attempt evidence；assignment 的 `frame_id` 作为 launch evidence 保留，current frame revision 只用于定位当前 agent 与 graph/activity scope。
- `ActivityRuntimeAssociationError` 区分 repository failure、missing lifecycle agent、missing assignment、ambiguous assignment、invalid attempt 与 missing run；缺 assignment 不再被 terminal path 静默当作非 lifecycle session。
- `select_assignment_for_runtime_frame` 的测试覆盖 exact launch frame、frame revision 后按 graph/activity scope 找回旧 assignment、无 scope 歧义和同 scope 歧义。
- orchestrator terminal/advance path 通过 `resolve_activity_session_association` 获取 `graph_instance_id + activity_key + attempt`，再交给 graph-instance owned activity service 推进。

验证记录：

- `cargo test -p agentdash-application workflow::session_association --lib -- --format terse`
- `cargo test -p agentdash-application workflow::projection --lib -- --format terse`
- `cargo test -p agentdash-application workflow::orchestrator --lib -- --format terse`
- `cargo check -p agentdash-application`

## P0-02 `LifecycleRun.activity_state` 与 `WorkflowGraphInstance` 双事实源

### 原始问题

`WorkflowGraphInstance` 已存在并支持同一 run 多 graph instance，但 engine / scheduler / orchestrator 仍主要读写 `LifecycleRun.activity_state`。同一 `LifecycleRun` 多 graph instance 时可能覆盖状态或无法调度非 root graph。

### 结构性分析

这是 lifecycle tracking 与 workflow graph instance 的 ownership 不清。目标模型里：

- `LifecycleRun` 是 tracked life process。
- `WorkflowGraphInstance` 是某个 Workflow graph 在 run 下的执行实例。
- Activity state 应属于 graph instance，而 run 只聚合状态、events、artifacts、agents 和 subject associations。

现在 run 仍直接持有主 activity state，说明 graph instance 只是附属记录，不是执行状态 owner。这样会让后续 linked workflow、companion workflow、same-run task subgraph 都重新被迫塞回 root run state。

系统性问题：**LifecycleRun 与 WorkflowGraphInstance 的 aggregate boundary 没有真正分开**。

### 解决方案

把 activity execution state 的写入 owner 迁到 `WorkflowGraphInstanceState`。

建议封装：

- `WorkflowGraphInstanceRuntime`
  - owns `activity_state`
  - owns graph-level scheduler cursor / active activities
  - exposes apply `ActivityEvent`
- `LifecycleRun`
  - aggregates graph instances
  - owns lifecycle-level artifacts / events / subject associations / agents
  - does not directly mutate graph activity attempts

迁移顺序上，不应先到处替换字段名，而应先把 engine 的函数签名从 `LifecycleRun` 改成 `GraphInstanceExecutionContext`，让 run 成为 context 而不是状态容器。

### 落地记录

2026-06-02 已完成 `WorkflowGraphInstance` activity state ownership 收口：

- `WorkflowGraphInstance` 成为 typed `ActivityLifecycleRunState` 的持有者，写入时校验 state 内的 `graph_instance_id` 与 instance id 一致。
- `ActivityLifecycleRunService` 的推进入口以 `graph_instance_id` 定位 graph instance；orchestrator terminal/advance 路径通过 `ActivityRuntimeAssociationResolver` 的 association 结果推进对应 graph instance。
- `LifecycleRun` 只保存 lifecycle-level 状态与从 graph instances 派生的 control projection；`active_node_keys` 带 `graph_instance_id:activity_key` 前缀，用于表达跨 graph instance 的活跃节点集合。
- persistence 层将 state 存入 `lifecycle_workflow_instances.activity_state_json`，并提供 `run_id + graph_instance_id` 查询；run 表的 `activity_state` 已由 migration 删除。
- frontend runtime overview 不再把旧 `WorkflowRun.activity_state` 当作运行态事实源，改为从 generated `LifecycleRunView.workflow_graph_instances` 展示 attempt/progress。

验证记录：

- `cargo test -p agentdash-application workflow::activity_run --lib -- --format terse`
- `pnpm --filter app-web test -- ContextOverviewTab.projection.test.tsx`
- `pnpm --filter app-web run typecheck`

## P0-03 `LifecycleDispatchService` 不创建 `AgentAssignment`

### 原始问题

统一 dispatch 已能创建 run、graph、agent、subject、frame、gate、lineage，但 result 中 `assignment_ref` 固定为空；真实 `AgentAssignment` 主要在 activity executor path 创建。

### 结构性分析

这个问题不能简单理解为“少填一个字段”。它暴露的是 dispatch 入口的职责不清：

- dispatch 如果只是“创建或选择 agent/frame/runtime”，那它不该承诺 assignment。
- dispatch 如果表达“SubjectRef execution”，它必须把 SubjectRef 连接到 Activity/Attempt 或至少产生 pending assignment plan。

当前 result shape 试图同时覆盖 ProjectAgent open、Task execution、Companion spawn、Routine trigger、manual run，但没有区分“agent launch”和“activity execution”。因此 `assignment_ref` 变成长期 nullable，中间层调用方也无法知道什么时候应该存在。

系统性问题：**ExecutionIntent 的 intent taxonomy 不够精确，导致 dispatch result 把不同生命周期阶段揉成一个宽 DTO**。

### 解决方案

拆清 dispatch 语义，而不是机械地在所有 dispatch 里创建 assignment：

- `AgentLaunchIntent`
  - 创建 / 复用 `LifecycleAgent`
  - 创建 `AgentFrame`
  - 可附 RuntimeSession
  - 不保证 ActivityAttempt assignment
- `SubjectExecutionIntent`
  - 必须指定或解析到 graph activity / procedure
  - 必须返回 `AgentAssignmentRef` 或 `PendingAssignmentRef`
- `LifecycleRunStartIntent`
  - 创建 run + graph instance
  - 后续由 scheduler 产生 assignments

`ExecutionDispatchResult` 应变成 discriminated result，而不是一组全 optional refs。

### 落地记录

2026-06-02 已完成 dispatch taxonomy 的核心拆分：

- `ExecutionIntent` / `ExecutionDispatchResult` 已是 discriminated enum；result 不再用一个全 optional DTO 覆盖所有状态。
- `AgentLaunchDispatchResult` 移除 required `assignment_ref`；ProjectAgent launch 返回 `assignment_ref: None`，因为该入口只表达 agent/frame/runtime surface launch，不承诺 ActivityAttempt execution evidence。
- `LifecycleDispatchService` 内部以 `bind_entry_assignment` 明确分流：`AgentLaunchIntent` 不创建 entry assignment；`SubjectExecutionIntent` 与 `InteractionDispatchIntent` 创建 assignment，并要求 graph instance 已拥有 initialized `ActivityLifecycleRunState`。
- `SubjectExecutionIntent` 的测试覆盖 SubjectRef -> association -> assignment -> ActivityAttemptState：返回的 `assignment_ref` 与 graph instance state 中的 entry attempt 使用同一 `graph_instance_id + activity_key + attempt`。
- runtime terminal resolver 对无 activity scope 的 AgentFrame 返回非 activity runtime，不把 pure agent surface frame 误判为 missing assignment；有 activity scope 但缺 assignment 仍是结构化错误。

该记录不关闭 P0-06：Story root/freeform 写侧 launch 尚未进入 dispatch。

## P0-04 `WorkflowGraphRef::ByKey` 未解析

### 原始问题

ProjectAgent route 传入 `WorkflowGraphRef::ByKey`，dispatch 对 ByKey 直接返回 None，随后可能使用随机 UUID 创建 run lifecycle_id。

### 结构性分析

这是 catalog resolution 与 runtime instantiation 被耦在 dispatch 内部后的典型漏洞。Workflow key 是用户/配置语义，run lifecycle_id / graph_instance_id 是 runtime identity。两者不能在找不到解析结果时静默生成随机 id，否则系统会产生“看似启动成功、实际没有绑定目标 graph”的运行事实。

系统性问题：**Definition resolution 没有作为强边界存在，runtime 层在缺少 definition 时自行造事实**。

### 解决方案

新增或强化 `WorkflowGraphResolver`：

- 输入 `WorkflowGraphRef::{ById, ByKey, InlineFreeform}`。
- 输出 `ResolvedWorkflowGraph { graph_id, graph_key, version, entry_activity_key, definition }`。
- `ByKey` 解析失败必须返回 domain error，除非 intent 明确是 `InlineFreeform`。
- dispatch 不允许自己生成 graph definition identity。

这应成为 dispatch 的前置依赖，不是 dispatch 内部 helper。

### 落地记录

2026-06-02 已完成 ByKey graph resolution 收口：

- `WorkflowGraphResolver` 成为 dispatch 前置边界，`ById` 校验 project scope，`ByKey` 通过 `project_id + key` 解析 existing graph。
- `ByKey` project mismatch 返回 bad request，missing key 返回 not found。
- `dispatch_rejects_unknown_workflow_graph_key_without_creating_run` 测试证明 missing key 不会创建 run / graph instance / assignment。
- `dispatch_resolves_workflow_graph_by_key_inside_service` 测试证明 dispatch 使用 resolver 解析到已有 graph id，而不是生成 runtime identity 代替 definition identity。

Freeform 仍以 built-in graph key 表达；是否需要 `InlineFreeform` 或等价显式 freeform ref 留给 P0-06 / Phase 5 处理。

## P0-05 manual lifecycle run 绕过 dispatch

### 原始问题

manual lifecycle run 仍通过 `ActivityLifecycleRunService::start_run` 直接创建 `LifecycleRun`，没有进入统一 intent / association / frame 入口。

### 结构性分析

这说明系统仍保留“特殊启动口”。每个特殊启动口都会重新决定 run、graph、agent、subject、frame、runtime session 的创建顺序，最终破坏统一模型。

系统性问题：**Lifecycle run creation 不是单一事务边界**。

### 解决方案

让 manual run 也通过 `LifecycleRunStartIntent`：

- explicit source = manual / API
- optional subject_ref
- explicit graph ref
- explicit launch policy
- returns run + graph instance + optional root agent/frame

`ActivityLifecycleRunService::start_run` 如保留，只能成为 dispatch 内部 private use case，不能作为 API route 的直接依赖。

### 落地记录

2026-06-02 当前代码已让 manual lifecycle run 进入 `LifecycleRunStartIntent`：

- `POST /workflow/lifecycle-runs` route 构造 `LifecycleRunStartIntent { source: Api, workflow_graph_ref }` 并调用 `LifecycleDispatchService::start_lifecycle_run`。
- dispatch 内部解析 graph ref、创建 `LifecycleRun` 与 root `WorkflowGraphInstance`，并初始化 graph-instance-owned `ActivityLifecycleRunState`。
- route 后续只以返回的 `graph_instance_ref` 调用 `ActivityLifecycleRunService::launch_ready_attempts`，不再 route-local 直接创建 run state。
- `lifecycle_run_start_intent_initializes_root_graph_instance_state` 测试证明 manual start 返回 run + graph instance，且 graph instance state owner 已初始化。

## P0-06 Story root / freeform launch 未统一进入 dispatch

### 原始问题

Story runs route 主要读取 `SubjectRef("story")` 投影；未发现 Story root/freeform launch 通过 `ExecutionIntent` 创建 Story association 和 root agent。

### 结构性分析

Story 是目标模型里的核心 business subject。如果 Story root launch 不走统一 dispatch，Task / Companion / Routine 的收束都会缺一个父级 lifecycle context 的可靠来源。

系统性问题：**Subject root lifecycle 缺少统一 launch policy，Story association 更像 read projection 而非 creation invariant**。

### 解决方案

定义 `StoryLifecycleLaunchPolicy`，但实现上仍进入通用 dispatch：

- `subject_ref = Story`
- `agent_policy = create_or_reuse_story_root`
- `workflow_graph_ref = story default graph | freeform graph`
- `context_policy = story root projection`

Story service 不直接创建 session 或 run，只提交 `ExecutionIntent`。

## P1-07 runtime commands 仍以 session 为主键

### 原始问题

runtime command store 仍是 `session_runtime_commands(session_id, phase_node)`。

### 结构性分析

runtime commands 的语义通常是“改变一个 Agent 的有效运行表面”，例如 capability/context/hook/pending action。以 session 为主键会让 RuntimeSession 再次变成 capability owner。

系统性问题：**command delivery 与 frame state transition 没拆开**。

### 解决方案

拆成两层：

- `AgentFrameTransition`
  - frame / agent owned
  - durable source of capability/context/procedure change
- `RuntimeDeliveryCommand`
  - runtime session owned
  - 只负责把某个 frame revision 投递给 connector

旧 `session_runtime_commands` 只能保留为 delivery outbox，不再持有 control-plane 语义。

### 落地记录

2026-06-01 的 Phase 4 切片已把 pending runtime context path 拆为两层：

- `AgentFrameTransitionRecord` 是 frame surface transition fact，持有 target frame、run/lifecycle/phase provenance 与 `RuntimeCapabilityTransition` records。
- `RuntimeDeliveryCommand` 是 delivery outbox payload，只持有 delivery kind、`frame_transition_id` 与 target frame ref。
- `session_runtime_commands` 通过 `frame_transition_id` 关联 `agent_frame_transitions`；读取 requested commands 时由 repository join 回 frame transition，再投影为 replay input。
- 验收证据是 hub / memory persistence tests 断言 delivery payload 不再携带 transition truth，pending replay 仍能从 frame transition 恢复 capability state overlay。

## P1-08 Hook / capability API 仍以 session 为入口

### 原始问题

Hook runtime 已经是 `AgentFrameHookRuntime`，但入口仍是 `session_id -> find frame`；capability 热更新服务 API 仍叫 `replace_current_capability_state(session_id, ...)`。

### 结构性分析

这说明 implementation 虽换了内部对象，但 command boundary 仍没换。外部调用方只要继续传 session_id，就会继续把 session 当成控制面对象。

系统性问题：**API boundary 没有对齐事实源 owner**。

### 解决方案

所有 capability/hook control command 改为：

- primary target: `agent_id` / `frame_id` / `assignment_id`
- source provenance: optional `runtime_session_id` / `turn_id`

`session_id` lookup 只能存在于 runtime callback adapter 中，并且 adapter 命名要明确是 trace-to-frame resolution。

### 落地记录

2026-06-01 的 Phase 4 follow-up 已把 runtime context transition input 的 delivery 语义显式化：

- `LiveRuntimeContextTransitionInput` / `PendingRuntimeContextTransitionInput` 使用 `target_frame_id` 作为 frame surface target。
- 同一 input 中 raw runtime id 改名为 `delivery_runtime_session_id`，只用于 live connector、runtime registry、context frame event 投递与 runtime delivery outbox。
- `StepActivation` 当时仍存在 live apply 路径，会在 applier 内部解析 `target_frame_id` 再传入 delivery runtime id；该边界随后在 2026-06-02 的 live apply slice 中改为 target-first。

Phase 4 hook/capability follow-up 调研进一步确认：

- 真正的 control coupling 集中在 SPI 与 application hook 边界：`ExecutionHookProvider`、`SessionHookSnapshotQuery`、`SessionHookRefreshQuery`、`HookEvaluationQuery` 仍以 top-level `session_id` 表达 snapshot / refresh / evaluate。
- `SessionHookService`、hub hook dispatch lazy rebuild、workflow orchestrator refresh、PhaseNode activation、canvas capability sync、companion parent notification 仍把 session 当成控制命令入口，再反查 frame/hook runtime。
- `AgentFrameRuntimeTarget`、`delivery_runtime_session_id`、`RuntimeDeliveryCommand`、`AgentFrameRepository::find_by_runtime_session` 已经可以作为正确的 delivery/provenance 边界保留；问题不是删除所有 `session_id`，而是禁止 control command 以 raw session 为 owner。

后续最小封装应拆成：

- `HookControlTarget { frame_id, agent_id, run_id, assignment_id }`。
- `RuntimeAdapterProvenance { runtime_session_id, turn_id, source }`。
- frame-first hook service：load / refresh / evaluate 都以 hook target 为主，runtime-session entry 只作为 adapter 并立刻解析到 target。
- frame-first capability service：live / pending transition 都要求调用方先提供 `AgentFrameRuntimeTarget`，`resolve_runtime_session_frame_id` 只留在 runtime adapter 模块。

2026-06-01 的 implementation slice 已把 hook runtime/provider 边界推进到 frame-first：

- SPI 新增 `HookControlTarget`、`RuntimeAdapterProvenance` 与 `AgentFrameHook*Query`，并要求所有 `ExecutionHookProvider` 实现显式实现 `load_frame_snapshot` / `refresh_frame_snapshot` / `evaluate_frame_hook`；trait 不再提供把 frame query 自动转回 session query 的默认 adapter。
- `AgentFrameHookRuntime::refresh` / `evaluate` 以自身 `run_id + agent_id + frame_id` 构造 control target，`runtime_session_id` 只进入 provenance；测试 `refresh_uses_frame_target_provider_entry` 传入错误 session owner，验证 provider 收到的仍是 frame target 与 runtime provenance。
- `AppExecutionHookProvider` 可按 `HookControlTarget` 解析 active workflow snapshot：`WorkflowSnapshotBuilder::resolve_active_workflow_for_target` 通过 run、assignment 或 frame scope 找到 `ActiveWorkflowProjection`，session snapshot load 只保留为旧 adapter entry。
- canvas / hook delegate / hub 测试 provider 也被迫显式实现 frame entry，避免公共 SPI 默认把新 query 静默降级回 session query。

本 slice 仍不关闭 P1-08 gate，因为：

- `SessionHookService::ensure_hook_runtime` 初始 snapshot load 仍以 `SessionHookSnapshotQuery { session_id }` 为入口，hub hook dispatch lazy rebuild 也仍从 session id rebuild snapshot。
- workflow orchestrator、VFS/canvas/companion 的若干 control path 仍可经 `SessionHookService` 或 `resolve_runtime_session_frame_id` 把 raw runtime session 当作命令入口；workflow agent executor 虽已传递 `AgentFrameRuntimeTarget`，ContinueRoot policy 输入仍是 root runtime session。
- SPI 仍保留 `SessionHookSnapshotQuery` / `SessionHookRefreshQuery` / `HookEvaluationQuery` 作为旧 adapter/rule-engine shape；下一步需要新增 frame-first hook service，把 session-shaped entry 限定在 runtime adapter 模块内，并继续迁移 capability / companion 调用方。

2026-06-02 的 StepActivation live apply slice 进一步收缩 capability caller：

- `SessionCapabilityService::resolve_runtime_session_target` 新增为 runtime session adapter 到 `AgentFrameRuntimeTarget` 的单一解析入口；workflow 调用点不再直接依赖裸 `target_frame_id` lookup。
- `StepActivation::apply_to_frame_runtime_target` 要求调用方显式传入 `AgentFrameRuntimeTarget` 与 base capability surface，函数内部不再调用 `resolve_runtime_session_frame_id` 或自行读取 current capability state。
- `AgentActivityExecutor` 的 live 与 pending runtime context transition 统一传递 `target_frame_id + delivery_runtime_session_id`，StepActivation applier 只做 activation surface 归一化与 transition apply。

该 slice 仍不关闭 P1-08 gate，因为：

- canvas capability sync 仍从 raw `session_id` 调用 `resolve_runtime_session_frame_id` 并读取 current capability state。
- ContinueRoot 仍以 `root_runtime_session_id` 作为 policy 输入，再解析出 `AgentFrameRuntimeTarget`；这只是把 lookup 集中到 adapter，并没有拆开 agent/frame reuse policy 与 runtime delivery policy。
- companion parent notification / hook control 仍以 parent session 作为入口，尚未迁到 parent frame/assignment target。

验证记录：

- `cargo check -p agentdash-application`
- `cargo test -p agentdash-application workflow::frame_hook_runtime --lib -- --format terse`
- `cargo test -p agentdash-application hooks::provider --lib -- --format terse`
- `cargo test -p agentdash-application session::hook_delegate --lib -- --format terse`
- `cargo test -p agentdash-application session::hub::tests --lib -- --format terse`
- `cargo test -p agentdash-application workflow::step_activation --lib -- --format terse`
- `cargo test -p agentdash-application workflow::agent_executor --lib -- --format terse`
- `rg -n "SessionHookSnapshotQuery|SessionHookRefreshQuery|HookEvaluationQuery \{|ensure_hook_runtime\(|get_hook_runtime\(|resolve_runtime_session_frame_id\(" crates/agentdash-application/src` 仍命中 session service / hub dispatch / canvas / companion / orchestrator 等入口，因此 static gate 保持未通过。

## P1-09 `StepActivation` 没收束进 AgentFrameBuilder

### 原始问题

`StepActivationInput` 仍是独立 DTO，且存在 apply-to-running-session 路径。

### 结构性分析

StepActivation 是旧模型里最接近 AgentFrame 的逻辑。如果它还独立存在并直接 apply session，就意味着 AgentFrame 只是另一个存储层，没有成为 effective runtime surface 的构建边界。

系统性问题：**runtime surface builder 被拆成两套并行路径**。

### 解决方案

把 StepActivation 降级为 `AgentFrameBuilder` 内部 stage：

- `resolve_procedure`
- `resolve_context_projection`
- `resolve_effective_capability`
- `resolve_vfs_mcp_surface`
- `create_frame_revision`
- `project_runtime_delivery`

禁止 business modules 或 workflow executor 直接拿 activation output apply session。

### 落地记录

2026-06-01 的 Phase 4 follow-up 已先把 `step_activation` 从 application crate 的 public workflow API 降为 crate-internal surface：

- `workflow::step_activation` module 改为 `pub(crate)`。
- `StepActivation` / `StepActivationInput` / `activate_step_with_platform` 等 re-export 改为 `pub(crate) use`，外部 crate 不再能把 activation DTO 当成稳定 workflow API。
- `workflow` facade 只保留 application 内部实际消费者需要的 activation API；未被调用的 capability delta / key helper 不再作为边界表面存在。
- `ready_port_keys` 接回 kickoff input section 渲染，保证 activation 输入字段对应真实 prompt projection，而不是保留一段未兑现的伪封装。

该记录不关闭 P1-09 gate，因为 `SessionRequestAssembler`、companion skill projection 与 `AgentActivityExecutor` 仍在 `AgentFrameBuilder` 外部消费/修改 activation；后续需要把 activation resolution 迁入 frame surface service 或 builder-owned stage。

2026-06-02 的 AgentFrame surface slice 进一步收束 activation surface 投影：

- `AgentFrameBuilder` 模块新增 `AgentFrameSurfaceInput`，集中处理 capability state、VFS、MCP servers、execution profile 与 context bundle summary 到 AgentFrame revision 的映射；`SessionAssemblyBuilder::project_assembly_to_frame` 不再自己逐列拼写 frame surface。
- `build_lifecycle_activation_surface` 成为 lifecycle activation 的 frame-owned surface stage，把 base VFS、activation lifecycle VFS、mount directives、MCP servers 与 capability state 归一化为一份同源 surface。
- `SessionAssemblyBuilder::apply_lifecycle_activation` 改为消费 frame builder 模块产出的 surface，session 装配层只保留 prompt/extras handoff。
- 新增 `lifecycle_activation_surface_outputs_single_coherent_frame_revision`，证明同一次 activation surface 能在一个 AgentFrame revision 中同时写入 procedure、context、capability、VFS/MCP、runtime refs 与 graph activity scope。

该 slice 关闭 AgentFrameBuilder 同源 surface 测试缺口，但仍不关闭 P1-09 主 gate：`StepActivation` live apply 当时仍以 runtime session 解析 live target，companion skill projection 仍可直接改 activation，ContinueRoot 仍绕过 builder-owned stage。

2026-06-02 的 StepActivation live apply slice 去掉了 applier 内部的 session owner 职责：

- `apply_to_frame_runtime_target` 以 `AgentFrameRuntimeTarget` 为输入，保留 delivery runtime session 与 frame id 的 mismatched rejection。
- base capability surface 由调用方在 frame target 解析后显式传入，StepActivation 不再负责读取 runtime session current state。
- live transition 与 pending transition 的输入形状一致地落到 `target_frame_id + delivery_runtime_session_id`。

该 slice 仍不关闭 P1-09 主 gate，因为 `StepActivation` 仍是 workflow 层独立 DTO，`AgentActivityExecutor` 仍直接消费 activation 并决定 target，companion skill projection 仍可直接改 activation；真正的目标状态仍是 builder-owned surface stage 或 `AgentFrameSurfaceService` 成为唯一 activation -> frame transition 封装。

## P1-10 `ContinueRoot` 依赖 root RuntimeSession

### 原始问题

`ContinueRoot` 仍强依赖 root runtime session。

### 结构性分析

ContinueRoot 的本质应该是“复用某个 LifecycleAgent 继续承接 Activity”，不是“继续某个 session”。如果它以 runtime session 为条件，就不能稳定表达同 Agent 多 runtime session、session replacement、resume after trace split。

系统性问题：**Agent reuse policy 与 RuntimeSession reuse policy 混同**。

### 解决方案

重命名并重建策略：

- `AgentReusePolicy::{ContinueCurrentAgent, SpawnLifecycleAgent, ReuseBySubject, ReuseByRoutineEntity}`
- `RuntimeSessionPolicy::{AttachExisting, CreateNew, ResumeLatestTrace}`

`ContinueRoot` 不再直接存在为 executor policy，而是这两组 policy 的组合。

## P1-11 多 RuntimeSession ref selection 不清

### 原始问题

`RuntimeLaunchRequest::from_frame` 从 frame 取 runtime session refs 时只取第一个。

### 结构性分析

AgentFrame 能持有多个 runtime refs 说明模型允许 runtime trace 多样性，但 launch adapter 没有 selection policy。取第一个是隐藏业务规则。

系统性问题：**Frame state 与 runtime adapter 之间缺少 explicit selection policy**。

### 解决方案

在 frame 或 launch request 中加入 `RuntimeRefSelection`：

- latest writable
- specific trace
- active turn owner
- create new if none
- resume by connector capability

adapter 只能执行 policy，不能自行挑选。

### 落地记录

当前实现已把 selection policy 显式化：

- `RuntimeSessionSelectionPolicy::{Specific, LaunchPrimary, LatestAttached}` 位于 domain `AgentFrame` 模型。
- `RuntimeLaunchRequest::from_frame(frame, runtime_policy)` 要求调用方显式传入 policy。
- API construction provider 对当前 launch session 使用 `Specific { runtime_session_id }`，runtime adapter 不再在 launch projection 中自行猜测 target。
- 已验证测试：
  - `cargo test -p agentdash-domain runtime_session_selection_requires_explicit_policy --lib -- --format terse`
  - `cargo test -p agentdash-application workflow::runtime_launch::tests::from_frame_uses_explicit_runtime_session_policy --lib -- --format terse`

## P1-12 / P1-13 Project active agents 不是 project-scoped view

### 原始问题

缺少 `ProjectActiveAgentsView`；前端 `ActiveLifecycleList` 从全局 store 拼装，并且未使用 `projectId` 过滤。

### 结构性分析

这是 read model ownership 问题。Project runtime overview 是产品核心视图，不应由前端从全局事实表自行拼装，否则每个页面都会重复实现“哪些 run/agent 属于这个 project”的规则。

系统性问题：**Project runtime projection 没有后端 owner，前端 store 被迫成为事实组合层**。

### 解决方案

新增 `ProjectActiveAgentsView`：

- query key: `project_id`
- includes active runs, lifecycle agents, frames, subject refs, runtime traces, gates
- excludes unrelated project data by repository query boundary

前端只消费 view，不自己从 global lifecycle store 推导跨项目过滤。

## P1-14 SessionPage 仍混用控制面与 trace

### 原始问题

`/session/:id` 虽标注为 RuntimeTraceView，但仍拉 session feed/meta/projection/lineage，并可能继续承载控制面体验。

### 结构性分析

这是 UI route boundary 问题。只要 `/session/:id` 仍是用户主要运行入口，RuntimeSession 就会继续拥有 product runtime semantics。

系统性问题：**Runtime trace view 与 AgentFrame runtime view 没完全分层**。

### 解决方案

建立严格 UI 分层：

- `AgentFrameRuntimePage`: 默认工作入口。
- `SubjectExecutionPage/Panel`: Story/Task 业务入口。
- `RuntimeSessionTracePage`: 只显示 transcript、events、turns、lineage、debug replay。

SessionPage 不再主动解析 lifecycle control-plane；它只显示 trace provenance，并提供回链到 agent/frame/subject。

## P1-15 Task execution API 未进入 generated contracts

### 原始问题

Task execution response 是 route-local 手写 DTO，返回 UUID refs；前端发起后丢弃 response 再 fetch task。

### 结构性分析

这是 contract boundary 漏洞。越关键的 write path 越不应该 route-local，否则 generated contract check 覆盖不到 drift。

系统性问题：**Subject execution command/result 没有成为稳定跨层契约**。

### 解决方案

把 Task execution API 收束为通用 subject execution contract：

- command input: `SubjectExecutionRequest`
- output: `ExecutionDispatchResultDto` 或 `SubjectExecutionView`
- route: 可以保留 `/tasks/{id}/start` wrapper，但返回 generated DTO

前端应消费 response 更新 lifecycle store，而不是丢弃后再 fetch task。

## P1-16 story-specific run view 与通用 lifecycle view 不一致

### 原始问题

`story_runs` 自建 `LifecycleRunView` 时 `runtime_trace_refs` 为空，而通用 lifecycle view 会从 AgentFrame 收集 trace refs。

### 结构性分析

这是 view builder 重复。两个 route 各自拼同名 view，必然产生字段完整度漂移。

系统性问题：**Read model assembly 没有单一 owner**。

### 解决方案

`LifecycleRunViewBuilder` 成为唯一组装入口。Story route 只能调用它，再做 story-specific wrapper，不复制 view assembly。

## P1-17 `ExecutorRunRef::RuntimeSession { session_id }` 裸露

### 原始问题

ExecutorRunRef 已不叫 AgentSession，但仍裸露 `session_id` raw shape。

### 结构性分析

这在 attempt evidence 层可以接受，但 contract 形状如果仍是 raw session_id，会继续诱导前端把它作为导航根。

系统性问题：**trace evidence 没有通过 ref DTO 表达语义降级**。

### 解决方案

改为 `RuntimeSessionRefDto`，并在字段名上表达 evidence：

- `runtime_trace_ref`
- `source_runtime_session_ref`
- `executor_runtime_ref`

避免 `session_id` 出现在 high-level workflow DTO 的第一层。

## P1-18 Task `agent_binding` 仍参与执行决策

### 原始问题

Task 仍保留 `agent_binding`，并用于 executor config 决策。

### 结构性分析

Task 可以保存用户意图，但执行决策不应由 Task entity 直接拥有。否则 Task 会继续成为 runtime policy holder。

系统性问题：**Business data 与 dispatch policy 混合**。

### 解决方案

拆分：

- `TaskSpec`: 用户业务数据。
- `SubjectExecutionPreference`: 可选的执行偏好，属于 dispatch request 或 Story/Project policy。
- `AgentProcedureOverride`: 属于 execution intent。

如果 UI 需要在 Task 上编辑偏好，也应写入 policy store 或 task projection metadata，不作为 Task runtime truth。

## P1-19 Task cancel 以 runtime session cancel 表达

### 原始问题

Task cancel 通过 current frame 找 runtime session，再调用 `cancel_session`。

### 结构性分析

取消一个 Task subject execution 不是取消一个 session。session cancel 是 transport action；subject cancel 应影响 gate/assignment/attempt/run projection。

系统性问题：**用户意图 command 与 runtime transport command 混同**。

### 解决方案

新增 `CancelSubjectExecutionCommand`：

- input `SubjectRef`
- resolves active agent assignment / gate / runtime refs
- writes lifecycle cancellation intent
- then projects runtime cancellation delivery

RuntimeSession cancel 只能是该 command 的 delivery step。

## P1-20 Companion gate 未完全成为交互面 owner

### 原始问题

Companion dispatch 已使用 dispatch/gate，但 parent notification、human wait、轮询仍依赖 session notification / hook runtime。

### 结构性分析

Companion 是交互通道，Gate 是 durable wait/resume fact，RuntimeSession notification 是 delivery。当前三者仍混在一起。

系统性问题：**CompanionChannel、LifecycleGate、RuntimeNotification 没分层**。

### 解决方案

封装：

- `CompanionChannel`: message/result protocol。
- `LifecycleGate`: durable wait state。
- `GateDeliveryAdapter`: 把 gate state 投递到 runtime session notification。

工具和 UI 查询 gate/channel，不直接轮询 session notification 作为 truth。

## P1-21 Routine Reuse 策略无 anchor lookup

### 原始问题

Routine `Reuse` 映射为 `ReuseExisting`，但无 parent_run_id 时仍会新建 run。

### 结构性分析

Routine reuse 应按 routine/entity/source subject 找 agent，而不是依赖 parent run。当前策略名与查询实现不一致。

系统性问题：**Reuse policy 没有自己的 repository/query boundary**。

### 解决方案

建立 `LifecycleAgentReuseResolver`：

- by project agent
- by routine execution/entity
- by subject association
- by current active run

Routine strategy 只转换为 resolver policy，不直接传 parent_run_id。

### 落地记录

2026-06-02 已完成 Routine Reuse anchor lookup 收口：

- `LifecycleAgentReuseResolver` 成为 Routine Reuse / PerEntity 的 application-layer 查询封装，输入 `Routine + RoutineExecution`，输出可复用的 `run_id + agent_id + frame_id + assignment_id` 或明确的无 target / conflict。
- resolver 使用 `RoutineExecution.dispatch_refs` 作为历史 dispatch evidence，并校验对应 `LifecycleRun` project、`LifecycleAgent` run/project/active 状态、`AgentFrame` owner、`AgentAssignment` agent/frame bridge，以及 `SubjectRef(kind=routine_execution)` 的 `LifecycleSubjectAssociation`。
- `DispatchStrategy::Reuse` 没有有效 anchor 时返回 conflict，不再通过 `RunPolicy::ReuseExisting` 缺 `parent_run_id` 的路径创建新 run。
- `DispatchStrategy::PerEntity` 必须解析非空 `entity_key`；已有同 entity target 时复用 resolver 输出的 run + agent，首次 entity 触发则显式创建新的 per-entity lifecycle anchor。
- `LifecycleDispatchService` 对 explicit `parent_agent_id` 执行严格校验，同一 run 多 active agent 时只复用指定 agent；底层 `RunPolicy::ReuseExisting` / `AppendGraph` 缺 `parent_run_id` 会拒绝。

验证记录：

- `cargo test -p agentdash-application routine::reuse_resolver --lib -- --format terse`
- `cargo test -p agentdash-application routine::dispatch --lib -- --format terse`
- `cargo test -p agentdash-application workflow::dispatch_service --lib -- --format terse`
- `cargo check -p agentdash-application`

## P1-22 Permission source runtime session 边界需明确

### 原始问题

PermissionGrant 已有 run provenance + effect frame anchor，但仍携带 source runtime session。

### 结构性分析

这不一定是错误。权限申请确实来自某个 runtime turn/tool。问题在于 query 和 effect owner 是否仍以 session 为中心。

系统性问题：**provenance 与 effect owner 容易混淆**。

### 解决方案

明确字段和 API：

- `source_runtime_session_ref`: audit provenance only。
- `effect_frame_ref`: capability effect owner。
- `scope_subject_association_ref`: control scope owner。

查询入口优先 frame/run/subject，session 只作为审计过滤条件。

## P2-23 shared-library legacy step normalization

### 原始问题

shared-library 仍接受 `entry_step_key / steps / edges` legacy template normalization。

### 结构性分析

预研阶段明确不保留兼容路径。旧 template normalization 是入口级兼容，会把旧概念继续带入新模型。

系统性问题：**资产入口仍允许旧语义流入目标模型**。

### 解决方案

移除 legacy normalization，或至少改为 explicit migration command，而不是 runtime import 自动兼容。默认 import 应只接受 target `Workflow / Activity` vocabulary。

## P2-24 `WorkflowContract` 命名残留

### 原始问题

`WorkflowContract` 仍作为单 Agent `AgentProcedure.contract` 的类型和 UI 文案。

### 结构性分析

本轮重构最重要的概念边界之一就是 Workflow graph 与 AgentProcedure contract。这个名字残留会持续诱导后续代码把二者混用。

系统性问题：**definition vocabulary 没有完成所有权重命名**。

### 解决方案

将单 Agent contract 统一命名为 `AgentProcedureContract` 或最终确定的 procedure contract 名。UI 文案也必须同步，避免“Workflow”在 graph 和 procedure 中双重含义。

## P2-25 schema readiness 只查表

### 原始问题

schema readiness 只检查目标表存在，不检查关键列、旧列删除、索引/约束完整性。

### 结构性分析

大型硬切后，如果只查表存在，就无法证明 clean baseline 和 forward migrations 真正收束。

系统性问题：**migration correctness 没有 executable invariant**。

### 解决方案

增加 schema assertion：

- required columns
- forbidden legacy columns/tables
- indexes/unique constraints
- FK constraints
- generated contract compatibility if applicable

用 clean DB + migrated dev DB 两种路径验证。

## P2-26 E2E 仍以 session stream 验证 runtime

### 原始问题

E2E 仍大量使用 `/session/:id` 和 `/sessions/:id/stream/ndjson` 验证 Task / Story runtime。

### 结构性分析

RuntimeSession trace 测试是必要的，但如果它是主要验证方式，就会掩盖 SubjectExecutionView / AgentFrameRuntimeView 是否完整。

系统性问题：**验证入口仍偏 runtime substrate，而不是 target read model**。

### 解决方案

为每个关键链路增加 target-view assertions：

- ProjectAgent launch returns agent/frame/runtime refs。
- Task start returns subject execution / dispatch refs。
- terminal updates SubjectExecutionView。
- AgentFrameRuntimeView shows capability/context/gates。
- RuntimeSessionTraceView 只作为下钻验证。

## P2-27 缺少 ProjectActiveAgentsView

### 原始问题

没有 `ProjectActiveAgentsView` contract / API / service / test。

### 结构性分析

Project runtime overview 是跨 run/agent/subject/frame 的典型 projection。如果不提供稳定后端 view，前端会继续拼装。

系统性问题：**project-level runtime projection 没有边界对象**。

### 解决方案

新增 generated `ProjectActiveAgentsView`，由后端聚合 project-scoped lifecycle agents、subjects、frames、gates、runtime traces。

## P2-28 DTO 家族填充不一致

### 原始问题

通用 lifecycle view 与 story-specific view 对 trace refs 等字段填充不一致。

### 结构性分析

这不是字段遗漏，而是同一个 read model 有多个 builder。

系统性问题：**read model builder ownership 分裂**。

### 解决方案

所有 route 使用共享 view builder；业务 route 只包一层 subject-specific metadata。

## P2-29 旧词消失不等于新模型内聚

### 原始问题

`WorkflowContract`、legacy conversion、session trace E2E 等残留说明旧字段被删掉后，新模型仍可能靠旧语义运行。

### 结构性分析

机械删旧路径只能减少表面噪声，不能证明模型高内聚。真正的判断是：每个事实是否有唯一 owner，每个 command 是否有稳定 ingress，每个 projection 是否有唯一 builder。

系统性问题：**重构成功标准过于依赖 grep，而不是 architecture invariants**。

### 解决方案

为目标模型写 executable invariants：

- no control command targets raw session id except runtime adapters。
- all subject execution writes through dispatch。
- all run views use common builder。
- all activity terminal paths resolve assignment independent of current frame revision。
- no route-local DTO for lifecycle/subject/agent/frame command results。

## P2-30 未运行完整 check

### 原始问题

当前审计未执行 `pnpm run check`。

### 结构性分析

静态审计足以定位结构问题，但不能证明运行链路妥善。

系统性问题：**质量门禁没有覆盖结构性架构目标**。

### 解决方案

后续实现任务的验证必须包含：

- unit tests for terminal resolver / graph instance state owner / dispatch result。
- contracts check。
- frontend tests for ProjectActiveAgentsView / SubjectExecution panels。
- critical E2E asserting subject/agent/frame views before session trace。
- schema assertions。
