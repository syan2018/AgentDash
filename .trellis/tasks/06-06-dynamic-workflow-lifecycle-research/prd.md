# Dynamic Workflow 与 Lifecycle Activity 对齐预研

## 目标

沉淀 Claude Code Dynamic Workflows 对 AgentDash Lifecycle Activity / WorkflowGraph 模块的启发与差距判断，并把 research 目录下两份 Claude Code Workflow 资料作为核心行为覆盖与扩展性压力测试，反向检验 AgentDash 的 Lifecycle / Orchestration 目标架构是否真实具备可扩展性。

本任务只保存 research、目标设计和实施计划文档，不进入代码实现。

## 需求

- 记录 Dynamic Workflows 的核心能力：模型生成可审脚本、独立运行时执行、脚本变量承载中间结果、agent 节点扇出/扇入、运行 journal/cache/resume、进度与成本观测。
- 明确本任务的评估目标是覆盖 research 目录下两份 Claude Code Workflow 参考资料描述的核心 workflow 语义，并验证后续扩展能自然落入目标架构；不要求一比一复刻 Claude Code 的命令、路径、UI、限制数值或产品权限选择。
- 建立行为覆盖矩阵：逐项确认 Claude Workflow 的脚本生成、运行前审批、隔离运行时、原语、schema retry、并发/总量限制、pause/resume/stop/restart、cache/journal、权限边界、进度树、成本统计、保存为命令等行为如何落入 AgentDash 目标架构，并明确 AgentRun、FunctionRun、本机 bridge/effect invocation 等不同执行身份的承载方式。
- 对照 AgentDash 当前 WorkflowGraph / LifecycleRun / Activity runtime 模型，说明已有能力、缺口和不能直接复用的边界。
- 给出推荐学习方向：将静态 graph 与动态脚本统一编译到同一套 runtime scripted rule plan 与持久化状态交换快照。
- 记录过程仓储可能过重、runtime state 过度分散的风险，并把后续收敛方向写入 research。
- 明确后续正式设计前必须回答的产品问题，尤其是脚本资产归属、运行态持久化、权限继承、可恢复边界、成本控制和 UI 审批体验。
- 新增 discussion journal，保存本次讨论中的判断变化与用户补充的架构原则。
- 在 research 中补充关键事实来源复核索引，说明后续应该去哪些 spec、源码、migration 和外部资料复核结论。
- 补充正式实现前的 `design.md` 与 `implement.md`，把 API 命名、目标 runtime 模型、迁移阶段、验证命令和风险文件写清楚。
- 当前任务已在用户确认后进入 `in_progress`，本轮除已完成的 session-scoped API 机械迁移外，后续工作继续以任务文档、模块预研和实现前计划为主。

## 验收标准

- [x] 创建新的 Trellis task，避免复用旧的 lifecycle branching 任务上下文。
- [x] 在任务目录内保存本次 research 结论。
- [x] 结论包含 AgentDash 当前模型的证据链和 Claude Dynamic Workflows 的关键差异。
- [x] 结论给出推荐方向、阶段性路线和主要风险。
- [x] 新增讨论 journal，记录研究结论如何被用户补充修正。
- [x] Research 文档包含关键事实来源复核索引。
- [x] 新增目标模型草案，展示当前模型、目标模型、命名和仓储边界。
- [x] 新增 Claude Workflow 行为覆盖矩阵，用两份参考资料检验目标架构是否真实承载。
- [x] 补充 `design.md` 与 `implement.md`，整理正式实现前的设计和实施拆解。
- [x] 经过用户 review 后启动第一批机械迁移。
- [x] 完成 session-scoped AgentRun command API 机械迁移，并提交为独立变更。
- [ ] 后续模块实现继续按子任务 review 后启动。

## 文档索引

| 文档 | 职责 |
| --- | --- |
| `prd.md` | 任务入口、验收标准、文档索引和当前 planning gate。 |
| `research.md` | 研究结论总览：Claude Workflow 关键启发、AgentDash 当前差距、目标方向和事实来源索引。 |
| `discussion-journal.md` | 按时间记录用户修正与共识变化，保留为什么收敛到当前模型的上下文。 |
| `target-model-sketch.md` | 概念模型草案：Lifecycle / Orchestration / AgentRun / FunctionRun 命名、关系图和仓储边界。 |
| `design.md` | 正式实现前的设计稿：核心合同、API 命名、数据流、分阶段制作方案和迁移策略。 |
| `implement.md` | 实施拆解：建议子任务、第一批范围、风险文件和验证命令。 |
| `research/claude-workflow-behavior-coverage.md` | Claude Workflow 核心行为覆盖矩阵，用作架构压力测试。 |
| `research/current-code-context.md` | 当前代码事实地图，用于后续设计/实现前快速恢复源码上下文。 |
| `research/follow-up-module-roadmap.md` | 后续模块路线汇总：domain contract、graph compiler、common runtime、trace anchor 与 dynamic script compiler 的启动顺序。 |
| `research/orchestration-domain-contract-plan.md` | `LifecycleRun.context` / `orchestrations[]` / `view_projection` 领域合同、migration 与 repository roundtrip 预研。 |
| `research/workflow-graph-compiler-plan.md` | `WorkflowGraph -> OrchestrationPlanSnapshot` compiler 映射、diagnostics、fixtures 与风险预研。 |
| `research/common-runtime-convergence-plan.md` | common orchestration runtime、snapshot/journal、scheduler、terminal resolver、view projection 与仓储收敛预研。 |
| `research/README.md` | research 子目录索引，说明外部资料副本与研究产物来源。 |
| `research/claude-dynamic-workflows-official-doc-zh-cn.md` | 用户贴入的 Claude 官方 Dynamic Workflows 文档副本。 |
| `research/claude-dynamic-workflows-article-zhihu-simpread.md` | 用户贴入的中文调研文章副本。 |
| `implement.jsonl` / `check.jsonl` | Trellis manifest 文件，列出实现/检查子代理压缩后需要恢复的任务文档、研究文档与 spec。 |
| `task.json` | Trellis task 元数据，当前 status 为 `in_progress`。 |

## 当前 Gate

- 旧任务目录中的历史 branching 设计不作为本任务依据。
- Claude Workflow 行为覆盖矩阵见 `research/claude-workflow-behavior-coverage.md`。后续正式设计必须覆盖核心语义，而不是复制 Claude Code 的全部产品表象；无法落入 `LifecycleRun` / `OrchestrationInstance`，或无法通过 `AgentRun` / `FunctionRun` / 受控本机 effect invocation / `RuntimeTraceAnchor` 等执行与 trace surface 表达的行为，都应视为目标架构缺口。
- `Lifecycle` 是项目核心定义，不应重命名；目标是强化它作为主 Agent 完整上下文容器的职责，并把所有相关 AgentRun 管理在该容器内。
- `Orchestration` 是 Lifecycle 内部状态容器，不与 Lifecycle 平级；同一个 Lifecycle 可以有 0..N 个 `OrchestrationInstance` 同时运行，plan snapshot、node tree、journal、dispatch、cache/resume 都应归入对应 instance。
- 目标模型中的领域职责不等于物理表清单；仓储应按 owning aggregate、读取粒度、写入并发和生命周期拆分，默认优先用 `LifecycleRun.context` / `LifecycleRun.orchestrations[]` 内聚过程状态。
- `_json` / `_jsonb` 不是当前项目的目标命名规范；JSON 文本只是复杂值对象的存储方式。新增目标字段和新增列默认使用无后缀命名。
- 用户贴入的两份 Dynamic Workflows 原文已复制到 `research/` 子目录，后续优先复核任务内副本。
- 当前代码事实地图见 `research/current-code-context.md`，用于后续正式设计前快速恢复 Lifecycle / WorkflowGraph / ProjectAgent / MCP / persistence 上下文。
- 评估当前代码时必须谨慎：Lifecycle / WorkflowGraph 相关实现来自快速重构阶段，只能作为现状事实与迁移来源，不应被默认视为最终目标架构。
- runtime session 入口的 AgentRun command API 目标命名采用 `/sessions/{runtime_session_id}/messages`、`/sessions/{runtime_session_id}/steering`、`/sessions/{runtime_session_id}/pending-messages`；显式 AgentRun 资源管理语境再使用 `/lifecycles/{lifecycle_run_id}/agent-runs`。
- 正式实现入口以 `design.md` 和 `implement.md` 为准；`research.md` 与 `target-model-sketch.md` 记录形成这些方案的研究依据和概念模型。
- 后续模块启动前优先读取 `research/follow-up-module-roadmap.md`，再分别进入 domain contract、compiler、common runtime 三份模块预研文档，避免在压缩后丢失研究上下文。
