# WorkflowGraph 编译器

## 目标

在 Orchestration 领域合同落地后，实现确定性的 `WorkflowGraph -> OrchestrationPlanSnapshot` 编译器，并将静态 graph definition 接入后续 common runtime 依赖链。本任务已完成实现并归档。

## 需求

- 定义纯函数、确定性的 `WorkflowGraph -> OrchestrationPlanSnapshot` 编译器实施计划。它是 definition 到语义 IR 的编译器，不是把 graph 拼成一段脚本再模拟执行。
- 将 compiler 建立在已归档的 Orchestration 领域合同之上；它是 common runtime 静态 graph 接入的已完成依赖。
- 编译目标必须对齐 Claude Workflow 的核心语法模型：`flow` 表达过程控制、顺序、分支、并发与屏障；`artifact` 表达脚本变量、节点输出、输入绑定和可恢复状态交换。现有 graph 的 flow edge / artifact edge 只是早期编辑器简化，不能成为目标 IR 的上限。
- 在 plan IR 中保留当前 public `WorkflowGraph` 语义：
  - `entry_activity_key`
  - Agent / Function / Human Activity executor identity
  - input / output ports
  - completion policy
  - transition condition
  - artifact binding
  - join policy
  - iteration policy
  - transition `max_traversals`
- 明确当前 runtime 缺口：即使旧 Activity runtime 尚未完整执行 `join_policy`、`artifact_alias`、`max_traversals`，compiler 也必须把这些语义保留进 plan。
- 定义 pathful diagnostics，覆盖 invalid graph shape、unsupported executor policy、dangling refs、legacy edge normalization conflict、unbounded cycle。
- 第一版编译器推荐放在 `agentdash-application`，domain 只保留 IR/value object。编译器虽然纯粹，但它负责把一种应用层 definition 资产规范化成运行计划，不应把旧 `WorkflowGraph` 形态继续固化到 domain 服务里。
- plan identity 使用内容 digest。`Uuid` 只用于 `OrchestrationInstance`、run、agent run 等运行实例；不可变 plan snapshot 的身份应由 canonical source + compiler schema 计算得出。
- graph activity 应编译成语义节点：Agent executor -> `AgentCall`，API request -> `Function`，BashExec / 本机桥接 -> `LocalEffect`，Human approval -> `HumanGate`。`Activity` 仅作为 legacy/source projection 或兼容元数据保留。
- 实现包含 fixture 覆盖。
- 编译器保持纯粹：不读 repository、不创建 run、不启动 agent/session、不做权限授权、不 materialize scheduler/runtime state。

## 验收标准

- [x] PRD、design、implement plan 和 context manifests 完整。
- [x] 任务完成实现并归档，作为 common runtime 静态 graph 接入的依赖。
- [x] design 明确要保留的 source semantics 与非目标。
- [x] implement plan 列出模块位置选择、诊断、fixtures 和验证命令。
- [x] 任务上下文指向父任务 compiler research、当前代码事实地图和相关 specs。

## 备注

- 父任务：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research`。
- 依赖：`.trellis/tasks/archive/2026-06/06-06-orchestration-domain-contract`。
- 研究来源：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/workflow-graph-compiler-plan.md`。
- 完成状态：已实现并归档；后续 dynamic script compiler 继续复用同一 `OrchestrationPlanSnapshot` 目标。
