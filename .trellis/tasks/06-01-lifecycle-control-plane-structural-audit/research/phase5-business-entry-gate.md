# Research: Phase 5 business entry and interaction gate convergence

- Query: Phase 5（业务入口与 interaction/gate 收束），覆盖 P0-06、P1-18、P1-19、P1-20、P1-21、P1-22。
- Scope: internal
- Date: 2026-06-02

## Findings

### Files Found

- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md`: Phase 5 的任务清单与 gate 要求，聚焦 Story root/freeform、Task command、Companion Gate、Routine Reuse、Permission provenance。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/structural-analysis.md`: P0-06、P1-18 至 P1-22 的结构分析与目标封装建议。
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/raw-exposed-issues-checklist.md`: P0/P1 原始暴露问题索引。
- `.trellis/spec/backend/story-task-runtime.md`: Story/Task 不能携带 RuntimeSession truth，Task start/continue/cancel 应走 SubjectExecution facade。
- `.trellis/spec/backend/workflow/activity-lifecycle.md`: Lifecycle run/graph/assignment/frame/runtime trace 的持久化边界。
- `.trellis/spec/backend/runtime-gateway.md`: RuntimeSession 是执行 trace/delivery，不是业务控制面 truth。
- `.trellis/spec/backend/session/execution-context-frames.md`: AgentFrame 是执行上下文帧，RuntimeSession ref 是可追溯执行引用。
- `.trellis/spec/backend/session/runtime-execution-state.md`: runtime session state 属于运行时状态，不应反向成为业务入口。
- `.trellis/spec/backend/permission/grant-lifecycle.md`: Permission grant 生命周期、审批与效果应用的业务语义。
- `.trellis/spec/backend/permission/policy-engine.md`: Permission policy/effect frame 与 scope escalation 的查询边界。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: 前后端 contract 生成与 API DTO 边界。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`: 前端读取 Lifecycle/SubjectExecution 的展示边界。
- `crates/agentdash-domain/src/workflow/dispatch.rs`: `ExecutionIntent`、`SubjectExecutionIntent`、`InteractionDispatchIntent`、`RunPolicy` 等统一 dispatch taxonomy。
- `crates/agentdash-application/src/workflow/dispatch_service.rs`: `LifecycleDispatchService::dispatch_common` 创建 run、graph instance、agent、frame、gate、assignment、subject association。
- `crates/agentdash-application/src/workflow/freeform.rs`: freeform workflow/procedure 的 definition seeding。
- `crates/agentdash-api/src/routes/story_runs.rs`: Story run/active run 读取 API。
- `packages/app-web/src/features/story/story-subject-execution-panel.tsx`: Story SubjectExecution 读取面板。
- `packages/app-web/src/services/story.ts`: Story/Task 前端 API service。
- `crates/agentdash-api/src/routes/project_agents.rs`: Project agent launch 的现有 route-local dispatch 入口。
- `crates/agentdash-api/src/routes/task_execution.rs`: Task start/continue/cancel API。
- `crates/agentdash-application/src/task/service.rs`: `StoryStepActivationService` 的 start/continue/cancel 实现。
- `crates/agentdash-application/src/task/execution.rs`: Task execution command/result contract。
- `crates/agentdash-application/src/task/config.rs`: Task `agent_binding` 到 executor config 的读取逻辑。
- `crates/agentdash-application/src/task/context_builder.rs`: `/tasks/{id}/session` read-only context 仍读取 task/session runtime 线索。
- `crates/agentdash-application/src/companion/tools.rs`: Companion request/respond 的 sub/parent/human/platform 入口。
- `crates/agentdash-application/src/workflow/lifecycle_gate_service.rs`: durable `LifecycleGate` 服务封装。
- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs`: durable gate entity。
- `crates/agentdash-application/src/session/control.rs`: session-scoped companion response facade。
- `crates/agentdash-api/src/routes/sessions.rs`: `/sessions/{id}/companion-requests/.../respond` 与 `/sessions/{id}/cancel` API。
- `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx`: 前端 companion request 响应 UI。
- `crates/agentdash-domain/src/routine/entity.rs`: Routine dispatch strategy、execution refs、entity key。
- `crates/agentdash-application/src/routine/dispatch.rs`: Routine strategy 到 ExecutionIntent 的映射。
- `crates/agentdash-application/src/routine/executor.rs`: Routine execution 与当前 reuse lookup。
- `crates/agentdash-domain/src/permission/entity.rs`: PermissionGrant 的 run/effect frame/source runtime session 字段。
- `crates/agentdash-domain/src/permission/repository.rs`: PermissionGrant repository 主查询接口。
- `crates/agentdash-application/src/permission/service.rs`: Grant request、approval、frame effect 应用。
- `crates/agentdash-application/src/permission/escalation.rs`: effect frame scoped escalation。
- `crates/agentdash-api/src/routes/permission_grants.rs`: Permission grant API。
- `crates/agentdash-contracts/src/permission.rs`: Permission grant 前后端 contract。
- `crates/agentdash-infrastructure/migrations/0081_permission_grants_frame_anchor.sql`: permission grant 从 session_id 迁移到 effect_frame_id/source_runtime_session_id。

### 1. Story root/freeform launch

**原始现象**

- P0-06 的目标是 Story root/freeform dispatch 必须通过 ExecutionIntent 创建 Story subject association/root LifecycleAgent；Phase 5 明确要求 “Story root/freeform dispatch 入口统一为 Story subject association + LifecycleAgent root”，见 `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:73`。
- 当前未发现 Story root/freeform launch 入口。`crates/agentdash-api/src/routes/story_runs.rs:30` 只注册 `GET /stories/{id}/runs` 和 `GET /stories/{id}/runs/active`，`list_story_runs` 在 `crates/agentdash-api/src/routes/story_runs.rs:43` 只构造 `SubjectRef::new("story", story_uuid)` 读取 projection，`get_active_story_run` 在 `crates/agentdash-api/src/routes/story_runs.rs:65` 只过滤 active run。
- 前端 `StorySubjectExecutionPanel` 也只是读投影。`packages/app-web/src/features/story/story-subject-execution-panel.tsx:131` 在没有 execution 时展示空态，`packages/app-web/src/services/story.ts:242` 以后只有 story/task CRUD 和 task execution API，未发现 launch story API。
- `FreeformLifecycleService::ensure_definition` 只负责 builtin freeform procedure/graph definition。`crates/agentdash-application/src/workflow/freeform.rs:17` 到 `freeform.rs:60` 创建 freeform agent procedure，`build_freeform_lifecycle` 在 `freeform.rs:62` 创建 freeform graph，并不绑定 Story subject 或 root agent。
- 最接近的现有业务入口是 `launch_project_agent`。`crates/agentdash-api/src/routes/project_agents.rs:113` 在 API route 内直接构造 `AgentLaunchIntent`，用 `SubjectRef::new("project", project.id)` 和默认 freeform graph 后调用 `LifecycleDispatchService`，它证明 dispatch taxonomy 可用，但封装位置仍是 route-local。
- `LifecycleDispatchService::dispatch_common` 已经具备 Story launch 所需底座：`dispatch_subject_execution` 在 `crates/agentdash-application/src/workflow/dispatch_service.rs:254` 接 `SubjectExecutionIntent`，`dispatch_common` 在 `dispatch_service.rs:345` 统一创建 run/graph/agent/runtime frame/gate/assignment，`create_subject_association` 在 `dispatch_service.rs:488` 可为非 task subject 创建 run-scoped association。

**过度耦合判断**

- Story 当前只有读侧 SubjectExecution projection，没有写侧 command；如果继续参考 `launch_project_agent` 在 API route 中拼装 run/graph/runtime，会把 Story root 的业务语义散落在 route，而不是沉到 application service。
- `freeform.rs` 是 workflow definition seeding，不应承载 “某个 Story 开始 root freeform run” 的业务入口。把 launch 放进 `FreeformLifecycleService` 会混合 definition lifecycle 和 subject execution lifecycle。
- 现有 `LifecycleDispatchService` 的 `RunPolicy::ReuseExisting` 只按 `parent_run_id` 复用。`resolve_or_create_run` 在 `crates/agentdash-application/src/workflow/dispatch_service.rs:435` 没有 Story subject association lookup，所以 Story root 如果要 create-or-reuse，不能只靠 `RunPolicy::ReuseExisting`。

**推荐封装**

- 新增 Story 写侧 application facade，优先放在 `crates/agentdash-application/src/story/`，命名可为 `StoryRootLaunchService` + `StoryRootLaunchCommand`；若后续 ProjectAgent 也要收束，可抽到 `crates/agentdash-application/src/workflow/subject_launch.rs` 做通用 `SubjectLaunchService`，Story service 只负责业务校验和默认策略。
- Command 最小字段建议：`story_id`、`project_id`、`prompt/input`、`requested_by`、`launch_mode`。service 内部构造 `SubjectExecutionIntent`，`subject_ref = SubjectRef::new("story", story_id)`，默认 `workflow_graph_ref` 指向 story/freeform root graph，`agent_policy` 为 create or resume root，`context_policy` 使用 Story root projection。
- 复用逻辑不要依赖 API route 或 RuntimeSession。应先通过 `LifecycleSubjectAssociationRepository` 查 Story subject 的 active root association，再决定 resume existing agent/run 或 create root；这个 resolver 可以成为 Phase 5 中 Story 与 Routine 共享的 `LifecycleAgentReuseResolver` 的第一版。
- API 入口可放 `crates/agentdash-api/src/routes/story_runs.rs` 旁边或新 `story_execution.rs`，例如 `POST /stories/{id}/execution`。前端入口可放 `StorySubjectExecutionPanel` 的空态/toolbar，因为该组件已经负责 Story subject execution projection。

**验证 gate**

- 静态 gate：`rg -n "SubjectRef::new\\(\"story\"|StoryRootLaunch|launch_story|/stories/.*/execution" crates packages` 应能看到 Story launch 写侧只经 application service，而不是 route-local 拼装 runtime。
- 后端新增测试建议：`cargo test -p agentdash-application story_root_launch`，覆盖 “无 association 创建 Story root run/agent/assignment” 和 “已有 active Story root 时复用”。
- API/contract 测试建议：`cargo test -p agentdash-api story_execution`，`pnpm run contracts:check`，确认新增 endpoint DTO 被生成且不暴露 runtime session 作为业务参数。
- 前端验证建议：`pnpm run frontend:check`，并补一个 Story panel/service 的轻量测试，证明空态 launch 调用 Story execution endpoint，成功后刷新 SubjectExecution。
- 必须改的最小代码切片：application Story launch service、API Story execution route + DTO/contract、前端 `story.ts` service 与 `StorySubjectExecutionPanel` 入口、Story subject association reuse resolver。

### 2. Task start/continue/cancel 与 `agent_binding`

**原始现象**

以下保留本轮调研时的原始快照；2026-06-02 后续状态见本节 “后续 slice 更新”。

- Phase 5 要求 Task command 使用 SubjectExecution service，cancel 收束为 `CancelSubjectExecutionCommand`，见 `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:74`。
- Task start/continue 已经进入 SubjectExecution contract。`start_task` 在 `crates/agentdash-api/src/routes/task_execution.rs:20` 调用 `StoryStepActivationService::start_task(TaskExecutionCommand { ... })`，`continue_task` 在 `task_execution.rs:70` 也是同一路径。
- `start_task_inner` 在 `crates/agentdash-application/src/task/service.rs:105` 构造 `SubjectExecutionIntent`，`subject_ref = SubjectRef::new("task", task.id)`，`run_policy = CreateLinkedRun`，`runtime_policy = CreateRuntimeSession`，随后 `execute_subject` 在 `task/service.rs:277` 调用 `LifecycleDispatchService::dispatch_subject_execution`。
- `continue_task_inner` 在 `crates/agentdash-application/src/task/service.rs:150` 先 `resolve_task_execution_refs`，再用 `parent_run_id = Some(refs.run_id)`、`parent_agent_id = Some(refs.agent_id)`、`RunPolicy::ReuseExisting`、`AgentPolicy::Resume` 构造 `SubjectExecutionIntent`。
- Task cancel 仍使用 raw RuntimeSession。`cancel_task` route 在 `crates/agentdash-api/src/routes/task_execution.rs:109` 只传 task id，`cancel_task_inner` 在 `crates/agentdash-application/src/task/service.rs:195` 通过 `resolve_task_execution_refs` 找 agent，再 `agent_frame_repo.get_current(refs.agent_id)`，最终在 `task/service.rs:213` 取 `frame.first_runtime_session_id()` 并调用 `dispatcher.cancel_session(&session_id)`。
- `TurnDispatcher` trait 在 `crates/agentdash-application/src/task/service.rs:24` 只有 `cancel_session(&str)`，所以 cancel 的 application boundary 仍是 session delivery。
- `TaskExecutionCommand` 只有 `ExecutionPhase::{Start, Continue}`。`crates/agentdash-application/src/task/execution.rs:20` 未定义 cancel command/result；`TaskExecutionResult` 在 `task/execution.rs:36` 返回 run/agent/frame/assignment/subject_execution refs，符合 SubjectExecution 读写 contract。
- P1-18 的 `agent_binding` 仍存在。`resolve_task_executor_source` 在 `crates/agentdash-application/src/task/config.rs:11` 读取 `task.agent_binding.agent_type/preset_name`，`resolve_task_executor_config` 在 `task/config.rs:47` 以 explicit request、task agent_binding、project default 的优先级生成 executor config。`CreateTaskAgentBindingRequest` 在 `crates/agentdash-api/src/dto/story.rs:45` 仍是创建 Task DTO 的一部分，前端 generated contract 中 `packages/app-web/src/generated/core-contracts.ts:6` 也暴露 `AgentBinding`。
- 同时，start/continue route 仍接收 `executor_config` DTO，但当前 `start_task_inner`/`continue_task_inner` 并未使用 `cmd.executor_config`。`StartTaskRequest` 在 `crates/agentdash-api/src/dto/task_execution.rs:7`、`ContinueTaskRequest` 在 `task_execution.rs:28` 暴露该字段。

**过度耦合判断**

- start/continue 与 active assignment cancel 的业务控制面已经切到 SubjectExecution；Task projection 已区分 Cancelled/Failed，剩余风险是 open gate cancellation 尚未进入同一 command transaction。
- `TurnDispatcher::deliver_runtime_cancel` 已把 runtime cancel 收束为 delivery seam；route/service 仍需要在后续 contract 中显式表达 Task subject、Assignment、Gate、Frame 的 command result。
- `agent_binding` 仍在 Task spec、DTO、context builder、frontend contract 中作为 executor preference 参与读取，且 start/continue request 的 `executor_config` 当前未生效，形成 “contract 看起来可指定执行器，实际 dispatch 不用它” 的漂移。
- `context_builder` 的 `/tasks/{id}/session` read-only 路径仍保留 session trace lookup。`find_active_workflow_via_task_sessions` 在 `crates/agentdash-application/src/task/context_builder.rs:181` 从 task association 到 agent/frame，再读 `first_runtime_session_id` 去解析 workflow projection。这是 trace adapter 可接受，但不能继续扩展为 command truth。

**推荐封装**

- 在 `crates/agentdash-application/src/task/execution.rs` 增加 `CancelSubjectExecutionCommand` 或通用 `SubjectExecutionCommand::{Start, Continue, Cancel}`，字段以 `subject_ref`、`requested_by`、`reason`、`idempotency_key` 为主，不接收 runtime session id。
- 在 application workflow 层新增 `SubjectExecutionService::cancel`，内部按 `SubjectRef("task", task_id)` 查 `LifecycleSubjectAssociation`、active `AgentAssignment`、active/open `LifecycleGate`、current `AgentFrame`；先写 lifecycle cancellation intent/status，再把 RuntimeSession cancel 作为 delivery adapter。
- `StoryStepActivationService::cancel_task` 应只做 task/project 校验并委派给 `CancelSubjectExecutionCommand`，route 不再感知 RuntimeSession。
- `agent_binding` 最小处理：Phase 5 不必一次删除字段，但应停止把它作为 executor 决策 truth。Task 启动决策应来自 SubjectExecution/agent policy；若保留 UI 字段，只作为 authoring metadata 或后续 migration 清理对象。`executor_config` DTO 要么移入 SubjectExecution preference 并真实参与 dispatch，要么从 start/continue request contract 移除。

**验证 gate**

- 静态 gate：`rg -n "cancel_session\\(|first_runtime_session_id\\(" crates/agentdash-application/src/task crates/agentdash-api/src/routes/task_execution.rs` 在 Phase 5 后不应命中 task command path，最多只能命中 delivery adapter 或 read-only trace。
- 静态 gate：`rg -n "agent_binding|executor_config" crates/agentdash-application/src/task crates/agentdash-api/src/dto packages/app-web/src/generated packages/app-web/src/features/story` 应显示 executor preference 不再参与 Task execution command 决策。
- 后端测试建议：`cargo test -p agentdash-application task::service`，新增 start/continue 断言 `SubjectExecutionIntent.subject_ref.kind == "task"`，新增 cancel 断言先解析 subject/assignment/gate，再调用 runtime delivery。
- API/contract 测试建议：`cargo test -p agentdash-api task_execution`，`pnpm run contracts:check`，确认 cancel response 返回 subject/assignment/gate refs，而不是 session id。
- 必须改的最小代码切片：`task/execution.rs` command contract、`task/service.rs` cancel implementation、`routes/task_execution.rs` cancel DTO/response、runtime cancel delivery adapter、`agent_binding/executor_config` 的 contract 决策边界。

**后续 slice 更新（2026-06-02）**

- active assignment cancel command boundary 已落地：`SubjectExecutionControlService` 以 `SubjectRef("task", task_id)` 解析 association/agent/assignment/frame，写 `ActivityCancelled`，abandon claim，release assignment，并产出 `RuntimeCancelDeliveryCommand`。
- `TurnDispatcher` 已从 `cancel_session(&str)` 收束为 `deliver_runtime_cancel(RuntimeCancelDeliveryCommand)`；Task service 不再把 raw RuntimeSession 作为 cancel command target。
- 当前剩余风险从 “cancel 是 session command” 转为更窄的问题：open `LifecycleGate` 尚未纳入 `CancelSubjectExecutionCommand` transaction。

### 3. Companion wait/resume/gate

**原始现象**

- Phase 5 要求 Companion Gate 成为 wait/resume truth，见 `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:75`；gate 要求是 “Companion wait/resume durable LifecycleGate truth”，见 `implement.md:83`。
- durable truth 已存在。`LifecycleGate` 在 `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:5` 持有 `run_id`、`agent_id`、`frame_id`、`gate_kind`、`correlation_id`、`status`、`payload`、`resolved_by`、`resolved_at`；`LifecycleGateService` 在 `crates/agentdash-application/src/workflow/lifecycle_gate_service.rs:24` 提供 create/wait/resolve。
- sub/session 目标基本走 dispatch/gate。`CompanionRequestTool::handle_sub_request` 在 `crates/agentdash-application/src/companion/tools.rs:377` 为 `target = sub/session` 构造 `InteractionDispatchIntent`，`wait = true` 时设置 `GatePolicy`，并在 `tools.rs:491` 后使用 gate id 轮询。
- 但 parent notification 仍依赖 runtime session/hook runtime。`handle_parent_request` 在 `crates/agentdash-application/src/companion/tools.rs:597` 从 `current_session_id` 反查 child frame/lineage/parent agent，再在 `tools.rs:690` 选择 parent runtime session，`tools.rs:712` 确保 parent hook runtime，`tools.rs:720` 写 pending action，`tools.rs:734` 注入 session notification。
- human wait 部分创建 durable gate，但 route/UI 仍是 session-first。`handle_human_request` 在 `crates/agentdash-application/src/companion/tools.rs:747` 对 wait=true 创建 `LifecycleGate::open`，payload 里仍写入 `session_id`/`turn_id`，并在 `tools.rs:824` 注入当前 session notification；`poll_gate_until_resolved` 在 `tools.rs:566` 直接轮询 repo。
- human resume API 仍按 session route 暴露。`respond_companion_request` 在 `crates/agentdash-application/src/session/control.rs:66` 以 `(session_id, request_id)` 为入口，若 `request_id` 是 gate UUID 则在 `session/control.rs:76` 读取并 resolve gate，随后在 `session/control.rs:125` 对同一个 session 注入 `companion_human_response` notification。API route 在 `crates/agentdash-api/src/routes/sessions.rs:638`，前端 `respondCompanionRequest` 在 `packages/app-web/src/services/executor.ts:36` POST 到 `/sessions/{sessionId}/companion-requests/{requestId}/respond`。
- 前端 request card 的 truth 来源是 runtime event。`SessionCompanionRequestCard` 在 `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx:21` 从 event payload 读 `request_id` 并绑定 `sessionId`，`SessionCompanionRequestCard.tsx:85` 根据 wait 文案展示 session suspended，而不是 gate projection。
- platform capability grant 仍走 human broker payload。`handle_platform_request` 在 `crates/agentdash-application/src/companion/tools.rs:943` 将 `capability_grant_request` 映射为 human request；未发现它直接调用 `PermissionGrantService::request`。

**过度耦合判断**

- 当前 durable gate 已经是部分 truth，但 request/resume 的入口、UI、通知投递、parent pending action 仍把 RuntimeSession/HookRuntime 当成事实载体。
- parent request 的 pending action 放在 hook runtime 中，`LifecycleGate` 只在 `try_complete_to_parent` 中按 correlation id 查 open gates。这样 parent wait/resume 的 “是否等待、等待谁、谁能恢复” 没有统一 durable owner。
- human wait 的 gate payload 携带 `session_id/turn_id` 是 trace/delivery 信息，可以保留为 audit/delivery metadata，但不能让 API 必须从 session URL 进入。
- `poll_gate_until_resolved` 直接 repo loop 分散在 companion tool 内，缺少 “gate truth + delivery adapter” 的统一封装，后续 parent/human/sub 三条路径很容易继续分叉。

**推荐封装**

- 新增或强化 `InteractionGateService`，以 `LifecycleGateService` 为 durable truth facade。所有 companion wait path 只创建/等待/resolve gate，RuntimeSession notification 通过 `GateDeliveryAdapter` 投递。
- response API 改为 gate-first，例如 `POST /lifecycle-gates/{gate_id}/respond` 或 `POST /interaction-gates/{gate_id}/respond`。session id 只作为 delivery metadata 或 trace lookup，不作为业务 URL 主键。
- parent request wait/resume 应创建 `LifecycleGate`，owner 指向 parent agent/frame，correlation id 指向 child request；hook runtime pending action 只能作为 delivery cache，不作为 resume truth。`companion_respond` 先 resolve gate，再由 adapter 注入 parent/child session notification。
- human request card 应从 gate/request projection 或 gate id contract 驱动。runtime event 可以作为通知来源，但响应时只提交 gate id、payload、actor identity。

**验证 gate**

- 静态 gate：`rg -n "ensure_hook_runtime|pending_actions|inject_notification|/companion-requests" crates/agentdash-application/src/companion crates/agentdash-application/src/session crates/agentdash-api/src/routes/sessions.rs packages/app-web/src/features/session packages/app-web/src/services/executor.ts`。Phase 5 后这些命中应集中到 delivery adapter 或 trace-only session surface。
- 后端测试建议：`cargo test -p agentdash-application companion`，新增 human/sub/parent wait 测试，断言 request 返回 gate id，respond 先 resolve `LifecycleGate`，通知只是副作用。
- Session control 回归测试建议：`cargo test -p agentdash-application session::hub::tests::respond_companion_request_resolves_waiting_tool_and_persists_response_event` 或迁移后的 gate-first 等价测试。
- 前端测试建议：`pnpm --filter app-web test SessionCompanionRequestCard`，断言响应调用 gate-first service，不再需要 session id 作为业务主键。
- 必须改的最小代码切片：`companion/tools.rs` parent/human wait path、`workflow/lifecycle_gate_service.rs` gate facade、`session/control.rs` 或新 gate control service、`routes/sessions.rs` 中 companion respond route、`executor.ts` 与 `SessionCompanionRequestCard`。

### 4. Routine Reuse

**原始现象**

- Phase 5 要求 Routine Reuse 通过 `LifecycleAgentReuseResolver`，gate 要求是 “Routine reuse by routine/entity/subject association, not parent_run_id”，见 `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:76` 和 `implement.md:84`。
- domain 已有 `DispatchStrategy::{Fresh, Reuse, PerEntity}`。`crates/agentdash-domain/src/routine/entity.rs:82` 定义策略，`RoutineDispatchRefs` 在 `routine/entity.rs:98` 持有 run/agent/frame/assignment，`RoutineExecution` 在 `routine/entity.rs:107` 持有 `entity_key`。
- application mapping 只是策略到 dispatch enum 的静态映射。`map_dispatch_strategy` 在 `crates/agentdash-application/src/routine/dispatch.rs:11` 将 `Reuse` 和 `PerEntity` 都映射为 `(RunPolicy::ReuseExisting, AgentPolicy::Resume)`。
- 普通 `build_routine_execution_intent` 在 `crates/agentdash-application/src/routine/dispatch.rs:30` 创建 `SubjectRef("routine_execution", routine_execution.id)`，但 `parent_run_id = None`。只有 `build_routine_execution_intent_with_reuse` 在 `routine/dispatch.rs:55` 收到 `reuse_run_id` 时才填 `parent_run_id`。
- 实际 resolver 只覆盖 `PerEntity`。`execute_with_dispatch` 在 `crates/agentdash-application/src/routine/executor.rs:217` 调 `resolve_reuse_run_id`；`resolve_reuse_run_id` 在 `routine/executor.rs:270` 只处理 `DispatchStrategy::PerEntity`，通过 payload 中 `entity_key` 查 `find_latest_by_entity_key`，再返回上次 `dispatch_refs.run_id`。
- `DispatchStrategy::Reuse` 当前返回 `None`，于是 `RunPolicy::ReuseExisting + parent_run_id=None` 进入 `LifecycleDispatchService::resolve_or_create_run`；该函数在 `crates/agentdash-application/src/workflow/dispatch_service.rs:435` 只有 `parent_run_id` 存在时才复用，否则创建新 run。因此 `Reuse` 模式并未按 routine 复用。
- `find_latest_by_entity_key` 在 `crates/agentdash-infrastructure/src/persistence/postgres/routine_repository.rs:362` 只按 `routine_id/entity_key` 查最近 RoutineExecution；没有验证对应 LifecycleAgent 是否 active、subject association 是否仍有效、assignment/gate 是否匹配。

**过度耦合判断**

- Routine Reuse 当前不是 “routine/entity/subject association truth”，而是 “如果外部提前找到了 parent_run_id 就复用”。`RunPolicy::ReuseExisting` 在没有 `parent_run_id` 时静默创建新 run，容易让测试只覆盖 enum mapping 而漏掉实际复用失败。
- `PerEntity` 依赖 `routine_execution.dispatch_refs.run_id`，使 RoutineExecution 历史表成为 reuse anchor。它不能表达 active lifecycle owner，也不能防止复用 stale/cancelled run。
- 每次 Routine execution 都以新的 `SubjectRef("routine_execution", id)` 创建 association，缺少稳定的 routine subject 或 routine/entity subject anchor；因此 LifecycleSubjectAssociation 不能承担复用查询。

**推荐封装**

- 新增 `LifecycleAgentReuseResolver`，放在 `crates/agentdash-application/src/workflow/` 或 `src/routine/`。输入：`routine_id`、`project_agent_id/agent_key`、`DispatchStrategy`、`entity_key`、`subject_ref`、`project_id`。输出：可复用的 `run_id/agent_id/frame_id/assignment_id` 或 `None`。
- resolver 主查询应以稳定 subject association 为 anchor。`Reuse` 按 `SubjectRef("routine", routine_id)` 或 routine-level association 查 active agent；`PerEntity` 按 routine + entity stable subject 查 active agent；单次 `routine_execution` 仍可作为审计/历史 subject。
- `build_routine_execution_intent` 不应把 `RunPolicy::ReuseExisting` 与空 `parent_run_id` 组合成 “看似复用”。没有 resolver anchor 时应显式选择 Fresh/CreateLinkedRun，或者让 dispatch service 拒绝 `ReuseExisting` without anchor。
- `RoutineExecution.dispatch_refs` 可保留为审计和 UI trace，但不再作为 reuse truth。

**验证 gate**

- 静态 gate：`rg -n "RunPolicy::ReuseExisting|parent_run_id|find_latest_by_entity_key|dispatch_refs\\.run_id" crates/agentdash-application/src/routine crates/agentdash-application/src/workflow/dispatch_service.rs`。Phase 5 后 reuse lookup 应集中在 `LifecycleAgentReuseResolver`。
- 后端测试建议：`cargo test -p agentdash-application routine`，新增三类测试：`Reuse` 对同一 routine 复用同一 lifecycle agent；`PerEntity` 对同一 entity 复用、不同 entity 新建；没有 active association 时明确 fresh create。
- dispatch service 防呆测试建议：`cargo test -p agentdash-application workflow::dispatch_service`，覆盖 `RunPolicy::ReuseExisting` 缺少 resolver anchor 时不会静默假复用。
- 必须改的最小代码切片：`routine/dispatch.rs` strategy mapping、`routine/executor.rs` reuse lookup、workflow/routine 侧 `LifecycleAgentReuseResolver`、routine subject association 写入/查询、现有 routine reuse tests。

### 5. Permission source runtime session provenance 与 effect owner frame/run/subject

**原始现象**

- Phase 5 要求 permission source runtime session 只做 provenance，effect owner 为 frame/run/subject，见 `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:77`；gate 要求 Permission query 以 frame/run/subject 为主，session 只做 audit provenance filter，见 `implement.md:85`。
- 当前 DB migration 已经向目标靠拢。`crates/agentdash-infrastructure/migrations/0081_permission_grants_frame_anchor.sql:1` 注释说明主查询路径从 `session_id` 切到 `effect_frame_id`，并将 `session_id` rename 为 `source_runtime_session_id`。
- domain grant 同时携带 run/effect frame/source runtime session。`PermissionGrant` 在 `crates/agentdash-domain/src/permission/entity.rs:17` 定义 `run_id`、`effect_frame_id`、`source_runtime_session_id`；`new` 在 `permission/entity.rs:51` 接收 source runtime session，`with_effect_frame` 在 `permission/entity.rs:82` 绑定 effect frame。
- repository 主查询已经是 frame/run。`PermissionGrantRepository` 在 `crates/agentdash-domain/src/permission/repository.rs:15` 提供 `list_active_by_frame`，`repository.rs:20` 提供 `list_active_by_run`，`repository.rs:24` 提供 `find_active_escalation_grant(effect_frame_id, ...)`；未发现 `list_by_source_runtime_session` 作为主查询。
- API/contract 也以 frame/run 为主。`ListPermissionGrantsQuery` 在 `crates/agentdash-contracts/src/permission.rs:28` 只有 `effect_frame_id/run_id/status`；`list_grants` 在 `crates/agentdash-api/src/routes/permission_grants.rs:104` 要求 `effect_frame_id` 或 `run_id`。
- 过界点在 effect 应用。`PermissionGrantService::apply_frame_effect` 在 `crates/agentdash-application/src/permission/service.rs:268` 要求 `effect_frame_id` 并创建新的 AgentFrame，但在 `permission/service.rs:326` 复制 current frame runtime session refs 后，又在 `permission/service.rs:327` 把 `grant.source_runtime_session_id` attach 到新 effect frame。
- `PermissionScopeEscalationService::try_escalate` 在 `crates/agentdash-application/src/permission/escalation.rs:54` 以 `effect_frame_id` 查 active escalation grant，并在 `escalation.rs:83` 创建 run-scoped `LifecycleSubjectAssociation`，整体符合 frame/run/subject owner 方向。
- platform `capability_grant_request` 还未直连 PermissionGrantService。`handle_platform_request` 在 `crates/agentdash-application/src/companion/tools.rs:953` 把它转成 human companion request；搜索未发现 companion platform path 调用 `PermissionGrantService::request`。

**过度耦合判断**

- 查询面基本已经收束到 frame/run，但 `apply_frame_effect` 把 source runtime session id 追加进 effect frame runtime refs，会让 provenance 变成 effect owner 的 runtime trace 成员。
- 如果 grant 的 source session 与 effect frame 当前 runtime refs 不一致，新 frame 会携带一个并非由该 frame 执行产生的 session ref。后续任何按 frame runtime refs 找 trace 的逻辑，都可能把授权来源误认为 effect frame 的执行 session。
- `source_runtime_session_id` 出现在 API response 是合理审计信息；问题不是字段存在，而是它参与 `AgentFrame` runtime session refs。

**推荐封装**

- `PermissionGrant.source_runtime_session_id` 保持为 audit provenance，只在 grant entity、storage、response、审计筛选中出现。
- `PermissionGrantService::apply_frame_effect` 创建新 frame 时只继承 effect frame/current frame 自身 runtime refs，不追加 `grant.source_runtime_session_id`。如果需要记录 “哪个请求触发了这个 frame 变更”，应使用 frame metadata/audit event，而不是 runtime_session_refs。
- permission query facade 继续以 `effect_frame_id/run_id` 为主；若需要 session provenance filter，应命名为 audit filter，不能成为 “active grant by session” 的业务入口。
- platform capability grant 可在后续接入 `PermissionGrantService::request`，输入必须包含 `run_id`、`effect_frame_id`、`source_runtime_session_id`，但审批和 effect application 仍以 frame/run owner 为准。

**验证 gate**

- 静态 gate：`rg -n "source_runtime_session_id|list_active_by_session|session_id" crates/agentdash-domain/src/permission crates/agentdash-application/src/permission crates/agentdash-api/src/routes/permission_grants.rs crates/agentdash-contracts/src/permission.rs crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs`。Phase 5 后 source runtime session 应只在 entity/storage/DTO/audit filter，不应出现在 frame runtime ref 写入。
- 后端测试建议：`cargo test -p agentdash-application permission`，新增 `apply_frame_effect_does_not_attach_source_runtime_session_to_effect_frame`。
- infra 测试建议：`cargo test -p agentdash-infrastructure permission_grant_repository`，覆盖 frame/run query 与 source_runtime_session 存储。
- contract 测试建议：`pnpm run contracts:check`，确认 `ListPermissionGrantsQuery` 不以 session 为主查询。
- 必须改的最小代码切片：`permission/service.rs` 的 `apply_frame_effect` frame builder、对应 permission service tests、如需 platform grant 接入则补 companion-to-permission request adapter。

### 6. Phase 5 gates and minimal slices

**原始现象**

- Phase 5 的四个 gate 在 `implement.md:82` 到 `implement.md:85`：Task start/continue/cancel target SubjectRef/assignment/gate；Companion wait/resume durable LifecycleGate truth；Routine reuse by routine/entity/subject association；Permission query frame/run/subject primary。
- 当前代码呈现为 “dispatch taxonomy 与 durable gate 已经存在，但多个业务入口仍绕回 RuntimeSession/HookRuntime/parent_run_id/source session”。

**过度耦合判断**

- P0-06 的 Story 缺写侧入口，风险是后续 UI 或 API 为了快而直接复制 `project_agents.rs` 的 route-local launch。
- P1-19 的 active assignment cancel 已进入 SubjectExecution control boundary，Task projection 已区分 Cancelled/Failed；剩余风险是 open gate cancellation 尚未进入同一 transaction。
- P1-20 的 Companion gate 只有部分路径是 durable truth，风险是 parent/human resume 的 truth 分裂在 hook runtime pending actions 和 session event。
- P1-21 的 Routine reuse 依赖 parent_run_id 或 routine_execution history，风险是 Reuse 策略静默不复用，PerEntity 复用 stale run。
- P1-22 的 Permission 已完成主要查询迁移，但 source session 仍被写入 effect frame runtime refs，风险是 provenance 与 effect owner 混合。

**推荐封装**

- Story：`StoryRootLaunchService` 或通用 `SubjectLaunchService`，封装 Story subject association + root LifecycleAgent 创建/复用。
- Task：`SubjectExecutionService::{start, continue, cancel}`，active assignment cancel 已以 SubjectRef/assignment/frame 为 truth，TaskExecution projection vocabulary 已落地；下一步把 open gate cancellation 纳入同一封装。
- Companion：`InteractionGateService` + `GateDeliveryAdapter`，gate 是 wait/resume truth，session/hook runtime 是通知与 trace。
- Routine：`LifecycleAgentReuseResolver`，以 routine/entity/subject association 查 active owner。
- Permission：`PermissionGrantService` 保持 frame/run 查询；source runtime session 只作为 grant audit provenance，不进入 AgentFrame runtime refs。

**验证 gate**

- Story gate:
  - `cargo test -p agentdash-application story_root_launch`
  - `cargo test -p agentdash-api story_execution`
  - `pnpm run contracts:check`
  - `pnpm run frontend:check`
  - `rg -n "launch_story|StoryRootLaunch|SubjectRef::new\\(\"story\"" crates packages`
- Task gate:
  - `cargo test -p agentdash-application task::service`
  - `cargo test -p agentdash-api task_execution`
  - `rg -n "cancel_session\\(|first_runtime_session_id\\(" crates/agentdash-application/src/task crates/agentdash-api/src/routes/task_execution.rs`
  - `rg -n "agent_binding|executor_config" crates/agentdash-application/src/task crates/agentdash-api/src/dto packages/app-web/src/generated`
- Companion gate:
  - `cargo test -p agentdash-application companion`
  - `cargo test -p agentdash-application session::hub::tests::respond_companion_request_resolves_waiting_tool_and_persists_response_event` 或迁移后的 gate-first 等价测试
  - `pnpm --filter app-web test SessionCompanionRequestCard`
  - `rg -n "ensure_hook_runtime|pending_actions|inject_notification|/companion-requests" crates/agentdash-application/src/companion crates/agentdash-application/src/session crates/agentdash-api/src/routes/sessions.rs packages/app-web/src`
- Routine gate:
  - `cargo test -p agentdash-application routine`
  - `cargo test -p agentdash-application workflow::dispatch_service`
  - `rg -n "RunPolicy::ReuseExisting|parent_run_id|find_latest_by_entity_key|dispatch_refs\\.run_id" crates/agentdash-application/src/routine crates/agentdash-application/src/workflow/dispatch_service.rs`
- Permission gate:
  - `cargo test -p agentdash-application permission`
  - `cargo test -p agentdash-infrastructure permission_grant_repository`
  - `pnpm run contracts:check`
  - `rg -n "source_runtime_session_id|list_active_by_session|session_id" crates/agentdash-domain/src/permission crates/agentdash-application/src/permission crates/agentdash-api/src/routes/permission_grants.rs crates/agentdash-contracts/src/permission.rs`
- Full smoke when slices are complete:
  - `pnpm run backend:test`
  - `pnpm run contracts:check`
  - `pnpm run frontend:check`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task; research output uses the explicit task path provided by the user.
- 未发现 Story root/freeform launch 的现有 UI/API/service 写入口；当前只有 Story execution projection 读取与 ProjectAgent 的 route-local launch 参考实现。
- 未运行测试；本文件只给 Phase 5 的验证命令和建议新增测试点。
- 未发现 companion platform `capability_grant_request` 直接调用 `PermissionGrantService::request`；当前实现先转为 human companion request。
- `.trellis/spec/backend/permission/grant-lifecycle.md` 中仍有早期 session_id 叙述，当前代码和 migration 已改为 `effect_frame_id` + `source_runtime_session_id`，本调研以当前代码和 `0081_permission_grants_frame_anchor.sql` 为准。
