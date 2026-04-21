# Workflow Lifecycle 条件分支与 Fork/Join 设计讨论

## Goal

在 `04-21-workflow-lifecycle-edge-redesign` 完成 flow/artifact edge 分离之后，进一步设计 lifecycle 的**条件分支（conditional branching）**与**显式并行/汇聚（fork/join）**语义。

本任务以**设计讨论**为主，产出：
- 条件分支的 schema 形态（包括条件来源：表达式 / hook / agent tool）
- fork/join 节点/策略的表达方式（join_policy: All / Any / N-of-M？）
- 与现有 flow edge / artifact edge 的组合规则
- 与 hook check / agent 工具链路的集成方式

## 前置依赖

- **前置 task**：[04-21-workflow-lifecycle-edge-redesign](../04-21-workflow-lifecycle-edge-redesign/) 必须先完成
- `LifecycleEdgeKind` 枚举（Flow / Artifact）已存在，本任务在此基础上扩展

## 设计方向锚点（来自前置任务决策 D3）

- Condition **大概率不是孤立 DSL 表达式**，而是要：
  - 暴露给 **agent 工作流工具**（agent 通过工具调用给出"走哪条分支"的信号）
  - 或与 **hook check 流程集成**（hook 的结果决定分支走向）
- 因此 schema 形态可能更偏：
  ```rust
  enum ConditionSource {
      HookResult { hook_key: String, expected: Value },
      AgentDecision { tool_key: String },
      // 将来可能：ArtifactValue { port: String, matcher: ... }
  }
  ```
  而非 `condition: "expr_string"` 形式

## Open Questions（待讨论）

- 条件分支粒度：**edge 级**（每条 flow edge 带 condition）还是 **node 级**（node 带一组 branch 定义）？
- 默认 join 语义：保留 AND（所有入边都 Ready），还是引入 `join_policy: All | Any | First`？
- 显式 fork 节点是否必要？（还是 DAG 天然表达已够用）
- 条件求值时机：step 完成时同步计算？还是异步等 agent/hook 回调？
- Failed 分支的 rollback / compensation 是否本轮讨论？

## Out of Scope（占位）

- 具体实现（本任务仅设计）
- Loop / 循环语义（会牵出 DAG 转 cyclic graph 的重大决定）
- 事务补偿（compensation）语义——可能独立一个 task

## Technical Notes

- 相关前置代码：
  - [agentdash-domain/src/workflow/value_objects.rs](../../../crates/agentdash-domain/src/workflow/value_objects.rs) — `LifecycleEdge` 定义
  - [agentdash-domain/src/workflow/entity.rs](../../../crates/agentdash-domain/src/workflow/entity.rs) — `complete_step` 就绪判定
- Hook 引擎参考：`03-30-hook-external-triggers` 任务（当前 planning 状态）
- Agent 工具能力参考：`04-21-workflow-contract-capabilities`（已归档）
