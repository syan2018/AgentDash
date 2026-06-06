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
- `LifecycleRun` 是完整上下文容器；`LifecycleAgent` 的目标命名应收敛为 `AgentRun`；`AgentFrame` / `RuntimeSessionExecutionAnchor` 仍是执行身份、surface revision 和 trace 反查的控制面骨架。
- `WorkflowGraphInstance.activity_state`、`ActivityExecutionClaim`、`AgentAssignment`、`LifecycleRun.execution_log`、session events 等需要后续重新审查职责，区分事实源、索引、lease 和 projection。
- 新能力必须从持久化状态交换快照和 journal 开始设计，否则只会在现有分散状态上再叠一层动态 workflow。
- 当前实现是快速重构后的过渡形态。后续评估应把代码当作“现有事实与迁移约束”，而不是把现有仓储拆分、状态权威性或命名边界视作最终答案。

## 下一步建议

本轮已补充 `research/current-code-context.md`，把当前 Lifecycle / WorkflowGraph / ProjectAgent / MCP / persistence 代码链路按事实索引保存。后续讨论实现时，应先以这份上下文恢复现状，再判断哪些结构是可保留的控制面骨架，哪些只是快速重构阶段的过渡状态容器。

用户进一步确认“先做静态 WorkflowGraph -> common runtime IR/snapshot 的迁移设计”是合适的，并指出 `WorkflowGraphInstance` 在新概念下过于强调 Graph。已新增 `target-model-sketch.md`，建议目标运行时从 `GraphInstance` 转向 `OrchestrationInstance` / `PlanActivation` / `RuntimeNodeState` / `OrchestrationJournal`：`WorkflowGraph` 和 dynamic script 都是 definition input，编译后的 `OrchestrationPlanSnapshot` 才是 internal orchestration runtime 输入。

用户继续修正仓储方向：上一版目标模型把逻辑职责列成多组 store，容易继续制造过度拆分。新的仓储原则是按 owning aggregate、读取粒度、写入并发和生命周期拆分；`OrchestrationInstance`、`PlanActivation`、node tree、agent invocation、dispatch state、artifact refs、progress projection 等默认归入 `lifecycle_runs` 的结构化 aggregate。只有无界 append journal、runtime session 反向索引、独立资产生命周期或被实际并发/查询压力证明的边界，才拆物理表。

用户继续明确命名边界：`Lifecycle` 是项目核心定义，不能重命名。它是把主 Agent 以及派生/协作 AgentRun 管进同一个共同生命周期容器的方案，核心仍面向主 Agent。已修正 `target-model-sketch.md`，撤回 `OrchestrationRun` / `RunAgent` 建议，保留 `LifecycleRun` 作为顶层容器，将 `LifecycleAgent` 的目标命名收敛为 `AgentRun`，并仅在 Lifecycle 内部引入 `OrchestrationInstance`、`OrchestrationPlanSnapshot`、`PlanActivation`、`RuntimeNodeState` 等状态概念。

用户进一步精确分层：`Lifecycle` 是全部上下文容器，`Orchestration` 是内部状态容器。随后又指出目标模型必须体现一个 Lifecycle 内可以有多个 orchestration 实例同时运行，并质疑 JSON 存储后缀是否只是前文惯性。已修正为：`LifecycleRun.orchestrations[]` 是内部状态实例集合，单个元素叫 `OrchestrationInstance`；plan activation、node tree、dispatch lease、journal/cache/resume 放入对应 instance；subject、主 Agent、AgentRun、AgentFrame、权限、trace 归属仍属于 Lifecycle context。新增目标字段和新增列统一不使用 `_json` / `_jsonb` 后缀。

后续正式设计已经沉淀为 `design.md` 与 `implement.md`。这两份文档应保持三类信息清晰：

1. Definition input：`WorkflowGraph`、dynamic script、AgentProcedure 的职责边界。
2. Runtime IR：rule、phase、node、agent call、artifact exchange、join、retry、budget、resume cursor 的最小表达。
3. Repository convergence matrix：现有仓储中哪些保留为事实源，哪些降级为 projection / index / lease，哪些可以合并到 runtime snapshot / journal。

在这三类信息经过 review 前，任务保持 planning 状态。

## 2026-06-06：覆盖基准与本机执行边界修正

用户进一步修正了本轮行为覆盖基准：AgentDash 不是要一比一复刻 Claude Code Dynamic Workflows，而是用两份资料约束目标架构是否足够清晰、可扩展、经得起复杂 workflow 压测。覆盖重点应放在脚本化编排、隔离运行时、typed execution、journal/cache/snapshot、权限/预算/观察这些核心语义，以及后续同类扩展是否能自然落入模型；Claude 的命令名、目录名、UI 细节、默认并发数和具体产品权限选择不应成为硬目标。

用户还指出“脚本本身不能直接访问文件系统或 shell”不能机械套用到 AgentDash。当前项目 workflow 已经支持走系统桥接的本机执行：`FunctionActivityExecutorSpec::BashExec`、`FunctionRunner`、`shell_execute` / `shell_exec`、relay shell exec、extension `process.execute` 都是现有事实。因此更合适的目标边界是：script runtime 不拥有未建模 raw host access；本机执行、API request、extension action 等必须作为受控 function/local effect node 或 effect invocation 进入 permission、workspace root、audit、trace、journal，而不是被排除在 workflow runtime 之外，也不是伪装成 AgentRun。

由此目标模型需要从“AgentRun 承接所有副作用”修正为 typed execution identity：`agent()` 落到 `AgentRun`，function / bash / API / extension action 落到 `FunctionRun` 或更通用的 `RuntimeEffectInvocation`，两者都归属于 `LifecycleRun` 内的某个 `OrchestrationInstance` 与 `RuntimeNodeState`。

## 2026-06-06：Session-scoped command API 命名

用户指出 `/runs/by-runtime-session/{runtime_session_id}/...` 仍然偏长，且 `/run` 这一层语义不够明确。重新复核当前 `lifecycle_agents.rs` 与前端 `lifecycle.ts` 后，确认这些端点的入口事实是 runtime session：API 先用 `runtime_session_id` 找 `RuntimeSessionExecutionAnchor`，再解析到 `LifecycleRun` / `LifecycleAgent` / `AgentFrame` 执行权限检查与消息投递。

因此目标命名调整为 session-scoped command API：

```text
POST   /sessions/{runtime_session_id}/messages
POST   /sessions/{runtime_session_id}/steering
GET    /sessions/{runtime_session_id}/pending-messages
POST   /sessions/{runtime_session_id}/pending-messages
DELETE /sessions/{runtime_session_id}/pending-messages/{message_id}
POST   /sessions/{runtime_session_id}/pending-messages/{message_id}/promote
```

`/sessions/{runtime_session_id}` 最清楚地表达了用户对当前 runtime session 的 delivery/control command。AgentRun / LifecycleRun 的写入归属属于 application service 内部解析结果；若后续需要显式管理 Lifecycle 内 AgentRun 资源，使用 `/lifecycles/{lifecycle_run_id}/agent-runs`。

## 2026-06-06：WorkflowGraph compiler 语义修正

用户进一步指出，Claude Workflow 参考的关键不是把 graph 用一段脚本模拟出来。Claude Workflow 的脚本里，`flow` 是命令式过程控制，`artifact` / 中间结果是变量和状态交换；一次运行轨迹可以投影成 DAG，但编排程序本身不应被静态 graph 形态限制。AgentDash 现有 flow edge / artifact edge 是快速实现阶段的简化，不能成为目标 IR 的上限。

据此修正 compiler 计划：

- `WorkflowGraph -> OrchestrationPlanSnapshot` 是 definition 到语义 IR 的编译器，不是 graph-to-script。
- compiler 推荐放在 application 层；domain 层持有 `OrchestrationPlanSnapshot`、`PlanNode`、`ActivationRule`、`ExecutorSpec` 等值对象和不变量。
- plan snapshot identity 使用 deterministic digest；UUID 留给 `OrchestrationInstance`、LifecycleRun、AgentRun 等运行实例。
- graph activity 进入 runtime IR 时按 executor 变成语义节点：Agent activity -> `AgentCall`，API request -> `Function`，BashExec / 本机桥接 -> `LocalEffect`，Human approval -> `HumanGate`。`Activity` 只保留为 source metadata 或兼容 projection。
- 旧 `ActivityTransitionKind::Flow` / `Artifact` 只作为 source metadata 和 normalization hint。目标 runtime 中，control dependency / condition / join / traversal limit 是过程控制维度；artifact binding / node output / input materialization 是状态交换维度。

这也暴露出已落地第一版 domain contract 的后续修正点：当前 `OrchestrationPlanSnapshot` 仍有 `plan_id: Uuid`。在 compiler 实现前，应先把 plan snapshot 的内容身份收敛到 digest，避免后续 cache/resume/audit 被随机 ID 污染。
