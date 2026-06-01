# Research: Phase 3 Dispatch Taxonomy Current Audit

- Query: Phase 3「Execution Dispatch Taxonomy」当前代码是否真正达成，重点审计 Dispatch / GraphResolver / 业务入口 / Task execution contract。
- Scope: internal
- Date: 2026-06-02

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/prd.md` | 定义本任务要判断旧路径残留背后的模型耦合，而不是只做搜索替换。 |
| `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/design.md` | Boundary 3/4/7 定义 dispatch taxonomy、WorkflowGraphResolver、SubjectExecutionContract。 |
| `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md` | Phase 3 gate 明确要求 typed intent/result、ByKey 测试、业务入口统一 dispatch、SubjectRef 到 ActivityAttemptState 证据链。 |
| `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/raw-exposed-issues-checklist.md` | P0-03/P0-04/P0-05/P0-06/P1-15/P1-20/P1-21 是本次审计的直接来源。 |
| `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/structural-analysis.md` | 解释 assignment_ref、ByKey、manual run、Story root/freeform 背后的结构性问题。 |
| `.trellis/spec/backend/workflow/activity-lifecycle.md` | 规定 AgentAssignment 是 Agent/Frame 到 ActivityAttempt 的执行证据桥。 |
| `.trellis/spec/backend/workflow/lifecycle-run-link.md` | 规定 SubjectRef 通过 LifecycleSubjectAssociation 进入 lifecycle，不通过 RuntimeSession ownership。 |
| `.trellis/spec/backend/story-task-runtime.md` | 规定 Story/Task 运行时 truth 位于 LifecycleRun/GraphInstance/Agent/Frame/Assignment。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | 规定前端消费的业务 HTTP DTO 应进入 `agentdash-contracts` 并生成 TS。 |
| `crates/agentdash-domain/src/workflow/dispatch.rs` | 当前 dispatch intent/result taxonomy 定义。 |
| `crates/agentdash-application/src/workflow/dispatch_service.rs` | 当前 LifecycleDispatchService typed facade 与共享 dispatch_common 实现。 |
| `crates/agentdash-application/src/workflow/graph_resolver.rs` | 当前 WorkflowGraphResolver。 |
| `crates/agentdash-api/src/routes/project_agents.rs` | ProjectAgent open 入口。 |
| `crates/agentdash-application/src/task/service.rs` | Task start/continue/cancel application 入口。 |
| `crates/agentdash-api/src/routes/task_execution.rs` | Task execution HTTP route。 |
| `crates/agentdash-api/src/dto/task_execution.rs` | Task execution route-local DTO。 |
| `crates/agentdash-application/src/companion/tools.rs` | Companion sub/human/parent dispatch 与 gate handling。 |
| `crates/agentdash-application/src/routine/dispatch.rs` | Routine DispatchStrategy 到 SubjectExecutionIntent 映射。 |
| `crates/agentdash-application/src/routine/executor.rs` | Routine fire 执行 dispatch 并持久化 refs。 |
| `crates/agentdash-api/src/routes/workflows.rs` | Manual lifecycle run route。 |
| `crates/agentdash-api/src/routes/story_runs.rs` | Story SubjectExecutionView 读投影 route。 |
| `crates/agentdash-application/src/workflow/freeform.rs` | Freeform built-in graph/procedure definition seeding。 |
| `crates/agentdash-contracts/src/project_agent.rs` | ProjectAgent generated launch result contract。 |
| `crates/agentdash-contracts/src/workflow.rs` | Shared lifecycle/subject/generated workflow DTO。 |
| `packages/app-web/src/services/story.ts` | 前端 Task start/continue 丢弃 route response 后重新 fetch task。 |

### Phase 3 Gates

| Gate | Status | Code Evidence | Judgment |
| --- | --- | --- | --- |
| 类型层面不再依赖一个全 optional `ExecutionDispatchResult`；不同 result variant 有 required refs | partial | `ExecutionIntent` 已是 tagged enum，见 `crates/agentdash-domain/src/workflow/dispatch.rs:176`；`ExecutionDispatchResult` 已是 tagged enum，见 `dispatch.rs:235`；但 `AgentLaunchDispatchResult.assignment_ref` 是 required，见 `dispatch.rs:191`，`InteractionGateOpenedDispatchResult.assignment_ref` 也是 required，见 `dispatch.rs:224`。 | 表面 taxonomy 已建立，但 result required refs 仍被共享流水线决定，不是由各 intent 的真实语义决定。 |
| `WorkflowGraphRef::ByKey` 解析失败有测试，并返回错误而不是生成随机 graph/lifecycle id | pass | `WorkflowGraphResolver::resolve` 对 `ByKey` 使用 `get_by_project_and_key` 并 NotFound，见 `crates/agentdash-application/src/workflow/graph_resolver.rs:43`；dispatch 使用 resolver，见 `dispatch_service.rs:314` 和 `dispatch_service.rs:347`；unknown key 测试断言不创建 run/graph/assignment，见 `dispatch_service.rs:1451`。 | P0-04 的核心缺陷已关闭。边界仍偏薄：`ResolvedWorkflowGraph` 只返回 graph，未表达 source/provenance，且 Freeform 仍通过普通 `ByKey` 表达。 |
| API route 检查证明 ProjectAgent、Task、Companion、Routine、manual run、Story root/freeform 都进入 typed dispatch | partial | ProjectAgent 使用 `AgentLaunchIntent` + `launch_agent`，见 `crates/agentdash-api/src/routes/project_agents.rs:156` 和 `project_agents.rs:184`；Task start/continue 使用 `SubjectExecutionIntent` + `execute_subject`，见 `crates/agentdash-application/src/task/service.rs:118`、`service.rs:163`、`service.rs:298`；Companion sub wait 使用 `InteractionDispatchIntent`，async 使用 `AgentLaunchIntent`，见 `crates/agentdash-application/src/companion/tools.rs:408` 和 `tools.rs:440`；Routine fire 使用 `SubjectExecutionIntent` + `execute_subject`，见 `crates/agentdash-application/src/routine/dispatch.rs:36` 和 `executor.rs:242`；manual run 使用 `LifecycleRunStartIntent`，见 `crates/agentdash-api/src/routes/workflows.rs:327`；Story routes 只有 read projection，见 `crates/agentdash-api/src/routes/story_runs.rs:32` 和 `story_runs.rs:57`；Freeform service 只 ensure definition，见 `crates/agentdash-application/src/workflow/freeform.rs:34`。 | 多数业务入口已接 typed facade，但 Story root/freeform launch 未发现写入口；Companion parent/human 仍大量依赖 session notification/hook runtime；manual route 仍返回 bare `LifecycleRun` 而不是 generated dispatch result。 |
| Subject execution 测试证明返回 assignment 或 pending assignment ref，且 SubjectRef 能追溯到 ActivityAttemptState | fail | `SubjectExecutionDispatchResult.assignment_ref` 是 required，见 `crates/agentdash-domain/src/workflow/dispatch.rs:199`；`execute_subject` 确实返回 `facts.assignment.id`，见 `crates/agentdash-application/src/workflow/dispatch_service.rs:276`；但 `dispatch_common` 对所有 non-run-start intent 只创建 `WorkflowGraphInstance::new_root/new` 后直接 `create`，见 `dispatch_service.rs:454`，而 `WorkflowGraphInstance::new_root/new` 的 `activity_state` 初始是 `None`，见 `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:25` 和 `workflow_graph_instance.rs:39`；只有 `start_lifecycle_run` 初始化 Activity state，见 `dispatch_service.rs:321`。 | SubjectExecution 目前能返回 assignment，但该 assignment 没有被证明连接到真实 `ActivityAttemptState`；代码形态还显示 dispatch_common 创建 assignment 时没有初始化 GraphInstance activity state。 |

### Code Patterns

- Domain taxonomy 已从旧宽 DTO 变成 typed enum：`ExecutionIntent::{AgentLaunch, SubjectExecution, LifecycleRunStart, InteractionDispatch}` 在 `crates/agentdash-domain/src/workflow/dispatch.rs:176`；`ExecutionDispatchResult` 在 `dispatch.rs:235`。
- Application facade 名字已经分开：`launch_agent`、`execute_subject`、`open_interaction_gate`、`start_lifecycle_run` 分别在 `crates/agentdash-application/src/workflow/dispatch_service.rs:250`、`dispatch_service.rs:266`、`dispatch_service.rs:288`、`dispatch_service.rs:310`。
- 但 facade 全部复用 `dispatch_common`，除 `start_lifecycle_run` 外都被压成同一组 `DispatchFacts { run, graph_instance, agent, frame, assignment, gate?, subject_execution_ref? }`，见 `dispatch_service.rs:132` 和 `dispatch_service.rs:343`。
- `dispatch_common` 总是创建 entry assignment，见 `dispatch_service.rs:399`；这让 AgentLaunch / Interaction 也被迫拥有 assignment。
- `resolve_or_create_entry_assignment` 只检查 graph entry activity 并写 `AgentAssignment`，见 `dispatch_service.rs:556`；它没有创建或推进 `ActivityAttemptState`。
- `WorkflowGraphInstance::new_root` / `new` 默认 `activity_state: None`，见 `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:25` 和 `workflow_graph_instance.rs:39`。
- `ActivityLifecycleRunService::load_context` 对缺少 `activity_state` 的 graph instance 会报错，见 `crates/agentdash-application/src/workflow/activity_run.rs:145`；这说明 dispatch_common 创建的 graph instance 不是完整可调度 Activity state。
- Task execution route 已返回 `assignment_ref`，见 `crates/agentdash-api/src/routes/task_execution.rs:47` 和 `task_execution.rs:97`，但 DTO 仍在 API crate route-local，见 `crates/agentdash-api/src/dto/task_execution.rs:7`、`task_execution.rs:16`、`task_execution.rs:38`。
- `agentdash-contracts` 生成 workflow view DTO 和 ProjectAgent launch DTO，见 `crates/agentdash-contracts/src/generate_ts.rs:265` 和 `generate_ts.rs:415`；未发现 StartTask/ContinueTask response 进入 generated contracts。
- 前端 `startTaskExecution` / `continueTaskExecution` 仍丢弃 response 再 `fetchTask`，见 `packages/app-web/src/services/story.ts:350` 和 `story.ts:358`。

### Unfinished Points and Coupling Diagnosis

| Unfinished Point | Model Over-Coupling? | Evidence | Why It Matters |
| --- | --- | --- | --- |
| `AgentLaunchIntent` 被迫返回 `assignment_ref` | yes | `AgentLaunchDispatchResult.assignment_ref: Uuid` 在 `crates/agentdash-domain/src/workflow/dispatch.rs:191`；`launch_agent` 填 `facts.assignment.id`，见 `crates/agentdash-application/src/workflow/dispatch_service.rs:260`；`dispatch_common` 总是 `resolve_or_create_entry_assignment`，见 `dispatch_service.rs:399`。 | 这是模型过度耦合，不是旧路径残留。AgentLaunch 的设计边界是 agent/frame/runtime surface，不应保证 ActivityAttempt assignment；assignment 应属于 SubjectExecution 或 scheduler/ActivityAttempt evidence。 |
| typed intent facade 共享 `dispatch_common` | yes | `AgentLaunchIntent`、`SubjectExecutionIntent`、`InteractionDispatchIntent` 都转换成同一个 `DispatchPlan`，见 `dispatch_service.rs:143`、`dispatch_service.rs:160`、`dispatch_service.rs:177`。 | 类型拆分存在，但事务边界和事实创建顺序仍混在一起；这会让 result variant 的 required refs 由共享实现而不是 intent semantic 决定。 |
| SubjectExecution assignment 没有证明能追到 ActivityAttemptState | yes | `WorkflowGraphInstance::new/new_root` activity_state 是 None，见 `workflow_graph_instance.rs:25`、`workflow_graph_instance.rs:39`；dispatch_common 没有 `LifecycleEngine::initialize`，见 `dispatch_service.rs:343`；`start_lifecycle_run` 才初始化 state，见 `dispatch_service.rs:321`。 | 这是执行证据链不闭合，不是少一个测试。SubjectRef -> association -> assignment 可能存在，但 assignment -> ActivityAttemptState 缺少 graph state truth。 |
| Story root/freeform launch 未进入 dispatch | yes | Story route 只有 GET projection，见 `crates/agentdash-api/src/routes/story_runs.rs:30`；`SubjectRef::new("story")` 只用于查询，见 `story_runs.rs:57` 和 `story_runs.rs:79`；Freeform 只 ensure definition，见 `crates/agentdash-application/src/workflow/freeform.rs:34`。 | Story 是业务 aggregate root；没有 root launch policy 时，Task/Routine/Companion 的 parent lifecycle context 会继续依赖 ProjectAgent 或 ad hoc freeform run。 |
| Task execution result 仍 route-local，前端丢弃 dispatch response | yes | API DTO 在 `crates/agentdash-api/src/dto/task_execution.rs:7`；前端 `await api.post(...); return fetchTask(taskId)` 在 `packages/app-web/src/services/story.ts:350`。 | 这是跨层 contract 边界未封装，不只是旧 DTO 文件残留。后端 dispatch result 没有成为前端可依赖的 subject execution command result。 |
| Routine `Reuse` 缺少稳定 anchor | yes | `DispatchStrategy::Reuse` 映射 `RunPolicy::ReuseExisting`，但 `parent_run_id` 默认 None，见 `crates/agentdash-application/src/routine/dispatch.rs:18` 和 `dispatch.rs:40`；`resolve_or_create_run` 在没有 parent_run_id 时创建新 run，见 `crates/agentdash-application/src/workflow/dispatch_service.rs:435`。 | Reuse 语义仍耦合到 caller 是否提前找到 run id；缺少 LifecycleAgentReuseResolver / subject association anchor。 |
| Companion wait 部分接入 gate，但 parent/human 原始路径仍 session delivery-first | yes | sub wait 用 `InteractionDispatchIntent`，见 `crates/agentdash-application/src/companion/tools.rs:408`；原始扫描时 human wait 自行创建 `LifecycleGate` 并注入 session notification，parent path 注入 notification / pending action。后续 Phase 5 slice 已将 human respond 与 parent result return 收束到 `CompanionGateControlService`，但 parent request pending action / hook evaluation 仍由 parent hook runtime/session 承载。 | 这是 interaction/gate truth 和 runtime delivery 混合；Phase 3 的 sub dispatch 入口有进展，Phase 5 已关闭 human respond 与 parent result return，剩余 wait/resume 分裂点在 parent request owner。 |

### Special Judgment: `AgentLaunchIntent` and `assignment_ref`

`AgentLaunchIntent` 仍被迫返回 `assignment_ref`，并且不是 optional：domain result 在 `crates/agentdash-domain/src/workflow/dispatch.rs:186` 到 `dispatch.rs:195` 定义 `AgentLaunchDispatchResult`，其中 `assignment_ref: Uuid` 是 required。application `launch_agent` 在 `crates/agentdash-application/src/workflow/dispatch_service.rs:250` 到 `dispatch_service.rs:263` 直接从 `dispatch_common` 的 entry assignment 填入该字段。

这违反了设计里的边界：`AgentLaunchIntent` 应表达创建/复用 `LifecycleAgent + AgentFrame + optional RuntimeSession`，不应保证 ActivityAttempt assignment。当前实现把 “agent surface launch” 和 “entry activity execution evidence” 绑定在一起；这不是单纯旧路径残留，而是模型层把 launch 与 subject/activity execution 过度耦合。

更关键的是，这个 required assignment 并不等于已闭合 ActivityAttempt 证据链。`dispatch_common` 创建 graph instance 后没有初始化 `activity_state`；`WorkflowGraphInstance::new_root/new` 默认 `activity_state: None`。因此 `assignment_ref` 在 AgentLaunch / SubjectExecution / Interaction result 中看起来很强，但它可能只是一个 entry assignment row，而不是已被 GraphInstance Activity state 承认的 attempt truth。

### Minimal Structural Repair Order

1. 先拆 `dispatch_common` 的事实创建边界：`AgentLaunchIntent` 只创建/复用 run、agent、frame、runtime trace；不创建 required assignment，不返回 `assignment_ref`。需要 entry assignment 的路径必须显式进入 `SubjectExecutionIntent` 或 scheduler-owned activity execution。
2. 让 `SubjectExecutionIntent` 拥有 Activity graph instance 初始化与 assignment scheduling：创建或复用 graph instance 时必须初始化 `ActivityLifecycleRunState`，并返回 `SubjectExecutionAssigned` 或明确的 `SubjectExecutionScheduled`。没有真实 ActivityAttemptState 时不能返回看似完成的 required `assignment_ref`。
3. 将 `InteractionDispatchIntent` 拆成 gate truth 和 optional child agent launch：`InteractionGateOpened` 的 required refs 应是 `gate_ref` 加 parent run/agent/frame target；child agent refs 只有请求了 child launch 时才存在，assignment 不属于 interaction gate 的 required result。
4. 保持 `WorkflowGraphResolver` 为 dispatch 前置边界，并补齐 result provenance / explicit freeform 表达：Freeform 不应靠普通 missing/default key 语义隐藏在 route 中。
5. 迁移 Story root/freeform launch：新增 Story root / freeform subject launch service，提交 `SubjectExecutionIntent(subject_ref=Story)` 或明确 root launch intent，创建 Story subject association 和 root agent/frame。
6. 把 Task start/continue result 迁入 `agentdash-contracts` 并生成 TS；前端 start/continue 直接消费 generated `SubjectExecutionDispatchResult` / Task execution command result，再按需刷新 projection，不再丢弃 dispatch response。
7. 收束 Routine reuse 到稳定 anchor：引入 LifecycleAgentReuseResolver，以 routine/entity/subject association 查 active owner；`ReuseExisting` 无 anchor 时不应静默创建新 run。

### Related Specs

- `.trellis/spec/backend/workflow/activity-lifecycle.md`: AgentAssignment 是 `WorkflowGraphInstance -> ActivityState -> ActivityAttemptState -> AgentAssignment -> LifecycleAgent -> AgentFrame` 链路中的证据桥。
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`: SubjectRef 查询必须通过 LifecycleSubjectAssociation，RuntimeSession 只做 trace 反查。
- `.trellis/spec/backend/story-task-runtime.md`: Story 不绑定 RuntimeSession；Task execution 通过 `SubjectRef(kind=Task)` 进入 execution intent。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: 前端消费的业务 HTTP DTO 应进入 `agentdash-contracts` 并生成 TS，route-local DTO 只允许极小 transport wrapper。

### External References

- None. This audit is internal-only and based on local Trellis docs/specs plus current repository code.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task; the user supplied the explicit task directory and target output path, so this file was written there.
- I did not run tests because this research role is constrained to write only under this task's `research/` directory; test/build commands may write outside that directory.
- No Story root/freeform write-side launch route or service was found. Existing Story routes are read projections.
- No generated StartTask/ContinueTask execution result contract was found in `agentdash-contracts` or `packages/app-web/src/generated`.
- No test was found that proves `SubjectExecutionIntent` creates an assignment that is also represented by `ActivityAttemptState` in initialized `WorkflowGraphInstance.activity_state`.

## Post-Fix Update: 2026-06-02

本文件前文记录的是 Phase 3 修复前的旁路审计结果；随后已完成一个 dispatch taxonomy 修复 slice：

- `AgentLaunchDispatchResult` 移除 required `assignment_ref`，ProjectAgent launch 不再把 pure agent surface launch 伪装成 ActivityAttempt assignment。
- `LifecycleDispatchService` 用显式 `bind_entry_assignment` 区分 `AgentLaunchIntent` 与 `SubjectExecutionIntent` / `InteractionDispatchIntent`；只有后两者创建 entry assignment。
- dispatch 创建或复用 graph instance 时会初始化 `ActivityLifecycleRunState`，因此 SubjectExecution 返回的 assignment 可追溯到同一 `graph_instance_id + activity_key + attempt` 的 ActivityAttemptState。
- terminal resolver 对无 activity scope 的 AgentFrame 返回 `Ok(None)`，保留 pure agent surface runtime 与 activity runtime 的边界。

已验证：

- `cargo test -p agentdash-domain workflow::dispatch --lib -- --format terse`
- `cargo test -p agentdash-application workflow::dispatch_service --lib -- --format terse`
- `cargo test -p agentdash-application workflow::session_association --lib -- --format terse`
- `cargo test -p agentdash-application routine::dispatch --lib -- --format terse`
- `cargo check -p agentdash-domain -p agentdash-application -p agentdash-api -p agentdash-contracts`

仍未关闭：

- Story root/freeform 写侧 launch 未发现统一 dispatch 入口。
- Task execution start/continue response 仍未进入 generated contracts，前端仍丢弃 route response 后重新 fetch task。
- Companion parent request 仍需在 Phase 5 继续拆分 durable gate truth 与 runtime notification delivery；human respond 与 parent result return 已在后续 slice 迁入 gate-first boundary。

## Post-Fix Update: 2026-06-02 Routine Reuse

Routine reuse 已在后续 Phase 5 slice 中关闭：

- 新增 `LifecycleAgentReuseResolver`，按 routine execution 历史、entity key、dispatch refs、LifecycleRun、LifecycleAgent、AgentFrame、AgentAssignment 与 `LifecycleSubjectAssociation` 校验可复用 anchor。
- `DispatchStrategy::Reuse` 无有效 anchor 时返回 conflict，不再让 `RunPolicy::ReuseExisting` 缺 `parent_run_id` 时创建新 run。
- `DispatchStrategy::PerEntity` 首次 entity 触发显式使用 `CreateLinkedRun + Create` 创建 per-entity anchor；已有 target 时显式传入 `parent_run_id + parent_agent_id`。
- `LifecycleDispatchService` 现在校验 explicit `parent_agent_id`，并拒绝缺 `parent_run_id` 的 `ReuseExisting` / `AppendGraph`。

已验证：

- `cargo test -p agentdash-application routine::reuse_resolver --lib -- --format terse`
- `cargo test -p agentdash-application routine::dispatch --lib -- --format terse`
- `cargo test -p agentdash-application workflow::dispatch_service --lib -- --format terse`
- `cargo check -p agentdash-application`
