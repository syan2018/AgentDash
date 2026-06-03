# Research: business modules gap

- Query: 任务切片 B，扫描 companion / task / story / canvas / vfs / mcp / routine / permission / agent repository 与 hook preset/scripts 中 Task、Story、Companion、Subagent、Activity、LifecycleRunLink/Subject 相关逻辑，评估迁移到 Lifecycle / Actor / ActorFrame / SubjectRef / LifecycleSubjectAssociation / Workflow / Activity 目标谓词的差距。
- Scope: internal
- Date: 2026-06-01

## Findings

### Related Specs / Target Predicate

- 当前任务目标文档把 scattered runtime facts 收束到 `LifecycleRun -> Actor -> ActorFrame -> RuntimeSession`，并要求 `Task` 只作为 data/view object，`ActivityAttemptState` 只作为 execution record（`.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/agent-operation-predicate-comparison.md:150`, `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/agent-operation-predicate-comparison.md:221`, `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/agent-operation-predicate-comparison.md:341`）。
- `SubjectRef` 的目标语义是 Story / Task / Project / External 等业务对象引用，且 Task entity 本身没有 runtime 语义（`.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/agent-operation-predicates.md:23`, `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/agent-operation-predicates.md:105`）。
- 现有 spec 仍以 `LifecycleRunLink` 为 run-level 关联层，支持 Story / Project / RoutineExecution / Task / LifecycleRun / External 与 Source / Subject / ProjectionTarget / ControlScope / SpawnedBy 角色（`.trellis/spec/backend/story-task-runtime.md:10`, `.trellis/spec/backend/story-task-runtime.md:42`, `.trellis/spec/backend/workflow/lifecycle-run-link.md:23`, `.trellis/spec/backend/workflow/lifecycle-run-link.md:34`）。
- Activity lifecycle 是 workflow 运行唯一模型，Task 启动/续跑应统一走 Activity activation，Task view 从 Activity attempts 投影（`.trellis/spec/backend/workflow/activity-lifecycle.md:3`, `.trellis/spec/backend/workflow/activity-lifecycle.md:14`, `.trellis/spec/backend/workflow/activity-lifecycle.md:192`）。

### Files Found

- `crates/agentdash-domain/src/task/entity.rs` — Task 领域实体与投影边界。
- `crates/agentdash-domain/src/story/entity.rs` / `crates/agentdash-application/src/story/management.rs` — Story aggregate 内 Task CRUD 与 spec-only mutation。
- `crates/agentdash-application/src/task/*` — Task service、Task session bridge、Task view projector、context builder、hook effect executor。
- `crates/agentdash-application/src/workflow/*` — Activity lifecycle run、scheduler、orchestrator、session association、step activation、agent executor、run link service。
- `crates/agentdash-application/src/companion/tools.rs` / `crates/agentdash-application/src/session/companion_wait.rs` — Companion/subagent dispatch、parent-child session context、wait registry。
- `crates/agentdash-application/src/session/construction_*` / `crates/agentdash-application/src/session/assembler.rs` — launch source、prompt/VFS/capability/context composition。
- `crates/agentdash-application/src/canvas/*` / `crates/agentdash-application/src/vfs/*` — Canvas/VFS runtime surface 与 lifecycle/routine mounts。
- `crates/agentdash-application/src/mcp_preset/*` / `crates/agentdash-application/src/capability/resolver.rs` — MCP preset 解析与 capability owner scope。
- `crates/agentdash-application/src/routine/*` / `crates/agentdash-domain/src/routine/*` — Routine execution、session strategy、routine VFS。
- `crates/agentdash-application/src/permission/*` / `crates/agentdash-domain/src/permission/*` — PermissionGrant、scope escalation、runtime capability transition。
- `crates/agentdash-application/src/hooks/*` / `crates/agentdash-application/scripts/hook-presets/*.rhai` — hook snapshot/provider、owner defaults、subagent/lifecycle/task presets。
- `crates/agentdash-domain/src/agent/*` — ProjectAgent repository/default lifecycle 配置。

### Task / Story

Current responsibility:
- Domain 层已经把 Task 定义为 Story aggregate 下的用户工作项和只读 projection 容器；注释明确 Task 不持有内部 `session_id` 或 executor resume id，`status` / `artifacts` 是 LifecycleStepState/Activity attempt 的只读投影（`crates/agentdash-domain/src/task/entity.rs:12`, `crates/agentdash-domain/src/task/entity.rs:17`, `crates/agentdash-domain/src/task/entity.rs:38`, `crates/agentdash-domain/src/task/entity.rs:42`）。
- Task spec 仍暴露 `lifecycle_step_key` 与 `agent_binding`，作为 Story aggregate 内可变 spec 字段；Story 更新通过 `TaskSpecMut` 限制只能改 spec，不能改投影字段（`crates/agentdash-domain/src/task/entity.rs:34`, `crates/agentdash-domain/src/task/entity.rs:147`, `crates/agentdash-domain/src/task/entity.rs:154`, `crates/agentdash-domain/src/story/entity.rs:77`）。
- Story management 创建/更新 Task 时写入 `lifecycle_step_key` 和 `agent_binding`，说明 Story/Task 是业务配置与 view 聚合，不是运行时主体（`crates/agentdash-application/src/story/management.rs:36`, `crates/agentdash-application/src/story/management.rs:198`, `crates/agentdash-application/src/story/management.rs:203`, `crates/agentdash-application/src/story/management.rs:218`）。

Current lifecycle/session dependencies:
- `StoryStepActivationService` 仍提供 `start_task` / `continue_task` / `cancel_task`，直接创建或复用 task execution session（`crates/agentdash-application/src/task/service.rs:43`, `crates/agentdash-application/src/task/service.rs:54`, `crates/agentdash-application/src/task/service.rs:65`, `crates/agentdash-application/src/task/service.rs:76`）。
- 直接 Task execution session 被声明为不挂 `lifecycle_activity:*` binding，assembly 需要容忍没有 active workflow（`crates/agentdash-application/src/task/service.rs:84`）。
- start path 调用 `gw_create_task_session`，随后 `bind_session_to_owner(&session_id, "task", task.id, "execution")`；但该 bind 目前只有 `mark_owner_bootstrap_pending` 和 TODO，没有写入 durable run/subject 关联（`crates/agentdash-application/src/task/service.rs:113`, `crates/agentdash-application/src/task/service.rs:115`, `crates/agentdash-application/src/task/service.rs:323`, `crates/agentdash-application/src/task/service.rs:325`）。
- continue/cancel 通过 `find_task_execution_session_id` 从 `LifecycleRunLink(Task)` 找 run 再取 `LifecycleRun.session_id`，说明读取路径已经期待 run link，但创建路径没有对应持久化事实（`crates/agentdash-application/src/task/mod.rs:19`, `crates/agentdash-application/src/task/mod.rs:29`, `crates/agentdash-application/src/task/mod.rs:35`, `crates/agentdash-application/src/task/service.rs:303`）。
- `task/gateway/session_bridge.rs` 注释说 Task session 归属由 `LifecycleRunLink` 管理，但实现只创建 `SessionMeta`（`crates/agentdash-application/src/task/gateway/session_bridge.rs:1`, `crates/agentdash-application/src/task/gateway/session_bridge.rs:4`, `crates/agentdash-application/src/task/gateway/session_bridge.rs:13`）。

Is Task treated as runtime subject:
- Domain: 否。Task 是 Story child data/view object，状态由 `apply_projection(ActivityAttemptStatus)` 写入（`crates/agentdash-domain/src/task/entity.rs:89`, `crates/agentdash-domain/src/story/entity.rs:140`, `crates/agentdash-domain/src/story/entity.rs:148`）。
- Application: 是，残留在 `StoryStepActivationService` 和 task session bridge。它把 Task 当作 session owner/launch subject，并用 `TaskLaunchSource` 进入 session construction（`crates/agentdash-application/src/task/service.rs:140`, `crates/agentdash-application/src/session/launch/command.rs:177`）。

Migration target:
- Task start/continue 应改为以 `SubjectRef(kind=Task, id=task_id)` 作为 Activity payload/subject association 输入，由 Lifecycle 创建/选择 Actor；Task entity 不再承担 session owner 或 runtime subject。
- `Task.lifecycle_step_key` 语义应从“运行绑定”收窄为“view projection selector / authoring hint”，最终由 `LifecycleSubjectAssociation(anchor=activity|actor, subject_ref=Task, role=ProjectionTarget|Subject)` 替代 run/session 查找。
- Task status/artifacts 继续由 ActivityAttemptState / Artifact projection 写回 Story aggregate；`task:set_status` hook effect 应变为投影请求或 Activity outcome 证据，不应成为 Task runtime 状态源。

### Task Projection / Context / Effects

Current responsibility:
- `task/view_projector.rs` 明确以 `LifecycleRun.activity_state` 为真相源，把 attempt status 投影到 `Story.tasks[i].status`；匹配条件仍是 `task.lifecycle_step_key == state.activity_key`（`crates/agentdash-application/src/task/view_projector.rs:1`, `crates/agentdash-application/src/task/view_projector.rs:7`, `crates/agentdash-application/src/task/view_projector.rs:95`, `crates/agentdash-application/src/task/view_projector.rs:104`）。
- projector 通过 `LifecycleRunLink(subject_kind=Story)` 找 Story，再遍历 task（`crates/agentdash-application/src/task/view_projector.rs:10`, `crates/agentdash-application/src/task/view_projector.rs:77`, `crates/agentdash-application/src/task/view_projector.rs:85`）。
- `lifecycle_task_projection_states` 只读取 `run.activity_state.attempts`，这是 ActivityAttemptState 作为执行证据的正确边界（`crates/agentdash-application/src/task/view_projector.rs:242`, `crates/agentdash-application/src/task/view_projector.rs:245`）。
- 启动 reconciliation 会把没有 active run 覆盖的 Running task 标为 Failed，这是从 view 清理角度出发的派生行为，不应升级为 Task runtime authority（`crates/agentdash-application/src/task/view_projector.rs:162`, `crates/agentdash-application/src/task/view_projector.rs:173`, `crates/agentdash-application/src/task/view_projector.rs:174`）。
- Task context builder 会通过 Task links 找 active workflow projection，并构造 `CapabilityScopeCtx::Task`（`crates/agentdash-application/src/task/context_builder.rs:75`, `crates/agentdash-application/src/task/context_builder.rs:95`, `crates/agentdash-application/src/task/context_builder.rs:184`）。
- `TaskHookEffectExecutor` 只支持 `task:set_status` / `task:clear_binding`，`task:clear_binding` 已是 no-op，`task:set_status` 最终只追加 `TaskUpdated` state change request（`crates/agentdash-application/src/task/gateway/effect_executor.rs:65`, `crates/agentdash-application/src/task/gateway/effect_executor.rs:105`, `crates/agentdash-application/src/task/gateway/effect_executor.rs:107`, `crates/agentdash-application/src/task/gateway/repo_ops.rs:55`, `crates/agentdash-application/src/task/gateway/repo_ops.rs:75`）。

Migration target:
- Projection source 应改为 `SubjectRef(Task)` + Actor assignment + ActivityAttemptState，而不是 `Task.lifecycle_step_key` 直接匹配。
- `CapabilityScopeCtx::Task` 可保留为 capability projection input，但应由 Actor/SubjectAssociation 推导，而不是 whole-run link 或 direct task session 推导。
- Task effects 应进入 Lifecycle/Activity event 或 ActorRevision，再由 projector 写 Task view。

### Workflow / Lifecycle / ActivityAttemptState

Current responsibility:
- `ActivityLifecycleRunService::start_run` 仍以 `session_id` 为 run 唯一性入口，创建 `LifecycleRun::new_activity(project_id, definition.id, Some(session_id), state)`（`crates/agentdash-application/src/workflow/activity_run.rs:22`, `crates/agentdash-application/src/workflow/activity_run.rs:26`, `crates/agentdash-application/src/workflow/activity_run.rs:48`, `crates/agentdash-application/src/workflow/activity_run.rs:67`）。
- Scheduler 以 `ActivityExecutorLauncher` claim ready attempts，启动后记录 `ExecutorRunRef`，这是 Activity execution record 层（`crates/agentdash-application/src/workflow/scheduler.rs:68`, `crates/agentdash-application/src/workflow/scheduler.rs:85`, `crates/agentdash-application/src/workflow/scheduler.rs:199`）。
- Orchestrator 在 session terminal 或 `complete_lifecycle_node` tool path 上解析 activity session association，生成 `ActivityEvent` 并继续 launch ready attempts（`crates/agentdash-application/src/workflow/orchestrator.rs:132`, `crates/agentdash-application/src/workflow/orchestrator.rs:137`, `crates/agentdash-application/src/workflow/orchestrator.rs:178`, `crates/agentdash-application/src/workflow/orchestrator.rs:244`, `crates/agentdash-application/src/workflow/orchestrator.rs:287`）。
- Agent executor 用 `root_session_id` 启动 SpawnChild / ContinueRoot / Function；SpawnChild 创建 child session，ContinueRoot 返回 root session as executor_run（`crates/agentdash-application/src/workflow/agent_executor.rs:36`, `crates/agentdash-application/src/workflow/agent_executor.rs:448`, `crates/agentdash-application/src/workflow/agent_executor.rs:457`, `crates/agentdash-application/src/workflow/agent_executor.rs:485`, `crates/agentdash-application/src/workflow/agent_executor.rs:489`, `crates/agentdash-application/src/workflow/agent_executor.rs:527`）。
- `StepActivationInput` 聚合 owner_ctx、active_activity、workflow、run_id、mcp/preset/companion/caps，并可 `apply_to_running_session` 更新 live runtime context（`crates/agentdash-application/src/workflow/step_activation.rs:44`, `crates/agentdash-application/src/workflow/step_activation.rs:46`, `crates/agentdash-application/src/workflow/step_activation.rs:122`, `crates/agentdash-application/src/workflow/step_activation.rs:283`, `crates/agentdash-application/src/workflow/step_activation.rs:305`）。

ActivityAttemptState boundary:
- Domain 中 `ActivityAttemptState` 只有 `activity_key`、`attempt`、`status`、`executor_run`、timestamps、summary；`ExecutorRunRef` 只表达 AgentSession / FunctionRun / HumanDecision 的执行引用（`crates/agentdash-domain/src/workflow/value_objects/run_state.rs:24`, `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:25`, `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:27`, `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:86`, `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:87`）。
- 结论：`ActivityAttemptState` 应保留现名和证据职责，不应成为 subject association anchor 或 Actor state。目标应新增 Actor assignment：Actor -> Activity / ActivityAttemptState，使 attempt 可追溯到 RuntimeSession。

Migration target:
- `LifecycleRun.session_id` 从 root RuntimeSession shortcut 迁到 Actor runtime ref；LifecycleRun 保留生命周期追踪/控制面，Actor 承担 session wrapper。
- `StepActivation` 的产物（capability/context/VFS/MCP/procedure/kickoff）应落为 `ActorFrame` revision，而不是只写入 SessionMeta/Hook runtime。
- `WorkflowDefinition` 当前更像单 Activity 的 procedure/contract；目标 `Workflow` 应表达 Lifecycle 下单个 Activity 演化的可执行图配置，Agent executor 使用 `ActivityProcedure / ActorProcedure`。

### LifecycleRunLink / Subject Association

Current responsibility:
- `LifecycleRunLink` 是 run-level `(run_id, subject_kind, subject_id, role)`，角色已覆盖 Source / Subject / ProjectionTarget / ControlScope / SpawnedBy（`crates/agentdash-domain/src/workflow/run_link.rs:13`, `crates/agentdash-domain/src/workflow/run_link.rs:50`, `crates/agentdash-domain/src/workflow/run_link.rs:86`, `crates/agentdash-domain/src/workflow/run_link.rs:89`, `crates/agentdash-domain/src/workflow/run_link.rs:92`, `crates/agentdash-domain/src/workflow/run_link.rs:94`, `crates/agentdash-domain/src/workflow/run_link.rs:96`）。
- `LifecycleRunLinkService` 提供 attach/list subject/run 查询，注释明确覆盖 Story / RoutineExecution / Task（`crates/agentdash-application/src/workflow/run_link_service.rs:15`, `crates/agentdash-application/src/workflow/run_link_service.rs:36`, `crates/agentdash-application/src/workflow/run_link_service.rs:45`, `crates/agentdash-application/src/workflow/run_link_service.rs:66`, `crates/agentdash-application/src/workflow/run_link_service.rs:86`）。
- `SessionRunContextResolver` 从 session 找 runs，再从 run links 选择 Task 优先于 Story，role rank 为 Subject > ProjectionTarget > ControlScope > Source > SpawnedBy（`crates/agentdash-application/src/workflow/session_run_context_resolver.rs:32`, `crates/agentdash-application/src/workflow/session_run_context_resolver.rs:47`, `crates/agentdash-application/src/workflow/session_run_context_resolver.rs:65`, `crates/agentdash-application/src/workflow/session_run_context_resolver.rs:69`, `crates/agentdash-application/src/workflow/session_run_context_resolver.rs:97`）。

Migration target:
- `LifecycleRunLink` 应演化为 `LifecycleSubjectAssociation`，保留 kind/id/role 查询语义，但 anchor 从 whole-run 扩展为 run / actor / activity。
- `SubjectRef` 应成为类型化引用层，替换散落的 `RunLinkSubjectKind + Uuid` 参数、Task id 直接传递、Companion session context 里的 parent/child id 语义。
- 已有 role rank 可复用，但 context scope 的选择应基于 Actor + association，而不是 run-level link 全局优先级。

### Companion / Subagent

Current responsibility:
- `CompanionRequestTool::execute_sub_request` 在当前 session/hook runtime 中派发 subagent；它读取当前 `session_id`、解析 dispatch plan，执行 BeforeSubagentDispatch / AfterSubagentDispatch / CompanionResult hook（`crates/agentdash-application/src/companion/tools.rs:181`, `crates/agentdash-application/src/companion/tools.rs:258`, `crates/agentdash-application/src/companion/tools.rs:282`, `crates/agentdash-application/src/companion/tools.rs:437`）。
- Dispatch plan 以 `parent_session_id` / `parent_turn_id` / `dispatch_id` 为主追溯字段（`crates/agentdash-application/src/companion/tools.rs:295`, `crates/agentdash-application/src/companion/tools.rs:296`, `crates/agentdash-application/src/companion/tools.rs:1855`, `crates/agentdash-application/src/companion/tools.rs:1858`）。
- Child session 的 `SessionMeta.companion_context` 保存 dispatch_id、parent_session_id、parent_turn_id、slice_mode、adoption_mode、request_type、agent_name 等（`crates/agentdash-application/src/companion/tools.rs:331`, `crates/agentdash-application/src/companion/tools.rs:333`, `crates/agentdash-application/src/companion/tools.rs:355`）。
- `setup_companion_workflow` 会为 companion workflow overlay 创建 `LifecycleRun::new_activity(... Some(target_binding.session_id) ...)`，但没有创建 RunLink/Subject association；同文件已有 TODO 要把 companion session lookup 迁到 LifecycleRunLink query（`crates/agentdash-application/src/companion/tools.rs:866`, `crates/agentdash-application/src/companion/tools.rs:903`, `crates/agentdash-application/src/companion/tools.rs:1030`）。
- `CompanionWaitRegistry` 是内存 HashMap，以 `dispatch_id -> session_id/turn_id/request_type/sender` 管理 wait=true 结果回流；这属于 live gate/wait state，当前不 durable（`crates/agentdash-application/src/session/companion_wait.rs:23`, `crates/agentdash-application/src/session/companion_wait.rs:28`, `crates/agentdash-application/src/session/companion_wait.rs:48`, `crates/agentdash-application/src/session/companion_wait.rs:55`）。
- Companion respond 通过当前 session 的 `companion_context` 找 parent session，构造 CompanionResult hook payload，并可 auto-resume parent session（`crates/agentdash-application/src/companion/tools.rs:1316`, `crates/agentdash-application/src/companion/tools.rs:1335`, `crates/agentdash-application/src/companion/tools.rs:1386`, `crates/agentdash-application/src/companion/tools.rs:1391`, `crates/agentdash-application/src/companion/tools.rs:1473`, `crates/agentdash-application/src/companion/tools.rs:1508`）。

Is StoryAgent creating TaskAgent/Companion traceable:
- Story/Project agent defaults are configuration sources, not runtime actors. `ProjectAgent` has `default_lifecycle_key`, `is_default_for_story`, `is_default_for_task`（`crates/agentdash-domain/src/agent/entity.rs:13`, `crates/agentdash-domain/src/agent/entity.rs:27`, `crates/agentdash-domain/src/agent/entity.rs:30`, `crates/agentdash-domain/src/agent/entity.rs:33`）。
- Session assembler resolves owner workflow tool directives by selecting project agent or default story agent, then following `default_lifecycle_key`（`crates/agentdash-application/src/session/assembler.rs:1661`, `crates/agentdash-application/src/session/assembler.rs:1677`, `crates/agentdash-application/src/session/assembler.rs:1684`, `crates/agentdash-application/src/session/assembler.rs:1688`, `crates/agentdash-application/src/session/assembler.rs:1692`）。
- Companion parent-child trace is currently in SessionMeta and HookTrace, not in Lifecycle/Actor/SubjectAssociation. There is no explicit “StoryAgent spawned TaskAgent/Companion Actor” durable fact.

Migration target:
- Companion/subagent should become Actor under Lifecycle: parent Actor spawns child Actor, child ActorFrame captures inherited injections/constraints/VFS/MCP slice, Gate captures wait=true blocking semantics.
- Parent/child/session ids in `CompanionSessionContext` should become Actor lineage plus `LifecycleSubjectAssociation(role=SpawnedBy|Source)`; `dispatch_id` can become ActorRevision/Gate correlation id.
- Companion workflow overlay must create Actor + ActorFrame and subject associations, not only `LifecycleRun(session_id=child)`.

### Session Construction / ActorFrame Facts

Current responsibility:
- `LaunchCommand` carries Task/Routine/Companion hints while source enum remains session-launch oriented (`CompanionDispatch`, `CompanionParentResume`, `RoutineExecutor`, `TaskService`, `WorkflowOrchestrator`) (`crates/agentdash-application/src/session/launch/command.rs:100`, `crates/agentdash-application/src/session/launch/command.rs:104`, `crates/agentdash-application/src/session/launch/command.rs:144`, `crates/agentdash-application/src/session/launch/command.rs:162`, `crates/agentdash-application/src/session/launch/command.rs:177`).
- `CompanionLaunchWorkflowSource` carries run/lifecycle/active_activity/workflow for companion prompt construction, while `CompanionLaunchSource` still anchors on `parent_session_id`（`crates/agentdash-application/src/session/construction_provider.rs:47`, `crates/agentdash-application/src/session/construction_provider.rs:48`, `crates/agentdash-application/src/session/construction_provider.rs:55`, `crates/agentdash-application/src/session/construction_provider.rs:56`）。
- Routine launch source carries routine_id/execution_id/trigger_source/entity_key, then session construction appends routine VFS and routine-memory skill（`crates/agentdash-application/src/session/construction_provider.rs:39`, `crates/agentdash-application/src/session/construction_provider.rs:40`, `crates/agentdash-application/src/session/construction_use_case.rs:247`, `crates/agentdash-application/src/session/construction_use_case.rs:453`, `crates/agentdash-application/src/session/construction_use_case.rs:464`, `crates/agentdash-application/src/session/construction_use_case.rs:480`）。
- Story step/session assembler resolves context sources from Story and `task.agent_binding.context_sources`, then computes capability via `CapabilityScopeCtx::Task`（`crates/agentdash-application/src/session/assembler.rs:891`, `crates/agentdash-application/src/session/assembler.rs:920`, `crates/agentdash-application/src/session/assembler.rs:987`, `crates/agentdash-application/src/session/assembler.rs:990`）。

Migration target:
- `SessionConstructionProviderInput`/`LaunchCommand` should become Actor creation/resume commands. Task/Routine/Companion hints become SubjectRef + procedure + frame contribution sources.
- Prompt/VFS/MCP/capability/context bundle assembly should produce `ActorFrame` facts, then runtime session launch consumes that frame.

### Hooks / Presets / Scripts

Current responsibility:
- Hook provider builds `SessionHookSnapshot` by resolving active workflow from `session_id`, owner context from run links, and metadata including active workflow lifecycle/run/activity/status/ports（`crates/agentdash-application/src/hooks/provider.rs:94`, `crates/agentdash-application/src/hooks/provider.rs:99`, `crates/agentdash-application/src/hooks/provider.rs:121`, `crates/agentdash-application/src/hooks/provider.rs:137`, `crates/agentdash-application/src/hooks/provider.rs:185`, `crates/agentdash-application/src/hooks/provider.rs:189`, `crates/agentdash-application/src/hooks/provider.rs:191`, `crates/agentdash-application/src/hooks/provider.rs:208`）。
- Workflow snapshot builder resolves active workflow by `session_id`（`crates/agentdash-application/src/hooks/workflow_snapshot.rs:46`, `crates/agentdash-application/src/hooks/workflow_snapshot.rs:49`, `crates/agentdash-application/src/hooks/workflow_snapshot.rs:51`）。
- Owner resolver uses LifecycleRunLink to derive SessionRunContext（`crates/agentdash-application/src/hooks/owner_resolver.rs:11`, `crates/agentdash-application/src/hooks/owner_resolver.rs:48`）。
- Preset registry includes subagent context inheritance/result recording, CompanionResult channel, lifecycle advance gates, port output gate, task terminal status（`crates/agentdash-application/src/hooks/presets.rs:45`, `crates/agentdash-application/src/hooks/presets.rs:54`, `crates/agentdash-application/src/hooks/presets.rs:63`, `crates/agentdash-application/src/hooks/presets.rs:111`, `crates/agentdash-application/src/hooks/presets.rs:120`, `crates/agentdash-application/src/hooks/presets.rs:129`）。
- Task owner default injects `task_session_terminal` only for `CapabilityScope::Task` with task_id（`crates/agentdash-application/src/hooks/rules/owner_defaults/task_owner_session_terminal.rs:12`, `crates/agentdash-application/src/hooks/rules/owner_defaults/task_owner_session_terminal.rs:27`）。
- Rhai scripts operate on `ctx.workflow`, `ctx.snapshot.injections`, subagent payload, `companion_session_id`, `dispatch_id`, and task terminal payload; they emit gate decisions, injections, notifications, pending actions, and task effects rather than durable runtime facts（`crates/agentdash-application/scripts/hook-presets/port_output_gate.rhai:12`, `crates/agentdash-application/scripts/hook-presets/subagent_inherit_context.rhai:8`, `crates/agentdash-application/scripts/hook-presets/companion_result_channel.rhai:15`, `crates/agentdash-application/scripts/hook-presets/companion_result_channel.rhai:47`, `crates/agentdash-application/scripts/hook-presets/task_session_terminal.rhai:9`, `crates/agentdash-application/scripts/hook-presets/task_session_terminal.rhai:19`）。

Migration target:
- Hook snapshot should be ActorFrame snapshot: active procedure, subject associations, capabilities, VFS/MCP/context, gate state, runtime refs. `session_id` remains runtime backing, not primary lookup key.
- Preset outputs should map to ActorRevision/Gate/FrameContribution/ActivityEvent. `task_session_terminal` should no longer imply Task runtime; it should request Task view projection from Actor/Activity terminal evidence.

### Canvas / VFS / MCP Runtime Surface

Canvas:
- `Canvas` is project-level runnable frontend asset with `project_id`, `mount_id`, files, bindings; it is not a lifecycle/session subject by itself（`crates/agentdash-domain/src/canvas/entity.rs:7`, `crates/agentdash-domain/src/canvas/entity.rs:9`, `crates/agentdash-domain/src/canvas/entity.rs:11`）。
- Canvas visibility is session-local today: only mount ids recorded in the session are appended to VFS（`crates/agentdash-application/src/canvas/visibility.rs:14`, `crates/agentdash-application/src/canvas/visibility.rs:18`, `crates/agentdash-application/src/canvas/visibility.rs:30`）。
- Canvas tools are injected by `ToolCluster::Canvas`, derive project_id from execution context, and `canvas_start` / `present_canvas` update current session visible mounts and inject `canvas_presented` notification（`crates/agentdash-application/src/vfs/tools/provider.rs:265`, `crates/agentdash-application/src/vfs/tools/provider.rs:267`, `crates/agentdash-application/src/vfs/tools/provider.rs:283`, `crates/agentdash-application/src/vfs/tools/provider.rs:306`, `crates/agentdash-application/src/canvas/tools.rs:552`, `crates/agentdash-application/src/canvas/tools.rs:562`, `crates/agentdash-application/src/canvas/tools.rs:573`, `crates/agentdash-application/src/canvas/tools.rs:484`）。
- Canvas runtime snapshot still uses `session_id: Option<String>` and `ResolvedVfsSurfaceSource::SessionRuntime` for resource surface refs（`crates/agentdash-application/src/canvas/runtime.rs:12`, `crates/agentdash-application/src/canvas/runtime.rs:14`, `crates/agentdash-application/src/canvas/runtime.rs:68`, `crates/agentdash-application/src/canvas/runtime.rs:139`, `crates/agentdash-application/src/canvas/runtime.rs:141`）。

VFS:
- `ResolvedVfsSurfaceSource` already distinguishes ProjectPreview / StoryPreview / TaskPreview / SessionRuntime; SessionRuntime is still keyed by raw session id（`crates/agentdash-application/src/vfs/surface.rs:20`, `crates/agentdash-application/src/vfs/surface.rs:24`, `crates/agentdash-application/src/vfs/surface.rs:28`, `crates/agentdash-application/src/vfs/surface.rs:32`, `crates/agentdash-application/src/vfs/surface.rs:61`, `crates/agentdash-application/src/vfs/surface.rs:122`）。
- Tool provider injects workflow, collaboration, canvas tools from current capability_state and hook_session session id（`crates/agentdash-application/src/vfs/tools/provider.rs:133`, `crates/agentdash-application/src/vfs/tools/provider.rs:140`, `crates/agentdash-application/src/vfs/tools/provider.rs:223`, `crates/agentdash-application/src/vfs/tools/provider.rs:246`, `crates/agentdash-application/src/vfs/tools/provider.rs:258`）。
- Lifecycle VFS mount is run-scoped, but current/node session projections read from `ActivityAttemptState.executor_run.AgentSession` / `LifecycleRun.session_id`（`crates/agentdash-application/src/vfs/mount.rs:783`, `crates/agentdash-application/src/vfs/mount.rs:832`, `crates/agentdash-application/src/vfs/provider_lifecycle.rs:80`, `crates/agentdash-application/src/vfs/provider_lifecycle.rs:242`, `crates/agentdash-application/src/vfs/provider_lifecycle.rs:321`, `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:529`, `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs:531`）。
- Routine mount stores `routine_id`, `execution_id`, `trigger_source`, `entity_key` in mount metadata（`crates/agentdash-application/src/vfs/mount.rs:787`, `crates/agentdash-application/src/vfs/mount.rs:807`, `crates/agentdash-application/src/vfs/mount.rs:808`, `crates/agentdash-application/src/vfs/mount.rs:809`）。

MCP:
- MCP presets are project-scoped definitions converted into `SessionMcpServer`; they are runtime surface/capability inputs, not subjects（`crates/agentdash-application/src/mcp_preset/definition.rs:35`, `crates/agentdash-application/src/mcp_preset/runtime.rs:8`, `crates/agentdash-application/src/mcp_preset/runtime.rs:20`, `crates/agentdash-application/src/mcp_preset/runtime.rs:31`）。
- Capability resolver injects Story/Task MCP based on `CapabilityScopeCtx`, and workflow MCP based on workflow directives/active workflow（`crates/agentdash-application/src/capability/resolver.rs:1072`, `crates/agentdash-application/src/capability/resolver.rs:1107`, `crates/agentdash-application/src/capability/resolver.rs:1148`, `crates/agentdash-application/src/capability/resolver.rs:971`, `crates/agentdash-application/src/capability/resolver.rs:986`）。

Migration target:
- Canvas visible mounts, VFS surface refs, lifecycle/routine mounts, MCP servers are ActorFrame runtime surface facts.
- `SessionRuntime` VFS surface should become ActorRuntime/ActorFrame surface ref; session id remains low-level runtime address.
- Canvas may be a `SubjectRef(kind=Canvas)` only when a lifecycle/activity explicitly acts on a canvas asset; otherwise it is a frame-visible project asset.

### Routine

Current responsibility:
- `Routine` is project-level Agent trigger rule with `project_agent_id`, `trigger_config`, and `session_strategy`; `RoutineExecution` stores trigger payload, resolved prompt, `session_id`, status, entity_key（`crates/agentdash-domain/src/routine/entity.rs:10`, `crates/agentdash-domain/src/routine/entity.rs:17`, `crates/agentdash-domain/src/routine/entity.rs:19`, `crates/agentdash-domain/src/routine/entity.rs:21`, `crates/agentdash-domain/src/routine/entity.rs:100`, `crates/agentdash-domain/src/routine/entity.rs:110`, `crates/agentdash-domain/src/routine/entity.rs:119`）。
- `RoutineExecutor` renders prompt, loads ProjectAgent/workspace, resolves session according to Fresh/Reuse/PerEntity, launches command, then `mark_completed` only means prompt dispatched to session（`crates/agentdash-application/src/routine/executor.rs:40`, `crates/agentdash-application/src/routine/executor.rs:136`, `crates/agentdash-application/src/routine/executor.rs:145`, `crates/agentdash-application/src/routine/executor.rs:169`, `crates/agentdash-application/src/routine/executor.rs:247`, `crates/agentdash-application/src/routine/executor.rs:253`, `crates/agentdash-application/src/routine/executor.rs:266`, `crates/agentdash-application/src/routine/executor.rs:277`）。
- Session strategy is explicitly session-centric: Fresh creates new project-owned session, Reuse finds/creates project agent session, PerEntity reuses latest execution session by entity key when available（`crates/agentdash-application/src/routine/executor.rs:325`, `crates/agentdash-application/src/routine/executor.rs:331`, `crates/agentdash-application/src/routine/executor.rs:340`, `crates/agentdash-application/src/routine/executor.rs:358`, `crates/agentdash-application/src/routine/executor.rs:368`, `crates/agentdash-application/src/routine/executor.rs:487`）。
- Routine VFS exposes `current/trigger.json`, `current/execution.json`, `current/resolved-prompt.md`, durable memory, and current entity memory; current facts are read-only（`crates/agentdash-application/src/vfs/provider_routine.rs:58`, `crates/agentdash-application/src/vfs/provider_routine.rs:66`, `crates/agentdash-application/src/vfs/provider_routine.rs:82`, `crates/agentdash-application/src/vfs/provider_routine.rs:91`, `crates/agentdash-application/src/vfs/provider_routine.rs:119`, `crates/agentdash-application/src/vfs/provider_routine.rs:373`, `crates/agentdash-application/src/vfs/provider_routine.rs:374`, `crates/agentdash-application/src/vfs/provider_routine.rs:375`）。

Migration target:
- `RoutineExecution` should be `SubjectRef(kind=RoutineExecution)` with role Source, and trigger payload is Activity payload/frame contribution.
- Routine session strategy should map to Actor reuse policy, not direct RuntimeSession reuse. PerEntity becomes actor lookup by subject association/entity key.
- Routine VFS is ActorFrame context surface for the Actor spawned/resumed by a Routine source.

### Permission

Current responsibility:
- `PermissionGrant` is session-bound today (`session_id`) and scoped by `GrantScope::Session | WorkflowStep`（`crates/agentdash-domain/src/permission/entity.rs:17`, `crates/agentdash-domain/src/permission/entity.rs:22`, `crates/agentdash-domain/src/permission/value_objects.rs:8`, `crates/agentdash-domain/src/permission/value_objects.rs:12`, `crates/agentdash-domain/src/permission/value_objects.rs:14`）。
- Grant request evaluates ProjectAgent auto-grantable capabilities intersected with lifecycle requestable capabilities, persists grant, and compiles approved grant to `RuntimeCapabilityTransition`（`crates/agentdash-application/src/permission/service.rs:19`, `crates/agentdash-application/src/permission/service.rs:21`, `crates/agentdash-application/src/permission/service.rs:61`, `crates/agentdash-application/src/permission/service.rs:62`, `crates/agentdash-application/src/permission/service.rs:102`, `crates/agentdash-application/src/permission/compiler.rs:18`, `crates/agentdash-application/src/permission/compiler.rs:25`）。
- Scope escalation coordinator finds active session grant, validates `ScopeEscalationIntent.target_subject_kind`, creates `LifecycleRunLink(ControlScope)` for the newly created subject, then marks grant scope-escalated（`crates/agentdash-application/src/permission/escalation.rs:46`, `crates/agentdash-application/src/permission/escalation.rs:56`, `crates/agentdash-application/src/permission/escalation.rs:62`, `crates/agentdash-application/src/permission/escalation.rs:72`, `crates/agentdash-application/src/permission/escalation.rs:77`, `crates/agentdash-application/src/permission/escalation.rs:81`, `crates/agentdash-application/src/permission/escalation.rs:91`）。
- Repository supports active grants by session/run and escalation grant by session（`crates/agentdash-domain/src/permission/repository.rs:15`, `crates/agentdash-domain/src/permission/repository.rs:21`, `crates/agentdash-domain/src/permission/repository.rs:24`）。

Migration target:
- PermissionGrant should be an ActorFrame capability source; apply/approve creates ActorFrame revision rather than only session runtime transition.
- Scope escalation link should migrate from `LifecycleRunLink(ControlScope)` to `LifecycleSubjectAssociation(anchor=actor|run, subject_ref, role=ControlScope)`.
- Grant scope should align with ActorFrame/activity rather than `Session` / `WorkflowStep` naming.

### Agent Repository / ProjectAgent

Current responsibility:
- `ProjectAgent` holds config and defaults (`default_lifecycle_key`, default-for-story/task, knowledge flag), not runtime state（`crates/agentdash-domain/src/agent/entity.rs:13`, `crates/agentdash-domain/src/agent/entity.rs:27`, `crates/agentdash-domain/src/agent/entity.rs:30`, `crates/agentdash-domain/src/agent/entity.rs:33`）。
- Repository supports get/list by project; `RepositorySet` centralizes project_agent_repo, workflow repos, lifecycle run/link repos, routine, permission, mcp, canvas（`crates/agentdash-domain/src/agent/repository.rs:8`, `crates/agentdash-domain/src/agent/repository.rs:13`, `crates/agentdash-domain/src/agent/repository.rs:27`, `crates/agentdash-application/src/repository_set.rs:40`, `crates/agentdash-application/src/repository_set.rs:59`, `crates/agentdash-application/src/repository_set.rs:61`, `crates/agentdash-application/src/repository_set.rs:67`, `crates/agentdash-application/src/repository_set.rs:68`, `crates/agentdash-application/src/repository_set.rs:69`, `crates/agentdash-application/src/repository_set.rs:72`）。

Migration target:
- ProjectAgent should remain ActorProcedure/profile/config source. Runtime Actor identity should reference ProjectAgent/profile but live under Lifecycle/Actor.
- StoryAgent creating TaskAgent/Companion needs durable Actor lineage/association; do not infer it from ProjectAgent defaults, task.agent_binding, or SessionMeta alone.

## Caveats / Not Found

- No external references were needed; this slice is internal source/spec research only.
- I did not find a durable companion RunLink/SubjectAssociation creation path in `setup_companion_workflow`; companion workflow overlay currently creates a LifecycleRun with child `session_id` only.
- I did not find a durable Task start path that writes the `LifecycleRunLink(Task)` expected by `find_task_execution_session_id`; current `bind_session_to_owner` is still TODO + owner bootstrap pending.
- `ActivityAttemptState` is already narrowly shaped as execution evidence; the missing piece is Actor/ActorFrame/Assignment, not a rename of ActivityAttemptState.
- Existing code still contains names like `step_key` / `lifecycle_step_key` in projection payloads and Task fields; migration should treat them as activity-key compatibility in the current model, then converge naming through Activity/SubjectRef rather than preserving a separate Step predicate.
