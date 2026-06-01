# Research: Phase 4 AgentFrame Runtime Surface

- Query: Phase 4 收束 AgentFrame 作为 runtime surface owner，覆盖 P1-07 ~ P1-11
- Scope: internal
- Date: 2026-06-01

## Findings

### Files Found

- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md` - Phase 4 gate 明确要求 StepActivation 入 builder、拆分 transition/delivery、Hook/capability target 改为 agent/frame/assignment、多 RuntimeSession selection 显式化。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/design.md` - Boundary 5 定义 `AgentFrameSurfaceService`，并明确 `AgentFrameTransition` 与 `RuntimeDeliveryCommand` 的语义拆分。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/structural-analysis.md` - P1-07 ~ P1-11 的原始结构风险说明。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/raw-exposed-issues-checklist.md` - 原始 checklist 中 P1-07 ~ P1-11 的暴露问题。
- `.trellis/spec/backend/session/architecture.md` - Session 目标语义是 RuntimeSession，只拥有 turn/tool/event/projection/trace，不拥有 Agent effective surface。
- `.trellis/spec/backend/session/runtime-execution-state.md` - 当前 pending runtime command baseline 仍以 `SessionRuntimeCommandStore` 为事实源。
- `.trellis/spec/backend/workflow/architecture.md` - Agent Activity execution identity 应由 `AgentAssignment(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)` 定位，RuntimeSession 只作为 evidence。
- `.trellis/spec/backend/capability/architecture.md` - CapabilityScope 由 association / AgentFrame / PermissionGrant 推导，支持 AgentFrame 成为 surface owner。
- `.trellis/spec/backend/hooks/architecture.md` - Hook 决策在 loop 边界消费，Agent loop 不查询 workflow/task/story/project repository。
- `crates/agentdash-domain/src/workflow/agent_frame.rs` - `AgentFrame` revision 与 runtime session refs、`RuntimeSessionSelectionPolicy`。
- `crates/agentdash-application/src/workflow/frame_builder.rs` - `AgentFrameBuilder` 当前 builder surface 边界。
- `crates/agentdash-application/src/workflow/step_activation.rs` - `StepActivationInput` / `StepActivation` 仍独立，包含 running-session applier。
- `crates/agentdash-application/src/session/assembler.rs` - `SessionRequestAssembler` 仍负责 owner/story/lifecycle/companion compose，再投影到 frame builder。
- `crates/agentdash-api/src/bootstrap/session_construction_provider.rs` - RuntimeSession launch 通过 `session_id -> AgentFrame -> RuntimeLaunchRequest`，必要时重新 compose frame。
- `crates/agentdash-application/src/workflow/runtime_launch.rs` - `RuntimeLaunchRequest::from_frame` 当前从 frame 投影 runtime adapter 输入。
- `crates/agentdash-application/src/session/hooks_service.rs` / `crates/agentdash-application/src/workflow/frame_hook_runtime.rs` - Hook runtime 已带 agent/frame 字段，但创建和 SPI query 仍 session-first。
- `crates/agentdash-application/src/session/capability_service.rs` / `crates/agentdash-application/src/session/hub/tool_builder.rs` / `crates/agentdash-application/src/session/hub/runtime_context_transition.rs` - capability live/pending hot update 仍以 session 为 primary command target。
- `crates/agentdash-spi/src/session_persistence.rs` / `crates/agentdash-infrastructure/migrations/0035_session_runtime_commands.sql` - runtime command record/table 仍为 `session_id + phase_node`。
- `crates/agentdash-application/src/workflow/agent_executor.rs` - ContinueRoot 仍以 root RuntimeSession 为控制条件，并直接 apply running session。
- `crates/agentdash-domain/src/workflow/dispatch.rs` / `crates/agentdash-application/src/workflow/dispatch_service.rs` - 已有 `AgentPolicy` / `RuntimePolicy`，但不是 Phase 4 目标的 AgentReusePolicy + RuntimeSessionPolicy split。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - `find_by_runtime_session` 当前按 runtime ref 反查最新 frame。

### Related Specs

- `.trellis/spec/backend/session/architecture.md:5` 定义 Session 目标语义为 RuntimeSession，不拥有业务归属、permission scope、Lifecycle progress 或 Agent effective surface。
- `.trellis/spec/backend/session/architecture.md:29` 要求目标控制面中 `AgentFrame` 是 capability / context / VFS / MCP / runtime refs 的事实源。
- `.trellis/spec/backend/session/architecture.md:30` 要求业务 command path 从 `ExecutionIntent`、`SubjectRef`、run/agent/frame refs 或 graph instance refs 开始，RuntimeSession 只能是 delivery / trace substrate。
- `.trellis/spec/backend/workflow/architecture.md:15` 要求 Agent Activity execution identity 由 assignment + frame 定位，RuntimeSession 只作为 terminal/runtime evidence。
- `.trellis/spec/backend/session/runtime-execution-state.md:119` 到 `:138` 仍记录当前 pending runtime command store baseline，这与 Phase 4 的拆分目标存在张力。

### External References

- 未使用外部资料；本次为纯代码调研。
- 本地验证入口来自 `package.json`: `backend:check = cargo check`，`backend:test = cargo test --workspace`，可针对 crate 缩小为 `cargo test -p agentdash-application ...` / `cargo test -p agentdash-domain ...`。

## 原始现象

### 1. StepActivation / StoryStepActivation / AgentFrameBuilder / SessionRequestAssembler / RuntimeLaunchRequest 当前边界

- `AgentFrame` 已经表达 revision 级 runtime surface：`effective_capability_json`、`context_slice_json`、`vfs_surface_json`、`mcp_surface_json`、`runtime_session_refs_json`、`execution_profile_json` 在 `crates/agentdash-domain/src/workflow/agent_frame.rs:25` 到 `:50`。
- `AgentFrameBuilder` 当前只是“已解析 surface 的写入器”：字段在 `crates/agentdash-application/src/workflow/frame_builder.rs:24` 到 `:36`，`with_capability_state` 把 `CapabilityState` 拆成 capability/VFS/MCP surface，见 `frame_builder.rs:82` 到 `:91`；`build` 从 repo 读取 current frame、carry forward runtime refs / procedure / context / surfaces，再 create 新 revision，见 `frame_builder.rs:152` 到 `:222`。
- `StepActivation` 仍是独立 DTO 和纯 resolver：`StepActivationInput` 在 `crates/agentdash-application/src/workflow/step_activation.rs:44` 到 `:76`，`StepActivation` 输出 capability/MCP/tool directives/kickoff/lifecycle VFS/mount directives，见 `step_activation.rs:102` 到 `:119`，`activate_step_with_platform` 在 `step_activation.rs:123` 到 `:219`。
- `SessionRequestAssembler` 仍是实际 compose owner：`compose_owner_bootstrap` 生成 `SessionAssemblyBuilder`，见 `crates/agentdash-application/src/session/assembler.rs:709` 到 `:773`；`compose_story_step` 生成 story/task 的 session assembly，见 `assembler.rs:1045` 到 `:1093`；`compose_lifecycle_node_with_audit` 仍直接调用 `activate_step_with_platform`，见 `assembler.rs:1276` 到 `:1307`。
- `project_assembly_to_frame` 才把 `SessionAssemblyBuilder` 投影到 `AgentFrameBuilder`，见 `crates/agentdash-application/src/session/assembly_builder.rs:372` 到 `:416`。这说明 builder 目前没有内置 `resolve_procedure / resolve_context_projection / resolve_effective_capability / resolve_vfs_mcp_surface` 阶段，只接收 assembler 的产物。
- `StoryStepActivationService` 已经不再直接创建 session 或 activation：`start_task_inner` 构造 `SubjectExecutionIntent` 并交给 dispatch，见 `crates/agentdash-application/src/task/service.rs:118` 到 `:136`；`continue_task_inner` 同样走 intent，见 `task/service.rs:163` 到 `:181`。这是 Phase 4 的可复用正向样例。
- `RuntimeLaunchRequest` 已改成从 frame 投影，并要求传入 `RuntimeSessionSelectionPolicy`：结构字段在 `crates/agentdash-application/src/workflow/runtime_launch.rs:48` 到 `:81`，`from_frame(frame, runtime_policy)` 在 `runtime_launch.rs:89` 到 `:159`。
- API 层 `SessionConstructionProvider` 仍以 raw `session_id` 进入：`build_frame_construction` 先 `find_by_runtime_session(&session_id)`，见 `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:75` 到 `:91`；direct request 用 `RuntimeLaunchRequest::from_frame(&frame, runtime_session_policy(input.session_id.as_str()))`，见 `session_construction_provider.rs:93` 到 `:107`。
- 同一 provider 在 direct frame 不 ready 时会按 companion/lifecycle task/project agent 等来源重新 compose frame，见 `session_construction_provider.rs:137` 到 `:166`；最终 `persist_composed_frame` build frame、更新 `LifecycleAgent.current_frame_id`、再投影 launch request，见 `session_construction_provider.rs:607` 到 `:634`。

### 2. 仍绕过 frame builder 或直接 apply running session 的路径

- ContinueRoot 是最明显的直接 apply running session 路径：`apply_continue_root_activity` 通过 `session_hooks.ensure_hook_runtime(root_runtime_session_id, None)` 得到 hook runtime，见 `crates/agentdash-application/src/workflow/agent_executor.rs:353` 到 `:357`，然后调用 `apply_to_running_session`，见 `agent_executor.rs:391` 到 `:400`。
- `apply_to_running_session` 用 `hook_runtime.session_id()` 读取当前 capability state，并构造 `LiveRuntimeContextTransitionInput { session_id, ... }` 调 `SessionCapabilityService.apply_live_runtime_context_transition`，见 `crates/agentdash-application/src/workflow/step_activation.rs:289` 到 `:323`。
- ContinueRoot 没有 live hook runtime 时会 enqueue pending runtime context transition 到 root session：`PendingRuntimeContextTransitionInput { session_id: root_runtime_session_id, ... }` 在 `crates/agentdash-application/src/workflow/agent_executor.rs:471` 到 `:490`。
- Runtime launch orchestrator 在 `LaunchPlanner` 之后又按 `session_id` 查 frame，并把 `launch_plan.context.turn.capability_state` 写成新 AgentFrame revision，见 `crates/agentdash-application/src/session/launch/orchestrator.rs:282` 到 `:318`。这条路径用 builder，但不是由 frame builder owning compose；并且没有同步更新 `LifecycleAgent.current_frame_id`，会让“当前 frame”语义依赖 repo `get_current(agent_id)` 而不是 agent row。
- SpawnChild 创建 runtime session 后直接 `attach_runtime_session_ref(frame.id, &runtime_session_id)` 修改既有 frame row，见 `crates/agentdash-application/src/workflow/agent_executor.rs:296` 到 `:313`；Postgres repo 的 `attach_runtime_session_ref` 是 UPDATE 当前 frame 的 `runtime_session_refs_json`，不是新 revision，见 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:451` 到 `:470`。
- `dispatch_service.create_initial_frame` 只把 graph/activity scope 和 optional runtime ref 写入 initial frame，见 `crates/agentdash-application/src/workflow/dispatch_service.rs:539` 到 `:553`。实际 surface 仍要等 session construction provider / launch path 后补，导致 frame builder 不是唯一构建边界。

### 3. Hook / capability / runtime command primary target 是否仍是 session_id

- Hook SPI 仍 session-primary：`HookRuntimeAccess::session_id` 在 `crates/agentdash-spi/src/hooks/mod.rs:467` 到 `:474`；`SessionHookSnapshotQuery.session_id` 在 `hooks/mod.rs:539` 到 `:545`；`SessionHookRefreshQuery.session_id` 在 `hooks/mod.rs:547` 到 `:555`；`HookEvaluationQuery.session_id` 在 `hooks/mod.rs:645` 到 `:665`；`ExecutionHookProvider::load_session_snapshot/refresh_session_snapshot` 仍接收 session query，见 `hooks/mod.rs:856` 到 `:868`。
- `AgentFrameHookRuntime` 内部已带 `run_id / agent_id / frame_id / frame_revision`，见 `crates/agentdash-application/src/workflow/frame_hook_runtime.rs:34` 到 `:40`，但 `runtime_session_id` 仍是 trait 的公开 identity，`session_id()` 返回它，见 `frame_hook_runtime.rs:200` 到 `:204`；`runtime_snapshot` 仍输出 `session_id`，见 `frame_hook_runtime.rs:238` 到 `:247`。
- Hook runtime 的生产入口仍是 session lookup：`build_frame_hook_runtime(hub, session_id, ...)` 先 `find_by_runtime_session(session_id)`，见 `crates/agentdash-application/src/session/hooks_service.rs:162` 到 `:200`。
- Capability service 仍 session-primary：`get_runtime_mcp_servers(session_id)`、`get_current_capability_state(session_id)`、`get_latest_capability_state(session_id)`、`list_requested_runtime_commands(session_id)`、`enqueue_pending_capability_state_transition(session_id, ...)` 在 `crates/agentdash-application/src/session/capability_service.rs:30` 到 `:62`。
- Live/pending transition input 结构仍带 `session_id`：`LiveRuntimeContextTransitionInput.session_id` 在 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:27` 到 `:41`；`PendingRuntimeContextTransitionInput.session_id` 在 `runtime_context_transition.rs:50` 到 `:63`。
- Capability hot update 的底层 primitive 仍是 `replace_current_capability_state(&self, session_id, state)`，并通过 `find_by_runtime_session(session_id)` 定位 frame，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:75` 到 `:112`；connector 同步也调用 `connector.update_session_tools(session_id, ...)`，见 `tool_builder.rs:138` 到 `:156`。
- Runtime command 数据模型仍 session-primary：`RuntimeCommandRecord.session_id` / `phase_node` 在 `crates/agentdash-spi/src/session_persistence.rs:356` 到 `:370`；`SessionRuntimeCommandStore::upsert_runtime_command_request(session_id, ...)` 和 `list_requested_runtime_commands(session_id)` 在 `session_persistence.rs:785` 到 `:791`。
- DB 表仍是 `session_runtime_commands(id, session_id, transition_id, phase_node, status, payload_json, ...)`，见 `crates/agentdash-infrastructure/migrations/0035_session_runtime_commands.sql:1` 到 `:14`，索引也是 `(session_id, status)`，见 `0035_session_runtime_commands.sql:19` 到 `:20`。
- Store implementation 会按 `session_id + phase_node + requested` supersede 旧 command，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:690` 到 `:720`；launch 入口读取 `list_requested_runtime_commands(&sid)`，见 `crates/agentdash-application/src/session/launch/orchestrator.rs:68` 到 `:84`。

### 4. RuntimeDeliveryCommand 与 AgentFrameTransition 当前状态

- 代码中未发现 `RuntimeDeliveryCommand` 或 `AgentFrameTransition` 类型；只有任务设计文档定义了拆分目标，见 `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/design.md:133` 到 `:134`。
- 当前 `PendingCapabilityStateTransition` 同时承载 frame truth intent 和 runtime delivery intent：字段包括 `run_id/lifecycle_key/phase_node/capability_keys/transition/source_turn_id`，见 `crates/agentdash-spi/src/session_persistence.rs:45` 到 `:57`，但它被 `RuntimeCommandRecord.session_id` 包装后进入 session command store，见 `session_persistence.rs:356` 到 `:370`。
- Pending command replay 发生在 launch/preparation：`pending_transition_application = deps.capability.apply_pending_runtime_context_transitions_on_turn(&session_id, ...)`，见 `crates/agentdash-application/src/session/launch/preparation.rs:157` 到 `:181`；connector accepted 后仅把 command ids 标记 applied/failed，见 `crates/agentdash-application/src/session/launch/commit.rs:70` 到 `:94` 和 `:160` 到 `:188`。
- Permission grant 是更接近目标形态的局部样例：`apply_frame_effect` 以 `grant.effect_frame_id` 找 anchor frame，再取 `get_current(anchor_frame.agent_id)`，通过 `AgentFrameBuilder` 写新 capability revision，见 `crates/agentdash-application/src/permission/service.rs:267` 到 `:325`。缺口是它没有对应的 runtime delivery command，只是保留 runtime refs。

### 5. ContinueRoot / AgentReusePolicy / RuntimeSessionPolicy / 多 RuntimeSession selection 当前实现

- 当前 workflow contract 仍是 `AgentSessionPolicy::{SpawnChild, ContinueRoot, AttachExisting}`，见 `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:49` 到 `:56` 和 `crates/agentdash-contracts/src/workflow.rs:238` 到 `:245`。
- Dispatch 层已有较接近的拆分，但命名和语义还不是 Phase 4 目标：`AgentPolicy::{Create, Reuse, Resume, SpawnChild}` 与 `RuntimePolicy::{CreateRuntimeSession, AttachExisting(Uuid), ContinueCurrent(Uuid)}` 在 `crates/agentdash-domain/src/workflow/dispatch.rs:33` 到 `:68`。
- `LifecycleDispatchService.resolve_or_create_agent` 对 `Reuse | Resume` 只是取 run 内第一个 active agent，见 `crates/agentdash-application/src/workflow/dispatch_service.rs:507` 到 `:523`；`resolve_or_create_runtime_session` 对 `AttachExisting | ContinueCurrent` 直接返回 id，对 create 则调用 runtime session creator，见 `dispatch_service.rs:634` 到 `:657`。这仍缺少 `ReuseBySubject` / `ReuseByRoutineEntity` / `ResumeLatestTrace` 等策略级选择依据。
- ContinueRoot 仍强依赖 root RuntimeSession：`start_continue_root` 先检查 running ContinueRoot 冲突，见 `crates/agentdash-application/src/workflow/agent_executor.rs:655` 到 `:672`，然后要求 `root_runtime_session_id` 非空，见 `agent_executor.rs:674` 到 `:678`，创建 assignment 时传 `Some(&self.context.root_runtime_session_id)`，见 `agent_executor.rs:685` 到 `:695`，最后返回同一个 root runtime session，见 `agent_executor.rs:697` 到 `:712`。
- 多 RuntimeSession selection 已从“裸 first”推进到 explicit enum：`RuntimeSessionSelectionPolicy::{Specific, LaunchPrimary, LatestAttached}` 在 `crates/agentdash-domain/src/workflow/agent_frame.rs:8` 到 `:20`；`select_runtime_session_id` 在 `agent_frame.rs:135` 到 `:146`。但 `LaunchPrimary` 仍是 refs array 的 first，`LatestAttached` 仍是 refs array 的 last，策略语义很薄。
- `AgentFrameBuilder.build` carry forward current refs，再 append 新 refs，见 `crates/agentdash-application/src/workflow/frame_builder.rs:159` 到 `:171`。这让 array order 成为隐式业务顺序。
- Postgres `find_by_runtime_session` 会返回所有包含该 runtime ref 的 frame 中 `created_at DESC LIMIT 1`，见 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:473` 到 `:493`。如果 ContinueRoot 或 shared root session 被多个 frame/agent 引用，session -> frame 反查会倾向“最新写入 frame”，不一定是当前 command 的 agent/frame/assignment。
- 当前调用点已经混用 selection：API session construction 用 `Specific { runtime_session_id }`，见 `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:721` 到 `:725`；Task cancel 用 `LatestAttached`，见 `crates/agentdash-application/src/task/service.rs:208` 到 `:221`；Permission test 保留 `LaunchPrimary`，见 `crates/agentdash-application/src/permission/service.rs:686` 到 `:704`；companion parent notification 用 `LatestAttached`，见 `crates/agentdash-application/src/companion/tools.rs:662` 到 `:668`。

## 过度耦合判断

1. `StepActivation` 仍是 parallel runtime surface builder。它的 output 是 capability/VFS/MCP/procedure-context 相关事实，但被 `SessionRequestAssembler` 和 `AgentActivityExecutor` 分别消费；尤其 `apply_to_running_session` 让 phase activation 可以不产生 frame transition 而直接改 live session。

2. `SessionRequestAssembler` 是实质 surface resolver，`AgentFrameBuilder` 是 persistence facade。当前命名已经说 builder 是唯一构造路径，但实现上 builder 没拥有 procedure/context/capability/VFS/MCP resolution stage，只在 `project_assembly_to_frame` 后半段接收结果。

3. Hook/capability/runtime command 的 public contract 仍让调用方认为 session 是控制面对象。`AgentFrameHookRuntime` 内部字段已经进步到 frame scope，但 SPI trait/query、service API、store schema 都仍是 `session_id` primary。

4. Runtime command store 同时表达“要改变什么 runtime surface”和“投递到哪个 runtime session”。这就是 P1-07 的核心耦合：transition fact 与 delivery outbox 没有拆开，因此下一轮 prompt replay、live connector update、frame revision 写入都围绕 session store 绕行。

5. ContinueRoot 把“复用 Agent 承接 activity”和“复用 root RuntimeSession”绑死。它会把 root session ref 写入新 frame/assignment，并通过 root session 更新 live/pending capability；这使同一个 RuntimeSession 可能成为多个 frame 的 ref，进一步放大 `find_by_runtime_session` 返回 latest frame 的不确定性。

6. 多 RuntimeSession selection 已经有 enum，但仍偏 adapter helper，不是一等 domain policy。`Specific` 解决了“不小心 first()”的一部分问题；但 `LaunchPrimary` / `LatestAttached` 仍依赖 refs array order，未表达 latest writable、active turn owner、specific trace、create new if none、resume by connector capability 等业务语义。

## 推荐封装

### 1. AgentFrameSurfaceService / AgentFrameBuilder 内部 stage

最小高内聚目标不是继续扩大 `SessionRequestAssembler`，而是把它拆成 frame surface builder 的输入 stage：

- `resolve_procedure`: 从 activity/procedure key/owner launch intent 解析 `AgentProcedureRef`。
- `resolve_context_projection`: 产出 frame-owned context summary 和 launch-only `SessionContextBundle`。
- `resolve_effective_capability`: 使用 `CapabilityResolver` / dimension pipeline 产出 `CapabilityState`。
- `resolve_vfs_mcp_surface`: 合成 lifecycle VFS、mount directives、MCP servers。
- `create_frame_revision`: 只在这里调用 `AgentFrameBuilder.build`。
- `project_runtime_delivery`: 从 frame revision + explicit runtime selection policy 生成 delivery command 或 launch request。

`StepActivationInput` 可以先降级为 private/internal stage input，保留纯函数计算，但不要从 `workflow/mod.rs` public re-export；business executor 只能提交 `AgentFrameTransition` 或 `FrameSurfaceBuildInput`，不能拿 activation output 直接 apply session。

### 2. AgentFrameTransition vs RuntimeDeliveryCommand

建议的最小拆分：

- `AgentFrameTransition`: control-plane fact，primary target 是 `frame_id` / `agent_id` / `assignment_id` 之一；payload 是 replayable dimension records、context projection diff、procedure binding、runtime ref attach intent；provenance 可带 `runtime_session_id` / `turn_id`。
- `AgentFrameTransitionRepository`: 保存 transition request/applied/failed 或直接保存 applied transition 与 resulting `frame_id + revision`。Phase 4 最小可先在 application 层建立 service + tests，DB 可后续落表，但不能继续把 session command store 当 truth owner。
- `RuntimeDeliveryCommand`: adapter/outbox fact，primary target 是 `runtime_session_id`，payload 只引用 `frame_id + frame_revision + delivery_kind`，例如 `launch`, `update_tools_live`, `apply_next_turn`, `notify_context_frame`。
- 旧 `session_runtime_commands` 若继续保留，语义应改成 delivery outbox：`session_id` 只表示要投递到哪个 runtime adapter，不再保存 capability/control-plane truth。payload 应指向 frame revision 或 transition id，而不是成为 transition truth。

### 3. Hook/capability target 收束

最小 API 切片：

- 新建 `FrameHookRuntimeAccess` 或扩展现有 `HookRuntimeAccess` 以暴露 `frame_scope(): { run_id, agent_id, frame_id, frame_revision }`，同时把 `session_id()` 降级为 `runtime_session_id()`。
- `SessionHookSnapshotQuery` / `SessionHookRefreshQuery` / `HookEvaluationQuery` 增加 frame/agent/assignment primary target；`session_id` 只放在 `trace` / `provenance` 子字段。
- `SessionCapabilityService` 增加 `replace_frame_capability_state(FrameCapabilityTarget, state, RuntimeDeliveryProvenance)`；`replace_current_capability_state(session_id, ...)` 只保留在 runtime adapter 层内部，不给 workflow executor 调用。
- `LiveRuntimeContextTransitionInput` / `PendingRuntimeContextTransitionInput` 改成 `frame_target + delivery_policy + provenance`；它们可以生成 `AgentFrameTransition`，再由 delivery service 决定 live update 或 next-turn delivery。

### 4. ContinueRoot policy split

建议先建立显式 mapping，而不是一次性重写全部 workflow contract：

- `AgentReusePolicy::{ContinueCurrentAgent, SpawnLifecycleAgent, ReuseBySubject, ReuseByRoutineEntity}`。
- `RuntimeSessionPolicy::{AttachExisting(runtime_session_id), CreateNew, ResumeLatestTrace, DeliverToActiveTrace}`。
- `AgentSessionPolicy::ContinueRoot` 先作为兼容枚举输入，在 executor/dispatch boundary 立即翻译为 `AgentReusePolicy::ContinueCurrentAgent + RuntimeSessionPolicy::DeliverToActiveTrace`，之后 workflow contract 再移除旧枚举。
- ContinueRoot 应先选择 agent/assignment/frame，再生成 frame transition；root session 只用于 delivery/provenance，不应决定 assignment/frame owner。

### 5. RuntimeSession selection policy

当前 `RuntimeSessionSelectionPolicy::Specific/LaunchPrimary/LatestAttached` 可以作为 Phase 4 的起点，但应上移到 launch/delivery policy：

- `SpecificTrace(runtime_session_id)`: 只用于 adapter callback 或用户明确选择 trace。
- `ActiveTurnOwner`: live update 必须投递到当前 active turn 所在 session。
- `LatestWritable`: 需要 next-turn delivery 时选择可写 trace。
- `CreateNewIfNone`: launch path 可以显式表达无 refs 时创建。
- `ResumeByConnectorCapability`: connector 支持 executor state restore 时选择可 resume trace。

adapter 只能执行 policy 结果，不能在 `RuntimeLaunchRequest::from_frame` 里自行挑选。

## 验证 Gate

### Gate 1: AgentFrameBuilder 输出同源 frame revision

可验证命令：

- `cargo test -p agentdash-application workflow::frame_builder`
- 新增后建议跑：`cargo test -p agentdash-application workflow::frame_builder::tests::build_frame_revision_projects_complete_runtime_surface`
- 辅助静态检查：`rg -n "activate_step_with_platform\\(|StepActivationInput|project_assembly_to_frame|with_capability_state|with_vfs_typed|with_mcp_servers" crates/agentdash-application/src`

必须改的最小代码切片：

- 在 `crates/agentdash-application/src/workflow/frame_builder.rs` 或新 `frame_surface_service.rs` 增加一个高层 build API，输入 lifecycle/story/owner compose intent，内部调用原 `activate_step_with_platform` 和 capability/context/VFS/MCP resolution。
- 把 `crates/agentdash-application/src/session/assembler.rs:1276` 到 `:1307`、`:1619` 到 `:1636` 这类 direct activation 调用迁入 builder/service stage。
- 保留 `project_assembly_to_frame` 作为过渡 adapter 可以，但新增测试必须证明 procedure、context、capability、VFS/MCP、runtime refs、execution profile 来自同一次 build，并生成一个 frame revision。

### Gate 2: Runtime command 只表达 delivery，frame transition 有独立事实源

可验证命令：

- `rg -n "session_runtime_commands|RuntimeCommandRecord|PendingCapabilityStateTransition|AgentFrameTransition|RuntimeDeliveryCommand" crates/agentdash-*`
- `cargo test -p agentdash-application session::launch::commit::tests::runtime_command_apply_commit_failure_marks_failed_and_returns_error`
- 新增后建议跑：`cargo test -p agentdash-application workflow::frame_transition`

必须改的最小代码切片：

- 新增 `AgentFrameTransition` application/domain type，primary target 使用 agent/frame/assignment，不含 mandatory session id。
- 新增 `RuntimeDeliveryCommand` type，payload 引用 `frame_id + frame_revision + transition_id`，session id 只作为 delivery target。
- `SessionRuntimeCommandStore::upsert_runtime_command_request(session_id, transition)` 改为 delivery outbox API，或新增 parallel store 后把旧接口调用点收敛到 adapter 层。
- `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:147` 到 `:215` 的 pending path 应先写 frame transition，再 enqueue delivery command。

### Gate 3: Hook/capability control command primary target 是 agent/frame/assignment

可验证命令：

- `rg -n "HookRuntimeAccess|SessionHookSnapshotQuery|SessionHookRefreshQuery|HookEvaluationQuery|replace_current_capability_state|LiveRuntimeContextTransitionInput|PendingRuntimeContextTransitionInput" crates/agentdash-spi/src crates/agentdash-application/src`
- `cargo test -p agentdash-application session::hub::tests::replace_current_capability_state_updates_active_turn_capability_state`
- 新增后建议跑：`cargo test -p agentdash-application workflow::frame_hook_runtime`

必须改的最小代码切片：

- SPI hook query 增加 frame/agent/assignment target，`session_id` 改名或搬到 provenance。
- `build_frame_hook_runtime` 不应以 `session_id -> find frame` 作为业务入口；只允许 runtime adapter callback 走 trace-to-frame resolver。
- `SessionCapabilityService` 新增 frame-target API，并把 `apply_to_running_session` 删除或改成 `apply_frame_transition_then_deliver_live`。
- 测试要覆盖：调用方给 `frame_id` 后，capability update 写 AgentFrame revision；只有 runtime delivery adapter 收到 raw session id。

### Gate 4: 多 RuntimeSession ref selection 有显式 policy 测试，禁止默认 first()

可验证命令：

- `cargo test -p agentdash-domain workflow::agent_frame::tests::runtime_session_selection_requires_explicit_policy`
- `cargo test -p agentdash-application workflow::runtime_launch::tests::from_frame_uses_explicit_runtime_session_policy`
- `rg -n "first_runtime_session_id|runtime_session_ids\\(\\).*first|select_runtime_session_id\\(RuntimeSessionSelectionPolicy::LaunchPrimary\\)|select_runtime_session_id\\(RuntimeSessionSelectionPolicy::LatestAttached\\)" crates/agentdash-*`

必须改的最小代码切片：

- 删除或禁止无 policy 的 selection helper；当前未发现 `first_runtime_session_id` 调用，但 `LaunchPrimary` 仍等价 first，需要限制使用场景。
- `RuntimeLaunchRequest::from_frame` 继续要求 explicit policy；API provider 的 `Specific` 是正确方向，见 `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:721` 到 `:725`。
- 为 `LatestWritable` / `ActiveTurnOwner` / `CreateNewIfNone` 至少补一个 failing-first unit test，防止以后继续把 array order 当业务规则。

### 补充 Gate: ContinueRoot 拆成 AgentReusePolicy + RuntimeSessionPolicy

可验证命令：

- `rg -n "ContinueRoot|AgentSessionPolicy|AgentPolicy|RuntimePolicy|root_runtime_session_id" crates/agentdash-domain/src crates/agentdash-application/src crates/agentdash-contracts/src`
- `cargo test -p agentdash-application workflow::agent_executor`
- 新增后建议跑：`cargo test -p agentdash-application workflow::agent_executor::tests::continue_root_selects_agent_before_runtime_delivery`

必须改的最小代码切片：

- 在 activity executor boundary 增加 `AgentReusePolicy + RuntimeSessionPolicy` 解析函数，把 `AgentSessionPolicy::ContinueRoot` 翻译为显式组合。
- `start_continue_root` 不再把 `root_runtime_session_id` 传给 `create_agent_activity_assignment` 作为 frame identity；先 resolve/reuse agent/frame/assignment，再 delivery 到 root session。
- `apply_continue_root_activity` 不再调用 `apply_to_running_session`；改为写 AgentFrameTransition，并由 RuntimeDeliveryCommand 决定 live update 或 next-turn delivery。
- 增加多 frame 共享 root session 的测试，验证 hook/capability target 不会被 `find_by_runtime_session(... ORDER BY created_at DESC)` 错绑到最新 frame。

## Caveats / Not Found

- 未运行 `cargo test` 或 `cargo check`；本文件给出的是调研结论与建议验证命令。
- 未发现代码中已有 `RuntimeDeliveryCommand` / `AgentFrameTransition` 类型；它们目前只存在于 task design/analysis 文档。
- `AgentFrameBuilder` 已经是 frame revision 的主要写入工具，但不是 surface resolution owner；Phase 4 不应只改命名，需要把 `StepActivation` / `SessionRequestAssembler` 的 resolution stage 收束进去。
- `RuntimeSessionSelectionPolicy::Specific` 已经降低了 P1-11 的“默认 first”风险，但 `LaunchPrimary` / `LatestAttached` 仍依赖 refs array order，不能算完整 selection policy。
