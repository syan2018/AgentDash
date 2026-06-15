# Research: session-entry-audit

- Query: 审计当前代码与 specs 中 session-first runtime 入口散落情况，为 AgentRun runtime entry session 残留收束提供迁移路线。
- Scope: internal
- Date: 2026-06-15

## Findings

### Related Specs

- `.trellis/tasks/06-15-agentrun-runtime-entry-session-convergence/prd.md`：要求区分可保留的 message stream / connector trace API、应迁到 AgentRun runtime address 的业务 API、应迁到 orchestration node coordinate 的 node execution API，以及测试/adapter 残留。
- `.trellis/tasks/06-15-agentrun-runtime-entry-session-convergence/design.md`：目标模型为 `AgentRunRuntimeAddress + optional MessageStreamRef + optional OrchestrationNodeCoordinate`，RuntimeSession 只作为 message stream / trace ref。
- `.trellis/tasks/06-15-agentrun-runtime-entry-session-convergence/implement.md`：建议先建立 address/ref types 与 anchor adapter，再迁 workspace、mailbox、hook、task effect。
- `.trellis/spec/backend/session/architecture.md`：明确 RuntimeSession 是 delivery / trace substrate；AgentRun delivery/control command 使用 AgentRun workspace public identity；通过 runtime session 反查业务上下文时只能走 anchor 证据链。
- `.trellis/spec/backend/session/runtime-execution-state.md`：明确 `SessionRuntimeRegistry`、turn supervisor、connector live session 与 active turn 是运行态问题；AgentRun workspace command 由 AgentRun mailbox / command receipt 负责，RuntimeSession trace endpoint 保留只读 trace/detail。
- `.trellis/spec/backend/workflow/architecture.md`：明确 RuntimeSession 是 connector delivery / trace evidence；Agent node execution identity 使用 `AgentInvocation(lifecycle_run_id, orchestration_id, node_path, attempt, agent_run_id, frame_id)`，RuntimeSession 只作为 terminal/runtime evidence。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`：明确 `LifecycleSubjectAssociation` 是业务 subject 归属入口；RuntimeSession 只能通过 `RuntimeSessionExecutionAnchor` 反查到 run / agent / frame / optional node。

### Files Found

- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs`：定义 `RuntimeDeliverySelectionPolicy` 与 `RuntimeSessionExecutionAnchor`，当前是 RuntimeSession 到 control-plane / node 的核心反查索引。
- `crates/agentdash-domain/src/workflow/repository.rs`：定义 `RuntimeSessionExecutionAnchorRepository`，包含 `find_by_session`、`list_by_run`、`list_by_agent`、`latest_for_agent` 等查询入口。
- `crates/agentdash-application/src/session/launch/orchestrator.rs`：session launch 主入口，connector delivery 仍以 `session_id` claim、prepare、start、commit、attach。
- `crates/agentdash-application/src/session/launch/planner.rs`：launch planning 用 `session_id` 解析 hook runtime、restore transcript、backend placement、terminal effect handler 与 mailbox delegate。
- `crates/agentdash-application/src/session/runtime_control.rs`：RuntimeSession detail/control 的 cancel 入口，以 session id 操作 turn supervisor、connector cancel、event replay。
- `crates/agentdash-application/src/session/agent_run_mailbox.rs`：AgentRun mailbox service 已有 run/agent 字段，但 command 与 scheduler 仍要求 `runtime_session_id` 并从 session 反查 control plane。
- `crates/agentdash-application/src/session/mailbox_delegate.rs`：runtime delegate 在 connector hook boundary 中以当前 `runtime_session_id` 调度 / drain mailbox。
- `crates/agentdash-application/src/session/hooks_service.rs`：hook runtime 目标已使用 `AgentFrameRuntimeTarget` / `HookControlTarget`，但仍用 `delivery_runtime_session_id` 反查 anchor 来构造 control target。
- `crates/agentdash-application/src/workflow/agent_run_workspace/query.rs`：AgentRun workspace query 从 run/agent 进入，但内部选择 latest delivery runtime session 并以 session meta / session state 派生 workspace 状态。
- `crates/agentdash-application/src/workflow/agent_run_workspace/types.rs`：workspace snapshot / list projection 暴露 `delivery_runtime_session_id` 与 `delivery_trace_meta`，trace meta 是合法 message stream projection ref，但命名还未收束到 `MessageStreamProjectionRef`。
- `crates/agentdash-application/src/workflow/session_run_context_resolver.rs`：提供 `resolve_for_session` 与 `resolve_for_run` 两条路径；前者是 session-first subject context adapter。
- `crates/agentdash-application/src/workflow/dispatch_service.rs`：业务 dispatch 创建 RuntimeSession、写 anchor、graph-backed dispatch 写 `NodeStarted` 且 executor ref 为 RuntimeSession。
- `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs`：lifecycle journey 的 session projection 是合法 trace view；但 `step_session_id` / `current_step_session_id` 把 node execution 读取降成 session id。
- `crates/agentdash-application/src/task/context_builder.rs`：task context builder 通过 latest agent anchor / runtime session 反查 active workflow projection 和 visible canvas mounts。
- `crates/agentdash-application/src/task/gateway/effect_executor.rs`：task hook effects 从 terminal hook handler 的 session id 出发校验 task ownership，并写 artifact/status context。
- `crates/agentdash-application/src/task/service.rs` 与 `crates/agentdash-application/src/task/view_projector.rs`：task execution refs / readonly task projection 从 subject association 出发，但仍通过 anchors 的 session-backed node refs 找 orchestration node。

### Current Session-first Entry Inventory

#### 1. Message Stream / Connector Trace

这些入口可以保留 session-first，因为它们操作的是 RuntimeSession trace identity、connector live session、transcript、compaction 或 terminal/event replay，而不是业务 owner。

- `SessionLaunchOrchestrator::launch(session_id, command)` 先检查 session meta、claim turn、构建 frame construction，再进入 prepare/start/commit/attach；这是 delivery runtime 启动链路，session id 是 stream/connector trace identity。代表行：`crates/agentdash-application/src/session/launch/orchestrator.rs:30`、`:42`、`:47`、`:88`、`:127`、`:223`、`:244`、`:270`。
- `LaunchPlanner::plan` 用 `session_id` 恢复 transcript、解析 follow-up executor session、装配 hook runtime delegate 和 terminal hook effect handler；这些是 connector-facing launch projection / trace restore。代表行：`crates/agentdash-application/src/session/launch/planner.rs:30`、`:115`、`:160`、`:181`、`:206`、`:222`。
- `SessionRuntimeControlService::cancel(session_id)` 操作 turn supervisor、connector cancel、session event replay，并在缺 terminal 时补 interrupted event；这是 `/sessions/{id}/runtime-control` 语境。代表行：`crates/agentdash-application/src/session/runtime_control.rs:36`、`:40`、`:71`、`:101`。
- `LifecycleJourneyService::read_session_projection(session_id, rest)` 读取 meta、events、items、messages、tools、writes、summaries、turns、terminal；这是 journey / transcript / trace browser。代表行：`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:71`、`:83`、`:112`、`:116`、`:124`、`:148`、`:150`、`:168`。
- `RuntimeSessionExecutionAnchor` 本身可以保留作为 trace backlink：它记录 launch evidence，不是业务 owner。代表行：`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20`、`:29`、`:47`、`:68`。

保留原则：这些 API 名称应继续暴露为 `RuntimeSession` / `MessageStream` / `Trace` / `Transcript` 语义；不要在这些入口上追加 workspace、mailbox、hook policy、capability、VFS ownership 判断，除非判断先转换到 AgentRun control-plane address。

#### 2. Business Runtime

这些入口不应继续以 session 作为业务入口，应迁到 `AgentRunRuntimeAddress { run_id, agent_id, frame_id }`，并把 session 留作 optional `MessageStreamProjectionRef`。

- `AgentRunMailboxUserMessageCommand` 与 `AgentRunMailboxControlCommand` 同时包含 run/agent 和 `runtime_session_id`，但 `accept_user_message` 仍通过 `resolve_control_plane(&command.runtime_session_id)` 反查 run/agent/frame，然后再校验 target。代表行：`crates/agentdash-application/src/session/agent_run_mailbox.rs:71`、`:86`、`:151`、`:174`、`:185`。
- Mailbox message 持久化、command digest、accepted refs、scheduler 均把 runtime session 作为 command 事实的一部分。代表行：`crates/agentdash-application/src/session/agent_run_mailbox.rs:207`、`:243`、`:280`、`:331`。
- Mailbox control actions `delete_message` / `promote_message` / `resume_mailbox` 的 command 入口携带 runtime session，仅用于 accepted refs / runtime state / schedule。代表行：`crates/agentdash-application/src/session/agent_run_mailbox.rs:469`、`:506`、`:533`、`:578`、`:605`。
- `AgentRunMailboxService::schedule` 仍需要 `runtime_session_id`，先读取 session execution state，再 claim/consume mailbox。代表行：`crates/agentdash-application/src/session/agent_run_mailbox.rs:805`、`:822`、`:837`、`:853`、`:879`。
- `schedule_for_runtime_session` 和 `drain_agent_run_turn_boundary_for_delegate` 是明确 session-first adapter；它们从 runtime session 反查 run/agent 后再调度。代表行：`crates/agentdash-application/src/session/agent_run_mailbox.rs:911`、`:921`。
- `AgentRunMailboxRuntimeDelegate` 以 runtime session 作为唯一字段，hook boundary / before_stop / after_turn 都由该 session 驱动 mailbox route。代表行：`crates/agentdash-application/src/session/mailbox_delegate.rs:42`、`:61`、`:97`、`:121`、`:174`、`:286`、`:329`。
- `AgentRunWorkspaceQueryService::resolve` 从 run/agent 进入是正确方向，但 `delivery_runtime_session_for_agent_run` 返回 latest session，随后 workspace shell、conversation state、steering support、list projection 仍强依赖 session meta/execution state。代表行：`crates/agentdash-application/src/workflow/agent_run_workspace/query.rs:57`、`:63`、`:66`、`:118`、`:127`、`:196`、`:220`、`:243`、`:249`、`:307`。
- `AgentRunWorkspaceSnapshot` / `AgentRunListProjection` 中的 `delivery_runtime_session_id` 与 `delivery_trace_meta` 可以保留为 optional projection ref，但不应成为 workspace status / command enablement 的事实入口。代表行：`crates/agentdash-application/src/workflow/agent_run_workspace/types.rs:21`、`:43`、`:65`、`:77`。
- `SubjectRunContextResolver::resolve_for_session` 从 session 反查 subject context；应只保留给 hook/trace adapter，业务调用迁到 `resolve_for_run` 或 AgentRun address。代表行：`crates/agentdash-application/src/workflow/session_run_context_resolver.rs:37`、`:38`、`:86`。
- `task/context_builder.rs` 的 `build_task_session_context` 接受 optional `runtime_session_id`，并通过 latest session 或 `find_by_session` 取 active workflow / visible canvas mounts。代表行：`crates/agentdash-application/src/task/context_builder.rs:44`、`:73`、`:183`、`:220`、`:246`。

建议迁移顺序：

1. 先引入 `AgentRunRuntimeAddress` 与 `MessageStreamProjectionRef`，并在 mailbox/workspace query 层提供 `resolve_runtime_address(run_id, agent_id)`，返回 current frame + optional latest delivery trace ref。
2. 迁 `AgentRunWorkspaceQueryService`：workspace state 的输入改为 AgentRun address + optional message stream projection；trace meta 只参与 `delivery_trace_meta`，command availability 从 AgentRun mailbox / current frame / active turn projection 组合得到。
3. 迁 mailbox public command structs：删除 command target 里的 required `runtime_session_id`，改为 address + optional delivery ref。`schedule_for_runtime_session` / delegate drain 暂作为 connector adapter 保留，但内部立刻转换到 address。
4. 迁 task context builder：从 task subject association / AgentRun address / current frame 出发取 workflow projection 和 visible mounts；`runtime_session_id` 只作为 trace ref 写入上下文 evidence。

#### 3. Orchestration Node

这些入口应迁到 `OrchestrationNodeCoordinate { run_id, orchestration_id, node_path, attempt }`，RuntimeSession 只作为 `RuntimeTraceRef` / executor evidence。

- `RuntimeSessionExecutionAnchor` 目前把 `orchestration_id`、`node_path`、`node_attempt` 挂在 session anchor 上。作为 trace backlink 可以保留，但 node execution API 不应从 session anchor 推导 node coordinate。代表行：`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:34`、`:68`。
- `WorkflowDispatchService` graph-backed dispatch 在创建 session anchor 后，用 `ExecutorRunRef::RuntimeSession { session_id }` materialize `NodeStarted`。这个写法作为 trace evidence 合法，但后续 node status advance / artifact / projection 应以 orchestration coordinate 为入口。代表行：`crates/agentdash-application/src/workflow/dispatch_service.rs:392`、`:410`、`:413`、`:416`。
- `LifecycleJourneyService::attempt_session_id`、`step_session_id`、`current_step_session_id` 从 node state 提取 session id 作为 step 读取目标。这适合“打开该 node 的 trace”快捷入口，但不适合成为 node execution coordinate。代表行：`crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:572`、`:579`、`:584`。
- `TaskExecutionService::resolve_task_execution_refs` 从 task association 找 agent，再 `list_by_agent` 遍历 anchors 并用 anchor 内 node refs 定位 runtime node。它输出的是 task execution refs，目标应直接从 association + orchestration node state 得出 coordinate，再附加 runtime trace evidence。代表行：`crates/agentdash-application/src/task/service.rs:51`、`:82`、`:145`。
- `Task view projector` 也把 `RuntimeSessionExecutionAnchor` 作为 task view 到 node fact 的桥。其只读投影方向是对的，但关键索引应改为 node coordinate-first。代表行：`crates/agentdash-application/src/task/view_projector.rs:7`、`:96`、`:221`、`:279`。
- `TaskHookEffectExecutor::validate_runtime_task_anchor` 从 session handler 反查 task association 并写 tool artifact/status。由于 hook effect 是 terminal trace 回调，此处可以保留 adapter，但 artifact ownership 与 target 校验应落到 `OrchestrationNodeCoordinate` 或 task subject association + AgentRun address。代表行：`crates/agentdash-application/src/task/gateway/effect_executor.rs:176`、`:180`、`:207`、`:231`。

建议迁移顺序：

1. 定义或复用 `OrchestrationNodeCoordinate`，并在 task service / projector 内部先生成 coordinate，再查 `LifecycleRun.orchestrations[]`。
2. 将 `RuntimeSessionExecutionAnchor` 上的 node fields 视为 `RuntimeTraceRef -> node coordinate` backlink，仅供 trace 入口反查；不要让 task projection 以 session anchor 为主 key。
3. 为 journey helper 新增 `step_coordinate` / `current_step_coordinate`，原 `step_session_id` 降级为 trace open helper。
4. terminal hook effect handler 从 session callback 中提取 trace ref 后，校验 target 时转到 coordinate/address，再执行 artifact/status effect。

#### 4. Tests Or Adapters

这些入口可以短期保留，但需要命名上标记 adapter/test 语义，避免新业务代码继续调用。

- `RuntimeSessionExecutionAnchorRepository::find_by_session`、`list_by_run`、`list_by_agent`、`latest_for_agent` 是过渡期必须存在的 anchor repo API；其中 `find_by_session` 应仅用于 trace adapter，`list_by_run` / `latest_for_agent` 可帮助生成 optional message stream projection。代表行：`crates/agentdash-domain/src/workflow/repository.rs:134`、`:141`、`:146`。
- `AgentRunMailboxService::schedule_for_runtime_session` 与 `drain_agent_run_turn_boundary_for_delegate` 是 connector/runtime delegate adapter。代表行：`crates/agentdash-application/src/session/agent_run_mailbox.rs:911`、`:921`。
- `SessionHookService::ensure_hook_runtime_for_target` 当前已经接受 `AgentFrameRuntimeTarget`，并用 delivery session 校验缓存；这是较好的过渡形态。代表行：`crates/agentdash-application/src/session/hooks_service.rs:25`、`:54`、`:66`。
- `SessionHookService::resolve_hook_runtime` 是 launch-path adapter，以 `session_id + expected_frame_id` 构造 `AgentFrameRuntimeTarget`。代表行：`crates/agentdash-application/src/session/hooks_service.rs:116`、`:126`。
- `build_frame_hook_runtime` 校验 target agent owns delivery RuntimeSession，这个检查可以保留在 adapter 层，但业务 target 已经是 `HookControlTarget`。代表行：`crates/agentdash-application/src/session/hooks_service.rs:391`、`:423`、`:428`、`:442`。
- `task/view_projector.rs` 内部 in-memory `RuntimeSessionExecutionAnchorRepository` test double 只是测试支撑，保留到生产 projector 改完后同步调整即可。代表行：`crates/agentdash-application/src/task/view_projector.rs:663`。

### Which Session-first Entries Can Stay

- RuntimeSession repository/meta/event/projection/lineage/branching/compaction/transcript APIs 可以保留 session-first，因为它们的查询对象就是 trace/message stream 本身。
- `SessionRuntimeControlService::cancel(session_id)` 可保留在 RuntimeSession detail/control surface，但 AgentRun workspace cancel 应另有 AgentRun address 入口，再选择 optional active delivery runtime ref 执行 cancel。
- Launch orchestrator / planner 可以保留 session-first，前提是其上游业务入口先决定 AgentRun address、frame surface、mailbox command，再把 delivery ref 传入 launch pipeline。
- `RuntimeSessionExecutionAnchorRepository::find_by_session` 可以保留给 trace-to-control-plane backlink、debug、terminal callback adapter，但不应用作 workspace/mailbox/hook/task 的 public business target。
- Lifecycle journey `read_session_projection` 可以保留 session-first，因为它读的是 session meta/events/items/messages/summaries/terminal。

### Which Entries Should Move To AgentRunRuntimeAddress

- Mailbox user/control commands：`AgentRunMailboxUserMessageCommand`、`AgentRunMailboxControlCommand`，以及 `accept_user_message`、`delete_message`、`promote_message`、`resume_mailbox`。
- Mailbox scheduler public side：`schedule(run_id, agent_id, runtime_session_id, ...)` 应改为 `schedule(address, optional_message_stream_ref, ...)`；delegate-only adapter 可以从 session ref 转 address 后调用新 API。
- AgentRun workspace query/list projection：`delivery_runtime_session_for_agent_run` 应成为 optional `MessageStreamProjectionRef` resolver；workspace shell/status/commands 不应从 session meta/state 开始。
- Subject context / task context：业务调用应使用 `resolve_for_run` 或 AgentRun address；`resolve_for_session` 只做 hook/trace adapter。
- Hook target rebuild：外部 hook control target 已经是 `HookControlTarget { run_id, agent_id, frame_id }`，应把 `AgentFrameRuntimeTarget` 重命名或拆成 `AgentRunRuntimeAddress + MessageStreamProjectionRef`，使 delivery session 不再像 target owner。

### Which Entries Should Move To OrchestrationNodeCoordinate

- Task execution refs / task view projector：从 `SubjectRef -> LifecycleSubjectAssociation -> OrchestrationNodeCoordinate -> RuntimeNodeState` 解析，RuntimeSession anchor 只补充 trace evidence。
- Task hook effect artifact/status validation：terminal callback 可携带 message stream ref，但 effect target 应校验 task subject association + node coordinate / AgentRun address。
- Lifecycle journey step helpers：新增 coordinate-first helper，session id helper 只负责打开某 node 的 runtime trace。
- Graph-backed dispatch 的 `NodeStarted` 已经以 `orchestration_id + node_path + attempt` 写事件，保持这一点；避免后续 complete/advance API 接受 session id 再回推 node。

### Parallel / Dependency Relationship With AgentRunLifecycleSurfaceProjector

- 主任务可以并行推进，不需要等待本审计的所有迁移完成。
- 主任务 projector 应直接采用 `AgentRunRuntimeAddress + Option<MessageStreamProjectionRef> + Option<OrchestrationNodeProjectionInput>`，并把 RuntimeSession 限定为 optional projection/evidence。
- 本审计建议的第一阶段 adapter 可以为主任务提供命名和转换 helper，但不是主任务 blocker。
- 主任务最需要避免的是复用 `FrameRuntimeSurface { runtime_session_id }` 或 `agent_run_session` 这类 session-owned 命名来表达 workspace/resource surface。`FrameRuntimeSurface::from_frame(frame, runtime_session_id)` 当前把 frame surface 与 session ref 混在一个 DTO 中，后续应拆成 frame surface + delivery/message stream ref。代表行：`crates/agentdash-application/src/workflow/runtime_launch.rs:39`、`:43`。
- 迁 mailbox/hook/task effect 时应复用主任务沉淀的 address/ref types，避免再造一套 `AgentFrameRuntimeTarget`、`delivery_runtime_session_id`、`runtime_session_id` 并行命名。

### Recommended Follow-up Implementation Subtasks

#### Subtask 1: Application runtime address/ref contracts

- File scope: `crates/agentdash-application/src/workflow/runtime_launch.rs`, `crates/agentdash-application/src/workflow/agent_run_workspace/types.rs`, `crates/agentdash-application/src/workflow/agent_run_workspace/query.rs`, possibly a new small value-object module under `workflow`.
- Work: introduce `AgentRunRuntimeAddress`, `MessageStreamProjectionRef`, and optional conversion from `RuntimeSessionExecutionAnchor`; split frame runtime surface from delivery trace ref.
- Risk: DTO churn may affect generated API/frontend if public contracts currently expose `delivery_runtime_session_id` directly.
- Validation commands: `cargo test -p agentdash-application workflow::agent_run_workspace`, `cargo check -p agentdash-application`; if DTO/export changes, also `pnpm run contracts:check` and `pnpm --filter app-web typecheck`.

#### Subtask 2: AgentRun mailbox command target convergence

- File scope: `crates/agentdash-application/src/session/agent_run_mailbox.rs`, `crates/agentdash-application/src/session/mailbox_delegate.rs`, route/API callers that construct `AgentRunMailboxUserMessageCommand` or `AgentRunMailboxControlCommand`.
- Work: make public mailbox commands target `AgentRunRuntimeAddress`; keep delegate-only `schedule_for_runtime_session` as adapter that resolves address + message stream ref, then calls address-first scheduler.
- Risk: scheduler still needs live delivery runtime for steer/cancel/launch decisions; migration must distinguish command target from optional active delivery channel.
- Validation commands: `cargo test -p agentdash-application session::agent_run_mailbox`, `cargo test -p agentdash-application session::mailbox_delegate`, `cargo check -p agentdash-application`.

#### Subtask 3: Orchestration node coordinate for task projection/effects

- File scope: `crates/agentdash-application/src/task/service.rs`, `crates/agentdash-application/src/task/view_projector.rs`, `crates/agentdash-application/src/task/context_builder.rs`, `crates/agentdash-application/src/task/gateway/effect_executor.rs`, `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs`.
- Work: add coordinate-first resolver for task execution refs and task view; demote session anchor to trace evidence; add journey coordinate helper; make task hook effect validation verify task/node/address before using session trace.
- Risk: task status/artifact projection is user-visible and can silently under-project if coordinate selection changes; tests should cover graph-backed task with node terminal and visible canvas mounts.
- Validation commands: `cargo test -p agentdash-application task::`, `cargo test -p agentdash-application workflow::lifecycle::journey`, `cargo check -p agentdash-application`.

#### Subtask 4: Hook runtime target naming cleanup

- File scope: `crates/agentdash-application/src/session/hooks_service.rs`, `crates/agentdash-application/src/session/types.rs`, `crates/agentdash-application/src/workflow/frame_hook_runtime.rs`, hook-related tests in `session/hook_delegate.rs`.
- Work: replace or wrap `AgentFrameRuntimeTarget { frame_id, delivery_runtime_session_id }` with address + message stream projection naming; keep session cache as runtime adapter binding.
- Risk: hook runtime rebuild is on launch path; stale cache handling must keep matching frame id and delivery session validation.
- Validation commands: `cargo test -p agentdash-application session::hook_delegate`, `cargo test -p agentdash-application session::hooks_service`, `cargo check -p agentdash-application`.

## Caveats / Not Found

- No external references were needed; this audit is based on task artifacts, Trellis specs, and internal Rust code only.
- The scan was limited to the user-requested files/directories plus directly relevant lines inside them. API route handlers, frontend service code, infrastructure repository implementations, migrations, and generated DTOs were not exhaustively audited.
- `RuntimeSessionExecutionAnchorRepository` implementation files were not audited in this pass; only the domain trait and in-file test doubles were inspected.
- Several session-first call sites are intentionally inside launch/connector/trace substrate. They should not be mechanically renamed away without first adding AgentRun-addressed business entry points above them.
- Some `AgentRunWorkspaceQueryService` session usage may remain necessary as optional delivery trace projection until a durable AgentRun active-turn projection exists; the migration should avoid replacing a session dependency with duplicated in-memory state.
