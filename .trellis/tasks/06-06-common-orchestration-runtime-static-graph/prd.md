# Common Orchestration Runtime Static Graph

## 目标

在 `workflow-graph-compiler` 能稳定输出 `OrchestrationPlanSnapshot` 后，将静态 `WorkflowGraph` 的执行切到 common orchestration runtime：以 `LifecycleRun.orchestrations[]` 中的 `OrchestrationInstance` 作为 runtime node state / state exchange snapshot / journal cursor / projection 的事实源，并把旧 `WorkflowGraphInstance.activity_state` 降级为迁移来源或兼容 projection。

本任务不实现 dynamic script runtime。它的目标是先证明静态 graph 能通过同一套 Orchestration Plan IR 执行，为后续 `RunScriptArtifact` / `WorkflowScriptDefinition` compiler 铺路。

## 前置条件

- `orchestration-domain-contract` 已完成并使用 `plan_digest` 作为 plan snapshot 内容身份。
- `workflow-graph-compiler` 已完成，能把现有 `WorkflowGraph` 编译成 semantic `OrchestrationPlanSnapshot`。
- compiler fixtures 已覆盖 AgentCall / Function / LocalEffect / HumanGate、transition control、state exchange、join/iteration、bounded loop 与 diagnostics。

## 需求

- 新增 application 层 `OrchestrationRuntime` 或等价模块，输入是 `OrchestrationPlanSnapshot` 和 `OrchestrationInstance`，不是 `WorkflowGraph`。
- 初始化静态 graph root orchestration：从 plan entry rules materialize ready runtime nodes。
- 用 `RuntimeNodeState` 表达 node status、attempt、inputs、outputs、executor refs、trace refs、error 与 cache refs。
- 将 transition control、condition、join、retry/iteration、state exchange materialization 落在 orchestration snapshot / journal facts 上。
- 将 Agent / Function / LocalEffect / HumanGate executor launch 适配到 semantic plan nodes，保留现有 AgentRun / FunctionRun / HumanDecision / local effect 的 typed execution identity。
- 将 scheduler 的业务事实源从 `WorkflowGraphInstance.activity_state` 迁到 `LifecycleRun.orchestrations[]`。如果仍需要 durable lease/outbox，必须保持它是 operational lease，不是 node state 第二事实源。
- 将 runtime session terminal resolver 从 activity attempt 坐标升级为 lifecycle / orchestration / node / agent / frame 坐标。
- 生成 graph-compatible `LifecycleRunView` projection，保证当前前端仍能观察静态 workflow run。
- 明确移除或停止读取旧 Activity runtime truth path，避免新旧 snapshot 双读 fallback。

## 非目标

- 不实现 dynamic JS/TS workflow script compiler。
- 不引入平行 scheduler。
- 不把 session events 当作 orchestration journal。
- 不把 `LifecycleRun.view_projection` 当作 command input。
- 不做长期兼容层；项目未上线，迁移应直接朝目标事实源收敛。

## 验收标准

- [ ] 静态 graph run 初始化后，root `OrchestrationInstance` 中 entry semantic node 处于 ready 状态。
- [ ] Agent / Function API / BashExec 或本机 effect / Human approval 节点能从 plan node 启动并更新 `RuntimeNodeState`。
- [ ] Function/local effect 即使同步完成，也记录 started 与 terminal materialization，不绕过 runtime node state。
- [ ] transition condition 与 artifact/state exchange 能从已完成 node outputs 物化 successor inputs。
- [ ] join policy、attempt policy、`max_traversals` 至少在 runtime plan/materialization 层有明确执行或 blocking diagnostic，不静默降级。
- [ ] session terminal callback 和 `complete_lifecycle_node` 通过 runtime node resolver 推进节点，重复 terminal event 幂等。
- [ ] `LifecycleRunView` 能从 orchestration snapshot 生成现有 graph-compatible projection。
- [ ] 新 runtime 不再依赖 `WorkflowGraphInstance.activity_state` 作为推进事实源。
- [ ] targeted Rust tests、migration guard 和 `git diff --check` 通过。

## 备注

- 父任务：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research`。
- 依赖任务：`.trellis/tasks/06-06-orchestration-domain-contract`、`.trellis/tasks/06-06-workflow-graph-compiler`。
- 研究来源：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/common-runtime-convergence-plan.md`。
