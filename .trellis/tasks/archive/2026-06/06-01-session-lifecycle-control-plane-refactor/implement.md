# 执行计划

## 执行模型

本任务是父级重构合同，不直接作为单次代码改动落地。进入 implementation 后按 child task 顺序推进；每个 child task 必须独立可验证，并在自己的 artifact 中写明依赖，不能只依赖父子目录关系。

全局执行以 `target-state-blueprint.md` 为验收蓝图。`design.md` 只解释旧结构如何迁移；child task 的 PRD 必须声明自己推进的 blueprint step，以及完成后蓝图中哪一条退出状态成立。

## 重构模式

本轮执行采用 breaking-mode 迁移窗口：

- 允许中间阶段系统无法启动、页面崩溃、接口缺失、部分测试失败。
- 不实现兼容层、fallback、双写双读或旧字段兜底。
- 每个 child task 优先切断本阶段对应旧事实源，再建立目标事实源。
- 不以保持旧产品体验为目标；以消除旧语义和建立蓝图不变量为目标。
- 每个 child task 的退出验收只要求其 blueprint step 的事实源和边界成立。
- 若需要跨 task 才能恢复完整运行链路，应在 child PRD 写明断裂点和后续接续任务。

## 子任务顺序

1. `06-01-session-lifecycle-spec-convergence`
   - 锁定 specs 与术语：`RuntimeSession`、`LifecycleRun`、`WorkflowGraph`、`WorkflowGraphInstance`、`LifecycleAgent`、`AgentFrame`、`AgentAssignment`、`LifecycleSubjectAssociation`、`LifecycleGate`。
   - 质量门：spec 中不再把 Session / Binding / Step / single WorkflowGraph 写成控制面事实源。

2. `06-01-session-lifecycle-target-anchors-schema`
   - 新增目标锚点表与 repository：`lifecycle_workflow_instances`、`lifecycle_agents`、`agent_frames`、`agent_assignments`、`lifecycle_subject_associations`、`lifecycle_gates`、`agent_lineages`。
   - backfill root agent/frame from `LifecycleRun.session_id` and `ExecutorRunRef::AgentSession`。
   - 质量门：修复 `SessionMeta.project_id` 持久读写；新增 schema 可支撑删除 `LifecycleRun.session_id` 与迁出 `LifecycleRun.lifecycle_id` 单 graph 假设。

3. `06-01-lifecycle-dispatch-service`
   - 引入 `ExecutionIntent -> ExecutionDispatchResult`，统一 ProjectAgent / Story / Task / Routine execution entrypoint。
   - 质量门：业务入口返回 run、agent、frame、runtime session、gate、subject view refs，不返回 binding owner。

4. `06-01-agent-frame-construction-migration`
   - StepActivation、SessionConstruction、Hook runtime、capability/context/VFS/MCP projection 收束为 AgentFrame construction。
   - 质量门：RuntimeSession launch 只消费 AgentFrame 投影；业务模块不再直接消费 session construction plan。

5. `06-01-workflow-agent-assignment-migration`
   - scheduler / orchestrator / terminal callback 从 session-first 切到 `AgentAssignment`。
   - 质量门：`complete_lifecycle_node` 不通过 session 解析 activity；assignment / attempt / claim key 包含 `graph_instance_id`。

6. `06-01-task-subject-execution-migration`
   - Task start / continue 迁到 `SubjectRef(kind=Task)`。
   - `Task.lifecycle_step_key`、`Task.status`、`Task.artifacts`、`Task.agent_binding` 转为 projection / policy。
   - 质量门：Task 不再保存 runtime truth。

7. `06-01-companion-gate-lineage-migration`
   - Companion wait/adoption 改为 durable `LifecycleGate`。
   - Companion child agent 进入 `LifecycleAgent` / `AgentFrame` / `AgentLineage`。
   - 质量门：companion resume 不依赖 `SessionMeta.companion_context` 作为事实源。

8. `06-01-routine-run-source-migration`
   - RoutineExecution 保存 dispatch truth。
   - terminal status 从 LifecycleAgent / LifecycleRun projection 派生。
   - 质量门：routine 不从 session-first run lookup 推导业务状态。

9. `06-01-frontend-actor-subject-views`
   - 引入 `LifecycleRunView`、`SubjectExecutionView`、`AgentFrameRuntimeView`、`ProjectActiveAgentsView`。
   - `/session/:id` 降级为 `RuntimeTraceView`。
   - 质量门：frontend 不以 `runsBySessionId` 作为 lifecycle 主索引。

10. `06-01-session-first-api-demotion`
   - 删除 `LifecycleRunRepository::list_by_session` 主路径。
   - 删除 route-local `SessionBinding*` response。
   - 删除 `LifecycleRun.session_id` contract 暴露与 session-first run API。
   - 质量门：`rg` 不再命中作为事实源的 session/binding/step 主锚点。

## 子任务交接物

| 子任务 | 必须产出 | 交给后续任务的内容 | 不应承担 |
| --- | --- | --- | --- |
| `06-01-session-lifecycle-spec-convergence` | 更新后的 durable specs 与术语约束 | 后续实现可引用的 `RuntimeSession`、`LifecycleRun`、`WorkflowGraph`、`AgentProcedure`、`AgentFrame` 等定义 | 不做 schema / API 实现 |
| `06-01-session-lifecycle-target-anchors-schema` | 目标 tables/entities/repositories/backfill；最小 query contract | `WorkflowGraphInstance`、`LifecycleAgent`、`AgentFrame`、`AgentAssignment`、`LifecycleSubjectAssociation`、`LifecycleGate`、`AgentLineage` 的持久锚点 | 不切业务入口；不完成 frame construction |
| `06-01-lifecycle-dispatch-service` | `ExecutionIntent`、`ExecutionDispatchResult`、same-run/linked-run 判定、首个业务入口接入 | 创建或复用 run/graph instance/agent/frame/runtime/gate/subject refs 的统一入口 | 不拥有 frame internals；不直接暴露 connector DTO |
| `06-01-agent-frame-construction-migration` | frame builder、frame revision policy、RuntimeLaunchRequest projection、hook runtime frame scope | `AgentFrame` 成为 capability/context/VFS/MCP/runtime refs 的唯一事实源 | 不决定 subject association；不推进 activity terminal |
| `06-01-workflow-agent-assignment-migration` | scheduler/orchestrator/terminal callback 的 assignment route | activity attempt 可通过 assignment 找到 agent/frame/runtime evidence | 不迁 Task/Companion/Routine 业务语义 |
| `06-01-task-subject-execution-migration` | Task start/continue 改为 `SubjectRef(kind=Task)` dispatch；Task projection source refs | Task 页面/API 可以读 subject execution projection | 不新增 Task-owned runtime 字段 |
| `06-01-companion-gate-lineage-migration` | durable gate、agent lineage、same-run companion graph policy | companion wait/resume/adoption 不依赖 `SessionMeta.companion_context` | 不把所有 companion graph 默认拆成 child run |
| `06-01-routine-run-source-migration` | RoutineExecution source association 与 dispatch policy | routine status 从 lifecycle/agent projection 派生 | 不把 dispatch success 当 terminal completion |
| `06-01-frontend-actor-subject-views` | generated target views、normalized run/subject/agent/frame stores、RuntimeTrace route | UI 从 subject/agent/run 进入，session 页面只做 trace drill-down | 不保留 `runsBySessionId` 作为主索引 |
| `06-01-session-first-api-demotion` | 删除 legacy fields/APIs/DTOs；最终 contract scan | 旧 session-first / binding / step / single-graph 入口消失 | 不再引入替代 compatibility endpoint |

## 启动门禁

- `target-anchors-schema`、`lifecycle-dispatch-service`、`agent-frame-construction-migration`、`workflow-agent-assignment-migration` 必须在 start 前拥有 `design.md` 与 `implement.md`。
- 每个子任务 PRD 必须写明：推进的蓝图阶段、输入依赖、必须产出的交接物、明确不承担的边界。
- 每个子任务完成时，必须能用 `target-state-blueprint.md` 的标准谓词说明它新增或切换了哪条事实链。
- 若某个子任务故意让系统中间不可用，需要在自己的 `implement.md` 写出断裂点，以及由哪个后续任务恢复。

## 父任务验证

- `python ./.trellis/scripts/task.py validate 06-01-session-lifecycle-control-plane-refactor`
- `python ./.trellis/scripts/task.py validate <child-task-dir>` for each child
- `git diff --check -- .trellis/tasks`

## 实施入口

用户已确认三项 P0 baseline：

- 删除而不是保留 `LifecycleRun.session_id`。
- `LifecycleSubjectAssociation` 只允许 run / LifecycleAgent anchor。
- `AgentFrame` 首版持久化采用 revision row。

父任务可以进入 implementation，但代码改动应从 `06-01-session-lifecycle-spec-convergence` 开始。
