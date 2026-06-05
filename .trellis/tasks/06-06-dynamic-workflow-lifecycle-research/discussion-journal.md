# Dynamic Workflow / Lifecycle Discussion Journal

## 2026-06-06：初始 review

用户提供 Claude Code Dynamic Workflows 资料后，本轮先做只读 review。初步判断是：Claude Dynamic Workflows 的关键不是普通 subagent fan-out，而是模型生成可审 orchestration script，隔离运行时执行，脚本变量 / journal 承载中间结果，最终只把汇总结果带回主会话。

对 AgentDash 的初始映射是：

- `WorkflowGraph` / Activity lifecycle 已经具备多步骤、多 Agent、artifact binding、transition condition、scheduler claim、runtime trace anchor 等基础。
- 现有模型主要运行已保存的静态 `WorkflowGraph` definition。
- Dynamic Workflows 的动态拓扑、脚本变量、journal/cache/resume、phase tree 和 agent call tree 还没有一等模型。
- 初始建议是避免把一次性动态脚本直接落进 `workflow_graphs`，因为这会污染长期资产。

## 2026-06-06：用户修正的架构原则

用户补充了更强的架构取向：很多过程仓储可能存在过重嫌疑；可以接受把 `WorkflowGraph` 编译成运行时脚本化规则和持久化状态交换快照，使动态工作流和静态工作流拥有完全一致的运行时规则，并借机收敛过度分散的仓储。

这个补充修正了初始 research 中“新增与 WorkflowGraph 并列的 script workflow / orchestration run 模型”的表达。新的方向不是简单并列两套 runtime，而是：

```text
WorkflowGraph definition
  -> compiler
  -> runtime scripted rule plan + state exchange snapshot
  -> common orchestration runtime

Dynamic script
  -> validator / compiler
  -> runtime scripted rule plan + state exchange snapshot
  -> common orchestration runtime
```

这意味着后续正式设计时，第一优先级不是“先支持 JS 脚本执行”，而是先定义 common runtime IR、snapshot/journal 和 repository convergence matrix。静态 graph 编译器应先证明现有 Activity lifecycle 能被同一套 runtime rule 执行；动态脚本只是另一种 definition input。

## 当前共识

- `WorkflowGraph` 不应直接变成脚本，但应可以编译到脚本化 runtime rule。
- 静态 workflow 与动态 workflow 不应各自拥有状态机、scheduler、journal 和 UI。
- `LifecycleRun` / `LifecycleAgent` / `AgentFrame` / `RuntimeSessionExecutionAnchor` 仍是执行身份、权限、归属和 trace 反查的控制面骨架。
- `WorkflowGraphInstance.activity_state`、`ActivityExecutionClaim`、`AgentAssignment`、`LifecycleRun.execution_log`、session events 等需要后续重新审查职责，区分事实源、索引、lease 和 projection。
- 新能力必须从持久化状态交换快照和 journal 开始设计，否则只会在现有分散状态上再叠一层动态 workflow。

## 下一步建议

后续如果进入正式设计，建议新增 `design.md`，先写三张表：

1. Definition input：`WorkflowGraph`、dynamic script、AgentProcedure 的职责边界。
2. Runtime IR：rule、phase、node、agent call、artifact exchange、join、retry、budget、resume cursor 的最小表达。
3. Repository convergence matrix：现有仓储中哪些保留为事实源，哪些降级为 projection / index / lease，哪些可以合并到 runtime snapshot / journal。

在这三张表清晰之前，不建议进入代码实现。
