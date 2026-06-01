# Session Lifecycle 控制面重构主线

## 目标

把当前 Session-first / Binding / Step / single-graph 遗留控制面重构为以 `LifecycleRun`、`WorkflowGraphInstance`、`LifecycleAgent`、`AgentFrame`、`LifecycleSubjectAssociation` 为核心的执行模型。这个任务不是再整理文档，而是后续代码重构的父级执行合同：所有 child task 必须按本文目标事实源推进，并删除或降级旧结构。

## 上游讨论任务

本任务承接 `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/`，该任务是上一轮概念 review / discussion 输出；本任务是正式执行主线，不再承担开放式概念发散。

关键来源：

- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/refactor-plan.md`
- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/semantic-inventory.md`
- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/lifecycle-entity-association-map.md`
- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/agent-operation-predicates.md`

## 已确认基线

- `RuntimeSession` 只保留 turn、tool、event、resume、debug、projection、trace lineage，不再承载 Story / Task / Project / Permission ownership。
- `LifecycleRun` 是生命周期容器，不是单个 WorkflowGraph 的 run；一个 `LifecycleRun` 可以包含多个 `WorkflowGraphInstance`。
- `LifecycleRun.session_id` 必须迁出并删除，不允许重命名保留。
- `LifecycleSubjectAssociation` 只允许 run / `LifecycleAgent` anchor，不允许 Activity / Attempt anchor。
- `AgentFrame` 首版持久化采用 revision row。
- `ActivityAttemptState` 保留为 execution record，不作为 subject anchor。
- Task 不拥有 runtime truth；Task execution 通过 `SubjectRef(kind=Task)` 进入 dispatch。

## 已确认事实

- 当前分支相对 `main` 的既有变化主要是 `06-01-lifecycle-control-plane-concept-alignment` review 文档，没有产品代码实现。
- `session_bindings` 与旧 step lifecycle columns 在数据库层已删除，但 API、frontend type、hook metadata、repository 查询和 Task projection 仍保留 session / binding / owner / step 惯性结构。
- 原始讨论已明确：多 Activity graph 本身不是 child run 判据；同一被追踪执行生命过程中的复杂 graph / subgraph / companion graph 应留在同一个 `LifecycleRun` 内。
- `SessionMeta.project_id` 已在 SPI 和 migration 中存在，但 Postgres session repository 的 `INSERT` / `SELECT` 当前未覆盖它，必须在目标锚点 schema 子任务中优先复核。
- 项目处于预研期，允许 breaking schema / API / generated contract 调整；不做兼容双轨。

## 需求

- 将 [target-state-blueprint.md](./target-state-blueprint.md) 作为最终验收蓝图；所有 child task 必须声明自身推进的 blueprint step。
- 将 [concept-boundaries.md](./concept-boundaries.md) 作为新增概念职责边界；所有实现不得只做重命名式迁移。
- 重构采用 breaking-mode 迁移窗口：允许中间态不可用，不保留兼容双轨，不为短期稳定维护旧入口。
- 建立目标事实源：`lifecycle_workflow_instances`、`lifecycle_agents`、`agent_frames`、`agent_assignments`、`lifecycle_subject_associations`、`lifecycle_gates`、`agent_lineages`。
- 将 `LifecycleRun.session_id`、`LifecycleRunRepository::list_by_session`、`ExecutorRunRef::AgentSession` 的主路径迁到 AgentFrame / AgentAssignment / RuntimeSession ref。
- 将 `WorkflowDefinition` 的目标语义收束为 `AgentProcedure`，将 `ActivityLifecycleDefinition` 的目标语义收束为 `WorkflowGraph`；命名重构可分阶段，但新代码不得继续扩大旧语义。
- 将当前 `LifecycleRun.lifecycle_id` 迁为 root `WorkflowGraphInstance`；后续 Activity state、attempt、assignment、claim 必须能区分 `graph_instance_id`。
- 将 Task runtime 字段迁为 dispatch policy 或 projection：`lifecycle_step_key`、`status`、`artifacts`、`agent_binding` 不再作为运行事实源。
- 将 Project / Story / Task 的 session-first 视图迁为 subject / agent / run / runtime trace 视图。
- 将 companion wait/adoption、hook pending action、permission grant provenance 接入 `LifecycleGate` / `AgentFrame` / `LifecycleSubjectAssociation`。
- 更新冲突 specs，让后续实现任务先读到同一套 durable contract。

## 子任务

- `06-01-session-lifecycle-spec-convergence`: 同步 specs，先锁定正式语义。
- `06-01-session-lifecycle-target-anchors-schema`: 新增目标锚点 schema 与 backfill。
- `06-01-lifecycle-dispatch-service`: 建立 `ExecutionIntent -> ExecutionDispatchResult`。
- `06-01-agent-frame-construction-migration`: 迁移 StepActivation / SessionConstruction / Hook runtime / capability surface 到 AgentFrame。
- `06-01-workflow-agent-assignment-migration`: 迁移 scheduler / orchestrator / terminal callback 到 AgentAssignment。
- `06-01-task-subject-execution-migration`: 迁移 Task execution 入口与投影。
- `06-01-companion-gate-lineage-migration`: 迁移 companion wait/adoption/lineage 到 durable gate 与 agent lineage。
- `06-01-routine-run-source-migration`: 迁移 routine run source 与 terminal projection。
- `06-01-frontend-actor-subject-views`: 重建前端 subject / agent / runtime trace 视图。
- `06-01-session-first-api-demotion`: 删除 session-first API 主路径和 binding DTO。

## 不在范围

- 不保留旧 schema / API / contract 兼容双轨。
- 不把本父任务当成单次巨型改动；实际代码通过 child tasks 分批完成。
- 不在本任务里继续扩写概念 review；概念证据只作为 `design.md` 的执行约束来源。

## 验收标准

- [ ] 所有 child task 的 PRD 明确依赖、改造面、交付物、不承担边界和验收标准。
- [ ] 所有 child task 的 PRD 明确引用 `target-state-blueprint.md` 中的 blueprint step 和退出状态。
- [ ] `target-anchors-schema`、`lifecycle-dispatch-service`、`agent-frame-construction-migration`、`workflow-agent-assignment-migration` 拥有 `design.md` 与 `implement.md`。
- [ ] `design.md` 形成完整结构迁移矩阵，覆盖 Session persistence、construction、connector、hook runtime、workflow domain、API DTO、frontend contracts、permission grants。
- [ ] `target-state-blueprint.md` 锁定最终目标状态，覆盖旧 Lifecycle / Workflow / Activity / Session / Task / Permission / Frontend view 的命名和事实源归属。
- [ ] `concept-boundaries.md` 明确所有新增概念的职责、非职责、不变量和腐化信号。
- [ ] `implement.md` 给出 child task 执行顺序和每阶段质量门。
- [ ] `implement.md` 明确 breaking-mode 迁移窗口，后续实现不以中间态可用性为优先级。
- [ ] 后续实现中不再新增以 session / binding / owner / step 为主锚点的控制面事实源。
- [ ] 最终代码中 `LifecycleRun.session_id`、`LifecycleRunRepository::list_by_session`、`SessionBinding*` route DTO、Task runtime truth 字段、frontend `runsBySessionId` 主索引被删除或降级为 trace/projection。
- [ ] 最终 schema / domain 不再假设 `LifecycleRun` 只能拥有一个 WorkflowGraph；root graph 只是 `WorkflowGraphInstance(role=root)`。
