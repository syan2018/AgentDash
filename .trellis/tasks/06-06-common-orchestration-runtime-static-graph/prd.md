# Common Orchestration Runtime Static Graph

## 目标

在 `workflow-graph-compiler` 能稳定输出 `OrchestrationPlanSnapshot` 后，将 runtime 正式收敛到 common orchestration runtime：以 `LifecycleRun.orchestrations[]` 中的 `OrchestrationInstance` 作为唯一运行实例、runtime node state、state exchange snapshot、journal cursor 和 projection 事实源。静态 `WorkflowGraph` 只是当前 compiler 输入之一；未来 `WorkflowScriptDefinition`、Run script artifact 或其它资产也应编译到同一 plan/runtime 合同。

本任务不实现 dynamic script runtime。它的目标是先把正式运行时路径收敛干净：静态 graph 通过同一套 Orchestration Plan IR 执行，同时拆除 `WorkflowGraphInstance`、Activity-specific repository 和 activity attempt 坐标对 runtime command path 的拥有关系。

## 目标校准

父任务的原始目标是用 research 目录下两份 Claude Workflow 资料作为架构压力测试，确认 AgentDash 的 Lifecycle / Orchestration 框架能承载脚本化编排、隔离运行时、typed execution、journal/cache/snapshot、权限/预算和进度观察等核心行为族。本子任务只处理其中的 runtime 地基：静态 `WorkflowGraph` 先编译到 `OrchestrationPlanSnapshot`，再由同一个 Orchestration runtime 执行。动态脚本、脚本资产审批和保存为 workflow 是后续 compiler frontend；不能在本子任务里引入平行 runtime。

本任务的判定口径是：所有运行态推进都能解释为 `LifecycleRun.orchestrations[]` 中某个 `OrchestrationInstance` 的 plan/node/state exchange 变化。Graph-compatible view 可以继续生成给现有前端观察，但不能重新成为 command path 的事实源。

## 前置条件

- `orchestration-domain-contract` 已完成并使用 `plan_digest` 作为 plan snapshot 内容身份。
- `workflow-graph-compiler` 已完成，能把现有 `WorkflowGraph` 编译成 semantic `OrchestrationPlanSnapshot`。
- compiler fixtures 已覆盖 AgentCall / Function / LocalEffect / HumanGate、transition control、state exchange、join/iteration、bounded loop 与 diagnostics。

## 需求

- 新增 application 层 `OrchestrationRuntime` 或等价模块，输入是 `OrchestrationPlanSnapshot` 和 `OrchestrationInstance`，不是 `WorkflowGraph` 或 `WorkflowGraphInstance`。
- `orchestration_id` 是唯一运行实例身份；definition source / asset provenance 只作为可选审计信息或 plan metadata，不参与 runtime identity。
- 初始化 orchestration：从 plan entry rules materialize ready runtime nodes。静态 graph、后续脚本和其它资产都走同一 activation 规则。
- 用 `RuntimeNodeState` 表达 node status、attempt、inputs、outputs、executor refs、trace refs、error 与 cache refs。
- 将 transition control、condition、join、retry/iteration、state exchange materialization 落在 orchestration snapshot / journal facts 上。
- 将 Agent / Function / LocalEffect / HumanGate executor launch 适配到 semantic plan nodes，保留现有 AgentRun / FunctionRun / HumanDecision / local effect 的 typed execution identity。
- 将 scheduler 的业务事实源迁到 `LifecycleRun.orchestrations[]`。如果仍需要 durable lease/outbox，必须保持它是 operational lease，不是 node state 第二事实源。
- 将 runtime session terminal resolver 从 activity attempt 坐标升级为 lifecycle / orchestration / node / agent / frame 坐标。
- 生成 graph-compatible `LifecycleRunView` projection，保证当前前端仍能观察静态 workflow run。
- 移除或停止读取旧 Activity runtime truth path，避免新旧 snapshot 双读 fallback。
- 拆除 `WorkflowGraphInstanceRepository` / `lifecycle_workflow_instances` / `ActivityBindingRefs(graph_instance_id, activity_key, attempt)` 对 command path 的拥有关系；必要的旧视图字段只能由 orchestration projection 派生。

## 非目标

- 不实现 dynamic JS/TS workflow script compiler。
- 不引入平行 scheduler。
- 不把 session events 当作 orchestration journal。
- 不把 `LifecycleRun.view_projection` 当作 command input。
- 不做长期兼容层；项目未上线，迁移应直接朝目标事实源收敛。

## 验收标准

- [x] 静态 graph run 初始化后，`LifecycleRun.orchestrations[]` 直接拥有一个 `OrchestrationInstance`，entry semantic node 处于 ready 状态。
- [x] 新 runtime 创建、调度、terminal callback、projection 不创建或读取 `WorkflowGraphInstance` 作为运行实例身份。
- [x] `orchestration_id + node_path + attempt` 替代 `graph_instance_id + activity_key + attempt` 成为 scheduler、executor、terminal 和 trace anchor 的节点坐标。
- [ ] Agent / Function API / BashExec 或本机 effect / Human approval 节点能从 plan node 启动并更新 `RuntimeNodeState`。（已完成 graph-backed entry AgentCall 的 `NodeStarted` materialization；完整 semantic launcher 仍待实现。）
- [ ] Function/local effect 即使同步完成，也记录 started 与 terminal materialization，不绕过 runtime node state。
- [x] transition condition 与 artifact/state exchange 能从已完成 node outputs 物化 successor inputs。
- [ ] join policy、attempt policy、`max_traversals` 至少在 runtime plan/materialization 层有明确执行或 blocking diagnostic，不静默降级。
- [x] session terminal callback 和 `complete_lifecycle_node` 通过 runtime node resolver 推进节点，重复 terminal event 幂等。
- [x] `LifecycleRunView` 能从 orchestration snapshot 生成现有 graph-compatible projection。
- [x] `WorkflowGraphInstanceRepository`、Activity claim/assignment 的事实源职责完成删除或降级为可移除 projection/lease adapter。
- [x] targeted Rust tests、migration guard 和 `git diff --check` 通过。

## 备注

- 父任务：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research`。
- 依赖任务：`.trellis/tasks/06-06-orchestration-domain-contract`、`.trellis/tasks/06-06-workflow-graph-compiler`。
- 研究来源：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/common-runtime-convergence-plan.md`。
