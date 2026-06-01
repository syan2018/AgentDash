# Research: Phase 4/5 Frame Interaction Current Audit

- Query: 旁路审计 Phase 4/5 的结构性风险：AgentFrame surface owner、hook/capability target、companion/gate、task cancel、routine reuse、permission provenance 是否仍存在模型过度耦合。
- Scope: internal
- Date: 2026-06-02

## Findings

### Files Found

- `.trellis/workflow.md` — Trellis 阶段、task/research 输出约束与实现前置阅读流程。
- `.trellis/spec/project-overview.md` — 项目边界与分层约束。
- `.trellis/spec/backend/session/architecture.md` — session/runtime/frame 边界相关规范入口。
- `.trellis/spec/backend/session/session-startup-pipeline.md` — runtime 启动、session assembler、frame projection 相关规范。
- `.trellis/spec/backend/session/runtime-execution-state.md` — runtime execution state 与 session delivery 的语义边界。
- `.trellis/spec/backend/hooks/execution-hook-runtime.md` — hook runtime 的 frame target 与 runtime provenance 规范。
- `.trellis/spec/backend/capability/architecture.md` — capability surface 与 AgentFrame 的归属边界。
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md` — capability/VFS/MCP 维度投影规范。
- `.trellis/spec/backend/permission/architecture.md` — permission grant frame anchor 与 runtime provenance 规范。
- `.trellis/spec/backend/permission/policy-engine.md` — permission policy/effect 应用规范。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` — lifecycle run/agent/frame/assignment 分层规范。
- `.trellis/spec/backend/story-task-runtime.md` — Story/Task execution 与 runtime 调度相关规范。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md` — frontend workflow/task execution 投影规范。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/prd.md` — 本轮结构审计目标和验收口径。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/design.md` — Phase 4/5 结构设计意图。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md` — Phase 4/5 已完成/未完成 gate 清单。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/structural-analysis.md` — P1-08/P1-09/P1-10/P1-20/P1-21/P1-22 风险定义。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/research/phase4-stepactivation-continue-root-followup.md` — Phase 4 StepActivation/ContinueRoot 既有跟进研究。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/research/phase4-hook-capability-targets-followup.md` — Phase 4 hook/capability target 既有跟进研究。
- `crates/agentdash-application/src/workflow/frame_builder.rs` — AgentFrameBuilder 和 frame revision projection。
- `crates/agentdash-application/src/workflow/step_activation.rs` — StepActivation DTO 与 running-session apply 路径。
- `crates/agentdash-application/src/workflow/agent_executor.rs` — workflow agent assignment、ContinueRoot 与 pending frame transition。
- `crates/agentdash-application/src/session/hub/runtime_context_transition.rs` — live/pending runtime context transition 与 delivery command。
- `crates/agentdash-application/src/session/hub/tool_builder.rs` — runtime session -> frame 解析与 capability live update。
- `crates/agentdash-application/src/session/assembler.rs` — session assembly 到 AgentFrameBuilder/StepActivation 的投影。
- `crates/agentdash-application/src/workflow/frame_hook_runtime.rs` — frame-first hook runtime adapter。
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs` — session hub hook runtime 创建入口。
- `crates/agentdash-application/src/session/hooks_service.rs` — session facade hook snapshot/refresh 入口。
- `crates/agentdash-application/src/hooks/provider.rs` — production hook provider frame/session 双路径实现。
- `crates/agentdash-application/src/canvas/tools.rs` — canvas capability live update 调用路径。
- `crates/agentdash-application/src/companion/tools.rs` — companion dispatch/wait/resume/notification 与 parent routing。
- `crates/agentdash-application/src/workflow/lifecycle_gate_service.rs` — durable lifecycle gate service。
- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs` — LifecycleGate domain model。
- `crates/agentdash-application/src/task/service.rs` — task start/continue/cancel service。
- `crates/agentdash-api/src/routes/task_execution.rs` — task execution routes。
- `packages/app-web/src/services/story.ts` — frontend task execution API consumption。
- `crates/agentdash-application/src/routine/dispatch.rs` — routine execution intent/reuse mapping。
- `crates/agentdash-application/src/routine/executor.rs` — routine reuse run resolution。
- `crates/agentdash-application/src/permission/service.rs` — permission grant query/effect application。
- `crates/agentdash-domain/src/permission/entity.rs` — permission grant frame anchor/runtime provenance fields。
- `crates/agentdash-domain/src/permission/repository.rs` — permission repository query surface。
- `crates/agentdash-api/src/routes/project_agents.rs` — freeform project agent launch route。
- `crates/agentdash-api/src/routes/story_runs.rs` — story execution projection route。

### Gate Status

| Gate | Status | Code Evidence | Model Over-Coupling |
| --- | --- | --- | --- |
| Phase 4: AgentFrame transition 与 RuntimeDeliveryCommand 分离 | pass | `AgentFrameRuntimeTarget` 显式包含 `frame_id` 与 `delivery_runtime_session_id`（`crates/agentdash-application/src/session/types.rs:58`），`RuntimeDeliveryCommand` 只记录 delivery target frame/transition（`crates/agentdash-spi/src/session_persistence.rs:426`），live transition 同时写 frame transition 与 runtime delivery outbox（`crates/agentdash-application/src/session/hub/runtime_context_transition.rs:195`）。 | 否。核心数据模型已把 frame effect 与 runtime delivery 拆开。 |
| Phase 4: StepActivation 纳入 AgentFrame surface owner / AgentFrameBuilder | fail | `StepActivationInput`/`StepActivation` 仍是 workflow 层独立 DTO（`crates/agentdash-application/src/workflow/step_activation.rs:44`, `crates/agentdash-application/src/workflow/step_activation.rs:91`），assembler 直接消费 activation（`crates/agentdash-application/src/session/assembler.rs:1377`, `crates/agentdash-application/src/session/assembler.rs:1600`），companion skill projection 直接改 `activation.lifecycle_vfs`（`crates/agentdash-application/src/companion/skill_projection.rs:9`, `crates/agentdash-application/src/companion/skill_projection.rs:50`）。 | 是。procedure/context/capability/VFS/MCP/runtime refs 的同源装配还不是一个封装内的 frame surface 阶段，StepActivation 仍可被多个上层 owner 拼装。 |
| Phase 4: AgentFrameBuilder 覆盖 StepActivation 同源 surface 测试 | pass | `AgentFrameBuilder` 通过 `AgentFrameSurfaceInput` 统一吸收 capability/VFS/MCP/context/execution profile surface，`build_lifecycle_activation_surface` 负责 lifecycle activation surface 归一化；`lifecycle_activation_surface_outputs_single_coherent_frame_revision` 覆盖一次 activation surface 同时产出 procedure/context/capability/VFS/MCP/runtime refs/activity scope。 | 否。测试 gate 已闭合；剩余耦合在 StepActivation 独立 DTO、companion projection 与 ContinueRoot 入口，不在同源 frame revision 断言本身。 |
| Phase 4: hook snapshot/refresh/evaluate 以 AgentFrame target 为 primary target | partial | SPI 已有 `HookControlTarget` 与 `RuntimeAdapterProvenance`（`crates/agentdash-spi/src/hooks/mod.rs:542`, `crates/agentdash-spi/src/hooks/mod.rs:552`），frame query 已存在（`crates/agentdash-spi/src/hooks/mod.rs:576`），`FrameHookRuntime` 会把 session-shaped query 转成 frame target（`crates/agentdash-application/src/workflow/frame_hook_runtime.rs:155`, `crates/agentdash-application/src/workflow/frame_hook_runtime.rs:272`, `crates/agentdash-application/src/workflow/frame_hook_runtime.rs:316`）。`SessionHookService::ensure_hook_runtime_for_target` / `get_hook_runtime_for_target` 会校验 hook runtime 的 session 与 frame target（`crates/agentdash-application/src/session/hooks_service.rs:32`, `crates/agentdash-application/src/session/hooks_service.rs:52`, `crates/agentdash-application/src/session/hooks_service.rs:176`），workflow/canvas capability caller 已迁到 target-aware 入口（`crates/agentdash-application/src/workflow/agent_executor.rs:357`, `crates/agentdash-application/src/canvas/tools.rs:612`）。但 SPI/session facade 仍保留 `SessionHookSnapshotQuery`/`SessionHookRefreshQuery`/`HookEvaluationQuery`（`crates/agentdash-spi/src/hooks/mod.rs:612`, `crates/agentdash-spi/src/hooks/mod.rs:718`），production provider 的 frame evaluate 仍回落到 session-shaped `HookEvaluationQuery`（`crates/agentdash-application/src/hooks/provider.rs:281`），hub lazy rebuild 入口仍 `ensure_hook_runtime(session_id)`（`crates/agentdash-application/src/session/hub/hook_dispatch.rs:178`），companion parent hook 仍 session-first。 | 是。hook control target 已建好，workflow/canvas capability caller 也会校验 target；剩余耦合在 hook SPI/session facade、hub lazy rebuild 与 companion parent hook。 |
| Phase 4: capability live update 以 AgentFrame target 为 primary target | partial | core primitive `replace_current_capability_state(AgentFrameRuntimeTarget, state)` 会校验 delivery session 属于 target frame 并写 AgentFrame revision（`crates/agentdash-application/src/session/hub/tool_builder.rs:101`, `crates/agentdash-application/src/session/hub/tool_builder.rs:118`, `crates/agentdash-application/src/session/hub/tool_builder.rs:154`）。`StepActivation::apply_to_frame_runtime_target` 已要求调用方传入 `AgentFrameRuntimeTarget` 与 base surface（`crates/agentdash-application/src/workflow/step_activation.rs:290`, `crates/agentdash-application/src/workflow/step_activation.rs:308`），workflow live/pending transition 已通过 `resolve_runtime_session_target` 取得显式 target（`crates/agentdash-application/src/workflow/agent_executor.rs:392`, `crates/agentdash-application/src/workflow/agent_executor.rs:480`）。canvas capability sync 已拆为 runtime-delivery adapter 与 target-first apply helper（`crates/agentdash-application/src/canvas/tools.rs:585`, `crates/agentdash-application/src/canvas/tools.rs:622`），并通过 `get_hook_runtime_for_target` 校验 hook runtime target（`crates/agentdash-application/src/canvas/tools.rs:612`）。但 ContinueRoot 仍以 root runtime session 作为 reuse/delivery policy 输入，companion parent notification 仍以 parent session 为入口。 | 是。StepActivation 与 canvas apply helper 已停止自行反查 frame，hook runtime 也会校验 target；剩余耦合在上层 policy/notification 入口，runtime session 仍可作为 control policy 输入。 |
| Phase 4: RuntimeSession 多引用选择策略 | pass | domain 暴露 `RuntimeSessionSelectionPolicy::{Specific, LaunchPrimary, LatestAttached}`（`crates/agentdash-domain/src/workflow/agent_frame.rs:8`），`select_runtime_session_id` 必须显式传 policy（`crates/agentdash-domain/src/workflow/agent_frame.rs:135`），runtime launch 也从 frame + policy 选择 runtime（`crates/agentdash-application/src/workflow/runtime_launch.rs:1`, `crates/agentdash-application/src/workflow/runtime_launch.rs:89`）。 | 否。选择入口已从“随手拿第一个 session”转为显式策略；剩余风险是 `LatestAttached` 仍是顺序策略，不是业务 owner。 |
| Phase 4: ContinueRoot policy split | fail | `start_continue_root` 仍要求 `root_runtime_session_id`、用它取 executor config、创建 assignment、应用 activity，并返回 `ExecutorRunRef::RuntimeSession`（`crates/agentdash-application/src/workflow/agent_executor.rs:678`, `crates/agentdash-application/src/workflow/agent_executor.rs:688`, `crates/agentdash-application/src/workflow/agent_executor.rs:695`, `crates/agentdash-application/src/workflow/agent_executor.rs:711`）。pending path 也从 root runtime session 解析 target frame 并把同一个 session 作为 delivery（`crates/agentdash-application/src/workflow/agent_executor.rs:471`）。 | 是。ContinueRoot 仍把“复用哪个 agent/frame”和“发给哪个 runtime session”压成一个 root runtime session。 |
| Phase 4: session_id 仅作为 runtime adapter provenance | partial | hook provenance 类型存在（`crates/agentdash-spi/src/hooks/mod.rs:552`），permission query 也已 frame/run primary；但 hook/canvas/ContinueRoot/task cancel/companion parent routing 多处仍把 session_id 当 primary command target（`crates/agentdash-application/src/session/hooks_service.rs:23`, `crates/agentdash-application/src/canvas/tools.rs:585`, `crates/agentdash-application/src/workflow/agent_executor.rs:678`, `crates/agentdash-application/src/task/service.rs:195`, `crates/agentdash-application/src/companion/tools.rs:687`）。 | 是。runtime session provenance 的模型已经出现，但调用层还没有统一收口。 |
| Phase 5: Story root / freeform launch 进入 dispatch taxonomy | partial | project agent/freeform launch 已构造 `AgentLaunchIntent` 并调用 `LifecycleDispatchService::launch_agent`（`crates/agentdash-api/src/routes/project_agents.rs:154`, `crates/agentdash-api/src/routes/project_agents.rs:183`），freeform lifecycle seed 仍使用 `AgentSessionPolicy::ContinueRoot`（`crates/agentdash-application/src/workflow/freeform.rs:89`）。Story routes 当前只读取 `SubjectExecutionView`，未找到 Story root launch write path（`crates/agentdash-api/src/routes/story_runs.rs:30`, `crates/agentdash-api/src/routes/story_runs.rs:57`）。 | 部分是。freeform launch 已进 dispatch，但其 lifecycle 仍带 ContinueRoot policy；Story launch 写路径缺失或不在本次扫描目标路径内。 |
| Phase 5: Task start/continue command target | partial | backend start/continue 已构造 `SubjectExecutionIntent`，subject 为 `task`，通过 `LifecycleDispatchService::execute_subject` 调度（`crates/agentdash-application/src/task/service.rs:118`, `crates/agentdash-application/src/task/service.rs:163`, `crates/agentdash-application/src/task/service.rs:279`），API 返回 run/agent/frame/assignment/subject_execution/trace refs（`crates/agentdash-api/src/routes/task_execution.rs:20`, `crates/agentdash-api/src/routes/task_execution.rs:70`）。frontend 仍 POST 后丢弃 start/continue response 并重新 fetch Task（`packages/app-web/src/services/story.ts:350`, `packages/app-web/src/services/story.ts:358`）。 | 部分是。后端 target 已从 session 迁到 subject execution；前端仍以 Task record 作为命令反馈模型，削弱 SubjectExecution contract。 |
| Phase 5: Task cancel command target | fail | `TurnDispatcher` 仍只有 `cancel_session(session_id)`（`crates/agentdash-application/src/task/service.rs:19`），cancel_task 先找 task execution refs，再取 current frame 的 `LatestAttached` runtime session，最后 cancel session（`crates/agentdash-application/src/task/service.rs:195`, `crates/agentdash-application/src/task/service.rs:207`, `crates/agentdash-application/src/task/service.rs:214`）。API cancel route 仍返回 Task record（`crates/agentdash-api/src/routes/task_execution.rs:109`）。 | 是。取消语义仍是 runtime delivery action，不是 subject execution / assignment / gate 的 lifecycle command。 |
| Phase 5: companion wait/resume/gate/notification 分层 | partial | `LifecycleGate` domain/service 已存在 durable wait/resume 点（`crates/agentdash-domain/src/workflow/lifecycle_gate.rs:5`, `crates/agentdash-application/src/workflow/lifecycle_gate_service.rs:1`），companion request wait 会 open interaction gate（`crates/agentdash-application/src/companion/tools.rs:378`, `crates/agentdash-application/src/companion/tools.rs:567`）。但 human response route 仍以 `/sessions/{id}/...` 注入 session notification（`crates/agentdash-application/src/session/control.rs:66`, `crates/agentdash-application/src/session/control.rs:120`, `crates/agentdash-api/src/routes/sessions.rs:638`），parent result 仍从 child session 反查 parent lineage、再选 parent latest runtime session 注入 notification（`crates/agentdash-application/src/companion/tools.rs:1151`, `crates/agentdash-application/src/companion/tools.rs:1273`, `crates/agentdash-application/src/companion/tools.rs:1363`）。 | 是。gate 已承担持久 wait/resume，但 notification delivery 与 parent hook/control 仍以 runtime session 为中心。 |
| Phase 5: companion parent hook/capability target | fail | parent companion result 通过 `ensure_hook_runtime(&parent_session_id)` 创建 hook runtime 并 evaluate pending action（`crates/agentdash-application/src/companion/tools.rs:687`, `crates/agentdash-application/src/companion/tools.rs:704`），helper `evaluate_subagent_hook` 仍使用 `HookEvaluationQuery { session_id }` 和 `SessionHookRefreshQuery`（`crates/agentdash-application/src/companion/tools.rs:1562`, `crates/agentdash-application/src/companion/tools.rs:1581`）。 | 是。CompanionChannel/Gate/Notification 没有完全分层，parent session 仍是 hook/capability/control 的聚合锚点。 |
| Phase 5: Routine reuse 通过 LifecycleAgentReuseResolver / subject association | fail | 未找到 `LifecycleAgentReuseResolver` 或等价封装。routine reuse intent 仍把 reuse 表达成 `parent_run_id = reuse_run_id` + `RunPolicy::ReuseExisting` + `AgentPolicy::Resume`（`crates/agentdash-application/src/routine/dispatch.rs:55`, `crates/agentdash-application/src/routine/dispatch.rs:64`），executor 通过 entity key 找最近 routine execution 并取 `existing.dispatch_refs.run_id`（`crates/agentdash-application/src/routine/executor.rs:270`, `crates/agentdash-application/src/routine/executor.rs:296`），dispatch service 的 run reuse 只认 `parent_run_id`（`crates/agentdash-application/src/workflow/dispatch_service.rs:452`）。 | 是。reuse policy 仍耦合到 parent run id，缺少按 routine/entity/subject association 解析可复用 agent/frame 的封装。 |
| Phase 5: permission query 中 runtime session 只是 provenance | pass with caveat | Grant domain 以 `effect_frame_id` 为 effect anchor，`source_runtime_session_id` 是 source 字段（`crates/agentdash-domain/src/permission/entity.rs:17`, `crates/agentdash-domain/src/permission/entity.rs:51`）；repository 查询面是 frame/run/escalation-by-frame，没有 source runtime query（`crates/agentdash-domain/src/permission/repository.rs:15`）；API list query 只接受 `effect_frame_id` 或 `run_id`（`crates/agentdash-api/src/routes/permission_grants.rs:104`），contracts query 也只有 `effect_frame_id`/`run_id`/`status`（`crates/agentdash-contracts/src/permission.rs:28`）。Caveat: `apply_frame_effect` 在构建新 frame revision 时会把 `grant.source_runtime_session_id` 再加入 runtime refs（`crates/agentdash-application/src/permission/service.rs:323`）。 | 查询侧否，effect 应用侧部分是。runtime session 不再是 permission query primary target，但 source runtime provenance 被重新写进 frame runtime refs，容易让 provenance 重新参与 delivery ownership。 |

### Code Patterns

1. AgentFrameBuilder 已经成为 frame revision 的主要写入工具，但不是 StepActivation 的唯一 owner。

   - `AgentFrameBuilder` 注释说明它接收 StepActivation/CapabilityResolver/Context 并成为 frame facts source（`crates/agentdash-application/src/workflow/frame_builder.rs:1`）。
   - builder 字段覆盖 procedure/context/capability/VFS/MCP/execution profile/runtime refs/activity scope（`crates/agentdash-application/src/workflow/frame_builder.rs:24`）。
   - `with_capability_state` 同时投影 capability/VFS/MCP surface（`crates/agentdash-application/src/workflow/frame_builder.rs:82`）。
   - `build()` 写入 revision 并 carry forward 未显式替换的 surface（`crates/agentdash-application/src/workflow/frame_builder.rs:149`, `crates/agentdash-application/src/workflow/frame_builder.rs:193`）。
   - 但 `StepActivation` 仍是 workflow 层独立 DTO，`apply_to_frame_runtime_target` 由 `AgentActivityExecutor` 直接调用；activation -> frame transition 还不是 builder-owned stage。

2. Runtime delivery split 的底层结构是正确的，但调用层仍常以 runtime session 开局。

   - `LiveRuntimeContextTransitionInput` 同时要求 `target_frame_id` 和 `delivery_runtime_session_id`（`crates/agentdash-application/src/session/hub/runtime_context_transition.rs:29`）。
   - live transition 用 `AgentFrameRuntimeTarget` 写 capability state（`crates/agentdash-application/src/session/hub/runtime_context_transition.rs:110`）。
   - `resolve_runtime_session_target(session_id)` 当前作为 application adapter lookup 存在，workflow live/pending transition 与 canvas capability sync 已消费显式 `AgentFrameRuntimeTarget`；但 ContinueRoot 仍从 root runtime session 推导 delivery target，runtime session 仍可扩散为 policy 输入。

3. Hook runtime 形成了 frame target/provenance 双模型，但 session facade 尚未收口。

   - `FrameHookRuntime::control_target` 与 `runtime_provenance` 可生成 frame target/provenance（`crates/agentdash-application/src/workflow/frame_hook_runtime.rs:155`）。
   - frame refresh/evaluate adapter 测试证明错误 query.session_id 不会改变 provider 收到的 frame target（`crates/agentdash-application/src/workflow/frame_hook_runtime.rs:731`）。
   - production provider 仍保留 `load_session_snapshot`/`refresh_session_snapshot`，并在 frame evaluate 内调用旧 `evaluate_hook(HookEvaluationQuery { session_id })`（`crates/agentdash-application/src/hooks/provider.rs:281`, `crates/agentdash-application/src/hooks/provider.rs:309`）。

4. Companion gate 已耐久化，notification 和 hook 仍是 session delivery。

   - `LifecycleGate::open/resolve/is_open` 提供 durable gate 状态（`crates/agentdash-domain/src/workflow/lifecycle_gate.rs:30`）。
   - companion wait 可通过 `LifecycleDispatchService::open_interaction_gate` 生成 gate（`crates/agentdash-application/src/companion/tools.rs:378`）。
   - parent result/notification 多处仍通过 parent runtime session 注入 backbone envelope（`crates/agentdash-application/src/companion/tools.rs:721`, `crates/agentdash-application/src/companion/tools.rs:1363`）。

5. Task start/continue 已进入 subject execution，cancel 仍是 session command。

   - start/continue service 注释明确通过 ExecutionIntent dispatch 编排 Task execution（`crates/agentdash-application/src/task/service.rs:29`）。
   - start/continue 都构造 `SubjectExecutionIntent` 并 dispatch（`crates/agentdash-application/src/task/service.rs:118`, `crates/agentdash-application/src/task/service.rs:163`）。
   - cancel 仍选择 latest runtime session 后调用 `cancel_session`（`crates/agentdash-application/src/task/service.rs:214`, `crates/agentdash-api/src/bootstrap/turn_dispatcher.rs:20`）。

6. Routine reuse 已脱离 raw session，但仍绑定 parent_run_id。

   - reuse helper 将 `reuse_run_id` 填入 `parent_run_id`（`crates/agentdash-application/src/routine/dispatch.rs:55`）。
   - dispatch service 的 `resolve_or_create_run` 在 `ReuseExisting` 下只检查 `intent.parent_run_id`（`crates/agentdash-application/src/workflow/dispatch_service.rs:452`）。
   - `AgentPolicy::Reuse|Resume` 只按 run 列出 agent 并取第一个 active agent（`crates/agentdash-application/src/workflow/dispatch_service.rs:529`）。

7. Permission query 模型基本收敛到 frame/run。

   - `PermissionGrantRepository` 只暴露 frame/run/escalation-by-frame 查询（`crates/agentdash-domain/src/permission/repository.rs:15`）。
   - `PermissionGrantService::list_active_by_frame` 与 `find_active_escalation_by_frame` 是 application query surface（`crates/agentdash-application/src/permission/service.rs:223`, `crates/agentdash-application/src/permission/service.rs:235`）。
   - migration `0081_permission_grants_frame_anchor.sql` 将旧 session field 改名为 `source_runtime_session_id` 并加 `effect_frame_id` index（`crates/agentdash-infrastructure/migrations/0081_permission_grants_frame_anchor.sql:1`）。

### Incomplete Points And Coupling Cause

| Incomplete Point | Is Over-Coupling? | Cause |
| --- | --- | --- |
| StepActivation 仍独立于 AgentFrameBuilder | yes | activation 既承载 lifecycle input，又暴露 capability/VFS/MCP/runtime refs 给 assembler、companion projection、executor 使用；没有一个 `AgentFrameSurfaceService` 负责把 activation 变成唯一 frame transition。 |
| Hook snapshot/refresh/evaluate session facade 未收口 | yes | SPI 已有 frame target/provenance，但 provider/service/hub 仍保留 session primary API，导致任何调用者都可绕过 frame target。 |
| Capability live update caller session-first | yes | `replace_current_capability_state` 封装正确，但 target resolution 没有被集中在 runtime adapter 边界；上层工具仍直接以 session 找 frame。 |
| ContinueRoot 仍用 root_runtime_session_id 表达 reuse/delivery | yes | agent/frame reuse policy 与 runtime delivery policy 没有拆成两个封装，root session 同时扮演 executor config、assignment、activity、delivery 的锚点。 |
| Story/freeform launch 未完全摆脱 ContinueRoot | partial | freeform route 已走 dispatch taxonomy，但 freeform lifecycle seed 仍声明 `AgentSessionPolicy::ContinueRoot`；Story launch 写路径未在当前扫描中找到。 |
| Task cancel 仍是 session command | yes | Task execution refs 已可定位 subject/agent/frame/assignment，但取消命令没有 lifecycle command model，只能降到 latest runtime session。 |
| Companion notification 与 parent hook 仍以 session 为 delivery/control | yes | LifecycleGate 已 durable，但 `CompanionChannel`、`LifecycleGate`、`RuntimeNotification` 未形成清晰封装；parent session 仍被用来挂 hook pending action 和 notification。 |
| Routine reuse 依赖 parent_run_id | yes | 缺少按 routine/entity/subject association 解析可复用 lifecycle agent/frame 的 resolver；`RunPolicy::ReuseExisting` 被迫使用 parent_run_id 作为复用载体。 |
| Permission effect apply 把 source runtime 加回 frame refs | partial | query 侧已 frame/run primary，但 effect 应用侧让 provenance 字段重新进入 runtime refs；如果没有显式校验 source runtime 属于 effect frame，会模糊 provenance 与 delivery ownership。 |

### Minimal Structural Fix Order

1. 收敛 AgentFrame surface owner。

   建立或强化 `AgentFrameSurfaceService` 作为 StepActivation -> AgentFrame transition 的唯一封装：输入 lifecycle activity/procedure/context/capability/VFS/MCP/runtime delivery intent，输出 persisted AgentFrame revision 和 runtime delivery command。`StepActivation` 保持在该封装内部，companion skill projection、session assembler、agent executor 只提交 surface intent，不直接改 activation 字段。补一组同源 builder 测试：同一 activation 产出 procedure/context/capability/VFS/MCP/runtime refs/activity scope，并验证 carry-forward 不混入旧 owner。

2. 收口 hook/capability primary target。

   将 application production 调用面改为 `HookControlTarget` 和 `AgentFrameRuntimeTarget`，把 `SessionHookSnapshotQuery`、`SessionHookRefreshQuery`、`HookEvaluationQuery { session_id }` 限在 runtime adapter/test helper 内。`ensure_hook_runtime` 的上层入口应接收 frame/agent/run/assignment target，runtime session 只作为 `RuntimeAdapterProvenance`。capability live update caller 不再自行 `resolve_runtime_session_frame_id`，统一通过 target resolver 封装取得 `AgentFrameRuntimeTarget`。

3. 拆 ContinueRoot 为 agent reuse policy 与 runtime delivery policy。

   用 lifecycle agent/frame/assignment 选择封装表达“继续哪个 root agent/frame”，再用 `RuntimeSessionSelectionPolicy` 表达“投递给哪个 runtime session”。`start_continue_root` 不再以 root runtime session 作为 executor config、assignment、activity 的共同锚点；freeform lifecycle 的 open-ended activity 也改为引用新的 lifecycle-level policy。

4. 引入 Task cancel lifecycle command。

   增加 `CancelSubjectExecutionCommand` 或等价 application service：以 `SubjectExecutionRef`/assignment/frame 为 target，先更新 lifecycle subject execution、assignment/gate/run 状态，再生成 runtime cancel delivery。API 返回 subject execution command result，frontend 使用该 result 更新 workflow/task execution 投影。

5. 切分 CompanionChannel、LifecycleGate、RuntimeNotification。

   `CompanionChannel` 负责 parent/child/gate correlation；`LifecycleGate` 只负责 durable wait/resume truth；`RuntimeNotification` 只接收 frame/assignment/gate target 并在 adapter 层选择 runtime session delivery。parent result hook evaluation 使用 parent `HookControlTarget`，notification 注入不再由 parent session id 决定。

6. 增加 LifecycleAgentReuseResolver 并迁移 routine reuse。

   resolver 输入 routine key、entity key、subject association、reuse strategy，输出可复用 run/agent/frame/assignment 或明确创建新 lifecycle execution。routine dispatch 不再把复用编码成 `parent_run_id`；`RunPolicy::ReuseExisting` 只接受 resolver 产物。

7. 固化 permission provenance 边界。

   permission query 保持 `effect_frame_id`/`run_id` primary。`apply_frame_effect` 对 `source_runtime_session_id` 做 provenance-only 处理：要么校验它已属于 effect frame 的 runtime refs，要么不把它加入新 frame runtime refs。effect frame owner 由 `effect_frame_id` 决定，source runtime 只用于 audit/turn/tool-call trace。

8. 补齐 cross-layer contract checks。

   将 Phase 4/5 的静态 gate 做成 focused checks：production application paths 不出现 session-primary hook/capability query；task cancel 不调用 `cancel_session` 作为业务入口；routine reuse 不写 `parent_run_id`；permission list query 不接受 source runtime session；frontend task start/continue/cancel 使用 generated subject execution response。

### External References

- 未使用外部引用。本次审计仅基于本地 `.trellis/` 文档、spec 与当前代码。

### Related Specs

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/backend/hooks/execution-hook-runtime.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/backend/permission/policy-engine.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回当前无 active task；本次按用户显式给出的 task path 写入 research 文档。
- 修复前未找到 `LifecycleAgentReuseResolver` 或等价命名封装；后续 post-fix update 已记录 Routine reuse slice。
- 未找到 Story root launch 的写路径；当前 `story_runs` API 是 `SubjectExecutionView` 读取投影。
- 本审计未运行测试，也未修改任何代码/规格文件；结论来自静态阅读与 targeted search。

## Post-Fix Update: 2026-06-02 Routine Reuse

本文件前文记录的是修复前静态审计结果；随后已完成 Routine Reuse structural slice：

- `LifecycleAgentReuseResolver` 已新增为 Routine reuse 查询边界，使用 routine execution 历史、entity key、dispatch refs、run/agent/frame/assignment 和 `LifecycleSubjectAssociation` 验证可复用 lifecycle anchor。
- `DispatchStrategy::Reuse` 无有效 active agent anchor 时返回 conflict；`DispatchStrategy::PerEntity` 首次 entity 触发显式创建新 anchor，已有同 entity target 时显式复用 `parent_run_id + parent_agent_id`。
- `LifecycleDispatchService` 对 explicit `parent_agent_id` 校验 run/project/status，同一 run 多 active agent 时只复用指定 agent；`RunPolicy::ReuseExisting` / `AppendGraph` 缺 `parent_run_id` 会拒绝。
- 因此表格中的「Phase 5: Routine reuse 通过 LifecycleAgentReuseResolver / subject association」已从 fail 更新为 pass；Phase 5 其它 gate 仍保持前文状态。

已验证：

- `cargo test -p agentdash-application routine::reuse_resolver --lib -- --format terse`
- `cargo test -p agentdash-application routine::dispatch --lib -- --format terse`
- `cargo test -p agentdash-application workflow::dispatch_service --lib -- --format terse`
- `cargo check -p agentdash-application`

## Post-Fix Update: 2026-06-02 AgentFrame Surface Projection

本文件前文记录的是修复前静态审计结果；随后已完成 AgentFrameBuilder 同源 surface gate：

- `AgentFrameSurfaceInput` 已新增为 assembly -> AgentFrame revision 的投影边界，session assembly 不再逐列拼写 capability/VFS/MCP/context/execution profile surface。
- `build_lifecycle_activation_surface` 已新增为 lifecycle activation -> frame surface 的归一化 stage，集中合并 base VFS、activation lifecycle VFS、mount directives、MCP servers 与 capability state。
- `lifecycle_activation_surface_outputs_single_coherent_frame_revision` 证明一次 activation surface 能在同一 AgentFrame revision 中同时写入 procedure、context、capability、VFS/MCP、runtime refs 与 graph activity scope。
- 因此表格中的「Phase 4: AgentFrameBuilder 覆盖 StepActivation 同源 surface 测试」已从 partial 更新为 pass；「Phase 4: StepActivation 纳入 AgentFrame surface owner / AgentFrameBuilder」仍保持 fail/partial，因为 StepActivation 独立 DTO、companion projection 与 ContinueRoot 仍未完全收口。

已验证：

- `cargo test -p agentdash-application workflow::frame_builder --lib -- --format terse`
- `cargo check -p agentdash-application`

## Post-Fix Update: 2026-06-02 StepActivation Live Target

本文件前文关于 StepActivation live apply 的修复前证据已过期；随后已完成 workflow 内部 applier 的 target-first 收口：

- `SessionCapabilityService::resolve_runtime_session_target` 把 runtime adapter lookup 包装成 `AgentFrameRuntimeTarget`，调用者不再直接传裸 `target_frame_id`。
- `StepActivation::apply_to_frame_runtime_target` 接收 `AgentFrameRuntimeTarget` 与 base capability surface，函数内部不再解析 runtime session 的 frame，也不再自行读取 current capability state。
- `AgentActivityExecutor` live 与 pending transition 都显式传递 `target_frame_id + delivery_runtime_session_id`。
- 因此表格中的「Phase 4: capability live update 以 AgentFrame target 为 primary target」保持 partial 但证据更新：StepActivation applier 子问题已关闭；canvas direct frame lookup 当时仍未关闭，ContinueRoot root session policy、companion parent notification 仍未关闭。

已验证：

- `cargo test -p agentdash-application workflow::step_activation --lib -- --format terse`
- `cargo test -p agentdash-application workflow::agent_executor --lib -- --format terse`
- `cargo check -p agentdash-application`
- `cargo fmt --all --check`
- `git diff --check`

## Post-Fix Update: 2026-06-02 Canvas Capability Target

canvas capability sync 已完成 direct frame lookup 收口：

- `sync_canvas_mount_capability_state_for_runtime_delivery` 是 runtime delivery adapter，负责在有 base capability state 与 hook runtime 时解析 `AgentFrameRuntimeTarget`。
- `sync_canvas_mount_capability_state` 接收 `AgentFrameRuntimeTarget`、base capability state 与 hook runtime，再调用 `apply_live_vfs_capability_state`；canvas apply helper 不再构造裸 frame id。
- `resolve_runtime_session_frame_id(` 在 application src 只剩 `SessionCapabilityService` 与 hub adapter 内部命中。
- 因此表格中的 capability live update gate 仍保持 partial，但 canvas direct lookup 子问题已关闭；剩余耦合是 hook SPI/session facade、ContinueRoot root runtime policy 与 companion parent notification。

已验证：

- `cargo test -p agentdash-application canvas::tools::tests::present_canvas_updates_meta_capability_skill_and_events --lib -- --format terse`
- `cargo check -p agentdash-application`

## Post-Fix Update: 2026-06-02 Hook Runtime Target-Aware Caller

workflow/canvas capability caller 已不再裸用 session-first hook getter：

- `SessionHookService::ensure_hook_runtime_for_target` / `get_hook_runtime_for_target` 以 `AgentFrameRuntimeTarget` 为输入，在 adapter lookup 后校验 hook runtime 的 `session_id` 与 `control_target.frame_id`。
- `AgentActivityExecutor` 先解析 target，再通过 target-aware ensure 获取 hook runtime。
- canvas capability sync 先解析 target，再通过 target-aware getter 获取 hook runtime。
- 因此表格中的 hook/capability gate 仍保持 partial，但 workflow/canvas caller 子问题已关闭；剩余耦合集中在 SPI/session facade、hub lazy rebuild、companion parent hook 与 ContinueRoot policy。

已验证：

- `cargo test -p agentdash-application workflow::agent_executor --lib -- --format terse`
- `cargo test -p agentdash-application canvas::tools::tests::present_canvas_updates_meta_capability_skill_and_events --lib -- --format terse`
- `cargo check -p agentdash-application`
