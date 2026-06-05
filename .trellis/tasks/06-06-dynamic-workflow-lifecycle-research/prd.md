# Dynamic Workflow 与 Lifecycle Activity 对齐预研

## Goal

沉淀 Claude Code Dynamic Workflows 对 AgentDash Lifecycle Activity / WorkflowGraph 模块的启发与差距判断，并把 research 目录下两份 Claude Code Workflow 资料作为核心行为覆盖与扩展性压力测试，反向检验 AgentDash 的 Lifecycle / Orchestration 目标架构是否真实具备可扩展性。

本任务只保存 research 结论和产品/架构问题，不进入实现。

## Requirements

- 记录 Dynamic Workflows 的核心能力：模型生成可审脚本、独立运行时执行、脚本变量承载中间结果、agent 节点扇出/扇入、运行 journal/cache/resume、进度与成本观测。
- 明确本任务的评估目标是覆盖 research 目录下两份 Claude Code Workflow 参考资料描述的核心 workflow 语义，并验证后续扩展能自然落入目标架构；不要求一比一复刻 Claude Code 的命令、路径、UI、限制数值或产品权限选择。
- 建立行为覆盖矩阵：逐项确认 Claude Workflow 的脚本生成、运行前审批、隔离运行时、原语、schema retry、并发/总量限制、pause/resume/stop/restart、cache/journal、权限边界、进度树、成本统计、保存为命令等行为如何落入 AgentDash 目标架构，并明确 AgentRun、FunctionRun、本机 bridge/effect invocation 等不同执行身份的承载方式。
- 对照 AgentDash 当前 WorkflowGraph / LifecycleRun / Activity runtime 模型，说明已有能力、缺口和不能直接复用的边界。
- 给出推荐学习方向：不要把现有 WorkflowGraph 强行改成脚本语言，而应考虑将静态 graph 与动态脚本统一编译到同一套 runtime scripted rule plan 与持久化状态交换快照。
- 记录过程仓储可能过重、runtime state 过度分散的风险，并把后续收敛方向写入 research。
- 明确后续正式设计前必须回答的产品问题，尤其是脚本资产归属、运行态持久化、权限继承、可恢复边界、成本控制和 UI 审批体验。
- 新增 discussion journal，保存本次讨论中的判断变化与用户补充的架构原则。
- 在 research 中补充关键事实来源复核索引，说明后续应该去哪些 spec、源码、migration 和外部资料复核结论。
- 保持当前任务只读研究性质，不修改代码、不做迁移、不启动 dev runtime。

## Acceptance Criteria

- [x] 创建新的 Trellis task，避免复用旧的 lifecycle branching 任务上下文。
- [x] 在任务目录内保存本次 research 结论。
- [x] 结论包含 AgentDash 当前模型的证据链和 Claude Dynamic Workflows 的关键差异。
- [x] 结论给出推荐方向、阶段性路线和主要风险。
- [x] 新增讨论 journal，记录研究结论如何被用户补充修正。
- [x] Research 文档包含关键事实来源复核索引。
- [x] 新增目标模型草案，展示当前模型、目标模型、命名和仓储边界。
- [x] 新增 Claude Workflow 行为覆盖矩阵，用两份参考资料检验目标架构是否真实承载。
- [ ] 若后续进入实现，应先补充 `design.md` 与 `implement.md`，并经过任务启动流程。

## Notes

- 旧任务目录中的历史 branching 设计不作为本任务依据。
- 详细研究记录见 `research.md`；讨论脉络见 `discussion-journal.md`。
- Claude Workflow 行为覆盖矩阵见 `research/claude-workflow-behavior-coverage.md`。后续正式设计必须覆盖核心语义，而不是复制 Claude Code 的全部产品表象；无法落入 `LifecycleRun` / `OrchestrationInstance`，或无法通过 `AgentRun` / `FunctionRun` / 受控本机 effect invocation / `RuntimeTraceAnchor` 等执行与 trace surface 表达的行为，都应视为目标架构缺口。
- 目标模型草案见 `target-model-sketch.md`，其中建议用 `OrchestrationInstance` 替代 `WorkflowGraphInstance` 的目标语义；`PlanActivation` 是 instance 内部的 plan binding / activation 状态。
- `Lifecycle` 是项目核心定义，不应重命名；目标是强化它作为主 Agent 完整上下文容器的职责，并把所有相关 AgentRun 管理在该容器内。
- `Orchestration` 是 Lifecycle 内部状态容器，不与 Lifecycle 平级；同一个 Lifecycle 可以有 0..N 个 `OrchestrationInstance` 同时运行，plan snapshot、node tree、journal、dispatch、cache/resume 都应归入对应 instance。
- 目标模型中的领域职责不等于物理表清单；仓储应按 owning aggregate、读取粒度、写入并发和生命周期拆分，默认优先用 `LifecycleRun.context` / `LifecycleRun.orchestrations[]` 内聚过程状态。
- `_jsonb` 不是当前项目的领域命名规范；它只应作为明确采用 PostgreSQL `jsonb` 列时的物理实现细节。目标模型文档默认使用无后缀领域字段名。
- 用户贴入的两份 Dynamic Workflows 原文已复制到 `research/` 子目录，后续优先复核任务内副本。
- 当前代码事实地图见 `research/current-code-context.md`，用于后续正式设计前快速恢复 Lifecycle / WorkflowGraph / ProjectAgent / MCP / persistence 上下文。
- 评估当前代码时必须谨慎：Lifecycle / WorkflowGraph 相关实现来自快速重构阶段，只能作为现状事实与迁移来源，不应被默认视为最终目标架构。
