# Orchestration 领域合同

## 目标

实现最小 `LifecycleRun` orchestration 领域合同、持久化字段、migration 与聚焦 roundtrip 测试，同时不切换当前静态 graph runtime 的事实源。

## 需求

- 新增 Lifecycle 拥有的 orchestration 状态领域合同：
  - `LifecycleContext`
  - `AgentRunRef`
  - `AgentFrameRef`
  - `OrchestrationInstance`
  - `OrchestrationSourceRef`
  - `OrchestrationStatus`
  - `OrchestrationPlanSnapshot`
  - `PlanNode`
  - `PlanNodeKind`
  - `ExecutorSpec`
  - `ActivationRule`
  - `RuntimeNodeState`
  - `RuntimeNodeStatus`
  - `DispatchState`
  - `StateExchangeSnapshot`
  - `OrchestrationJournalFact`
- 将 `LifecycleRun` 扩展为 owning aggregate，新增 `context`、`orchestrations`、`view_projection`。
- 通过 `LifecycleRunRepository` 持久化这些 aggregate 字段，新增 `lifecycle_runs` 列：
  - `context`
  - `orchestrations`
  - `view_projection`
- 新列承载 JSON 文本，但字段名不使用 `_json` / `_jsonb` 后缀；JSON 只是当前 PostgreSQL 存储细节，不是领域概念。
- 保持当前静态 graph runtime 行为。本任务不得把 scheduler、terminal callback、`WorkflowGraphInstance.activity_state`、`ActivityExecutionClaim`、`AgentAssignment` 或 `RuntimeSessionExecutionAnchor` 迁到新 orchestration 合同。
- 本任务不新增 journal 表、runtime trace anchor schema、script asset、compiler、generated DTO 暴露或前端 view 迁移。
- 新增 forward migration，不修改已提交 migration。
- 增加聚焦测试，证明 0、1、多个 `OrchestrationInstance` 的 serde 与 repository roundtrip。

## 验收标准

- [x] Domain value objects 能编译，并按 snake_case / tagged enum 约定完成 serde roundtrip。
- [x] `LifecycleRun::new_control` 与 `LifecycleRun::new_graphless` 初始化空 context / orchestration / projection 字段。
- [x] `LifecycleRun` 提供小型 aggregate 方法：设置 lifecycle context、添加/替换/查找 orchestration instance。
- [x] PostgreSQL `LifecycleRunRepository` 的 create / update / select 能保存和恢复 `context`、`orchestrations`、`view_projection`。
- [x] 新 migration 添加三列并通过 migration guard。
- [x] 测试至少覆盖空 run、单 orchestration run、包含 Agent / Function / Human executor ref 的多 orchestration run。
- [x] 已触及包的现有 Activity runtime 测试与类型检查仍通过。

## 备注

- 父任务：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research`。
- 研究来源：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/orchestration-domain-contract-plan.md`。
- 本任务是已归档 `workflow-graph-compiler` 的合同地基；它提供 compiler 需要的领域合同，但自身不实现 compiler。
