# Research 资料索引

本目录保存 Dynamic Workflows 预研任务的外部资料副本、源码事实地图和后续模块研究文档。两份 Claude Workflow 资料来自用户贴入的 attachment；后续如果需要复核最新产品事实，优先查官方 Claude Code 文档。

## 文件

- `claude-dynamic-workflows-official-doc-zh-cn.md`
  - 来源：用户贴入 Codex 的 Claude Code Dynamic Workflows 官方中文文档文本。
  - 原始 attachment：`C:\Users\Syan\.codex\attachments\eb234242-cfb0-41b0-a46b-98ed35c00340\pasted-text.txt`

- `claude-dynamic-workflows-article-zhihu-simpread.md`
  - 来源：用户贴入的 SimpRead 转码中文文章，讨论 Claude Code Dynamic Workflows。
  - 原始 attachment：`C:\Users\Syan\.codex\attachments\79de185a-0bc7-414b-8d05-87a4e2392039\pasted-text.txt`

- `current-code-context.md`
  - 来源：2026-06-06 对 AgentDash 本地源码的复核。
  - 用途：记录当前 Lifecycle / WorkflowGraph / ProjectAgent / MCP / persistence / 本机执行事实与源码位置，便于后续设计前恢复上下文。

- `claude-workflow-behavior-coverage.md`
  - 来源：两份 Claude Dynamic Workflows 资料与 AgentDash 代码/spec review 抽象出的行为矩阵。
  - 用途：作为架构压力测试，记录 AgentDash 应覆盖的核心 workflow 语义，不要求一比一复刻 Claude Code 产品细节。

- `follow-up-module-roadmap.md`
  - 来源：session-scoped API 迁移与三条模块预研后的汇总结论。
  - 用途：给出 `orchestration-domain-contract`、`workflow-graph-compiler`、`common-orchestration-runtime-static-graph`、trace anchor convergence 与 dynamic script compiler 的推荐推进顺序。

- `orchestration-domain-contract-plan.md`
  - 来源：围绕 `LifecycleRun`、`WorkflowGraphInstance`、Activity runtime state、repository mapping 与 migration 的源码/spec review。
  - 用途：定义 `LifecycleContext`、`OrchestrationInstance`、`OrchestrationPlanSnapshot`、`RuntimeNodeState`、`StateExchangeSnapshot` 的第一批实现切片。

- `workflow-graph-compiler-plan.md`
  - 来源：AgentDash graph / activity / compiler surface review。
  - 用途：规划 deterministic `WorkflowGraph -> OrchestrationPlanSnapshot` compiler，把旧 graph 的 flow/artifact 简化规范化为控制流与状态交换两个维度，并覆盖语义节点、fixtures 与 diagnostics。

- `common-runtime-convergence-plan.md`
  - 来源：AgentDash engine / scheduler / executor / orchestrator / persistence review。
  - 用途：规划从 `WorkflowGraphInstance.activity_state` 迁移到 common orchestration runtime snapshot / journal 与 graph-compatible projection 的路线。
