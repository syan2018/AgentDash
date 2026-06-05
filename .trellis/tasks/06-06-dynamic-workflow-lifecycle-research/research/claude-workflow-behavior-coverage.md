# Claude Workflow Behavior Coverage

本文把 research 目录下两份 Claude Code Dynamic Workflows 资料抽象成 AgentDash 目标架构的核心行为覆盖基准。这里的目标不是“参考一下 Claude 的动态编排”，也不是一比一复刻 Claude Code，而是：如果 AgentDash 要证明自身 Lifecycle / Orchestration 框架有足够扩展性，就必须能解释这些 workflow 行为族如何落到本项目的领域模型、运行时规则、持久化和 UI 控制面，并保证后续同类扩展能自然接入。

## 评估原则

- 两份资料是本轮行为参照集：`claude-dynamic-workflows-official-doc-zh-cn.md` 与 `claude-dynamic-workflows-article-zhihu-simpread.md`。
- “核心语义覆盖”指覆盖编排脚本、隔离运行时、typed execution、journal/cache、权限/预算/观察等关键行为，不要求复制 Claude Code 的命令名、目录名、UI 文案、默认限制数值或权限产品选择。
- 若某个行为无法自然落入 `LifecycleRun` / `OrchestrationInstance`，或无法通过 `AgentRun` / `FunctionRun` / 受控 effect invocation / `RuntimeTraceAnchor` 等执行与 trace surface 表达，说明目标架构还不充分。
- 若某个行为只能靠新增平行 runtime 支持，也说明目标架构没有通过可扩展性评估。
- 当前项目仍是预研重构态，现有 `WorkflowGraphInstance.activity_state` 等结构只作为迁移来源，不作为判定目标架构正确性的依据。

## 覆盖矩阵

| Claude Workflow 行为 | 目标架构承载方式 | 当前判断 |
| --- | --- | --- |
| 主 Agent 根据任务生成可审 orchestration script | `RunScriptArtifact` 作为 Lifecycle 内的一次性运行资产；可复用后转为 `WorkflowScriptDefinition` | 可支持；目标文档需继续补脚本审批与资产归属细节 |
| 运行前展示阶段计划与脚本源码，用户批准后启动 | `RunScriptArtifact` + `OrchestrationPlanSnapshot` + approval gate；启动前只创建 draft artifact，不创建正式空历史 | 可支持；必须沿用 ProjectAgent create-plus-first-send 的经验，避免空 Lifecycle 污染 |
| 隔离运行时后台执行，主会话保持响应 | `OrchestrationInstance` 是 Lifecycle 内部状态容器，执行由 orchestration runtime 推进；主 AgentRun 只接收最终或摘要结果 | 可支持；需要明确 orchestration runtime 不等于 RuntimeSession conversation loop |
| 中间结果保留在脚本变量 / journal，而不是进入主 Agent 上下文 | `StateExchangeSnapshot` 保存变量摘要、node outputs、cache refs；`OrchestrationJournal` 保存可恢复 facts；主 AgentRun 只消费最终 projection | 可支持；这是引入 snapshot / journal 的核心理由 |
| `agent(prompt, opts?)` 生成子 agent，带 schema 时自动校验和重试 | `PlanNode(kind=agent_call)` + `AgentInvocation` + `AgentRun`；schema/retry 是 node execution policy | 可支持；需要 IR 显式表达 schema、retry policy、result validation |
| `parallel(thunks)` 屏障并发 | `PlanNode(kind=parallel_group)` + child node dependencies + join policy | 可支持；不能只复用 DAG edge，需表达 barrier semantics |
| `pipeline(items, stages...)` 无屏障流水线 | `PlanNode(kind=pipeline)` 或动态展开为 item-scoped stage nodes；每个 item 可独立推进 | 可支持；这是对现有静态 Activity DAG 的关键扩展点 |
| `phase(title)` 与 `log(msg)` 进度分组和运行日志 | `RuntimeNodeState.phase_path`、progress projection、orchestration log facts | 可支持；UI 不能只显示 Activity graph |
| `workflow(name, args)` 嵌套调用保存的 workflow | `PlanNode(kind=subworkflow)` 创建或引用子 `OrchestrationInstance`，并传入 args | 可支持；需要限制嵌套深度与 parent/child instance 关系 |
| `args` 作为结构化输入传入保存 workflow | `OrchestrationInstance.input` / `PlanActivation.args` | 可支持；必须保留结构化 JSON，不走 prompt 字符串解析 |
| `budget.total` / `budget.remaining()` 控制深度和成本 | Lifecycle budget + orchestration budget counter + node-level token/cost accounting | 可支持；需要把预算作为 runtime rule，而不是 UI 统计补丁 |
| 并发上限，例如最多 16 个并发 agent | `DispatchScheduler` 按 Lifecycle / OrchestrationInstance / executor capacity 做 lease 控制 | 可支持；上限应是配置和 runtime policy |
| 单次运行 agent 总数上限，例如 1000 | `OrchestrationInstance.limits.agent_total` 与 journal 计数 | 可支持；必须是硬约束，防止脚本失控 |
| 暂停 / 恢复整个 workflow | Lifecycle command 写入 `OrchestrationJournal`，scheduler 停止或恢复 claim 新节点 | 可支持；pause/resume 应作用于 instance，也可向上聚合到 Lifecycle |
| 停止整个 workflow 或单个 agent | `OrchestrationInstance` / `RuntimeNodeState` cancel command，关联 `AgentRun` cancellation | 可支持；需要区分取消编排节点与取消 runtime session turn |
| 重启运行中的单个 agent | 针对 `RuntimeNodeState` 创建新 attempt / new `AgentInvocation`，保留旧 attempt trace | 可支持；需要 attempt model，不能只覆写 node state |
| 同会话内恢复，已完成 agent 命中缓存，其余继续运行 | `OrchestrationJournal` + `StateExchangeSnapshot` + agent call cache key | 可支持；AgentDash 可选择比 Claude 更 durable，但最低要支持同 Lifecycle 恢复 |
| 编辑脚本后用同脚本或 run id 重跑，未变 agent 调用命中缓存 | `RunScriptArtifact` revision + compiled plan digest + `AgentInvocation.cache_key` | 可支持；需要把 cache key 建在 agent prompt/schema/tools/model/input 上 |
| 保存成功脚本为可复用命令 | `WorkflowScriptDefinition` 或 Shared Library / Project asset；slash command 属于产品入口，不是领域必需名 | 可支持；资产归属需要设计 |
| 项目级与个人级保存位置、同名优先级 | Project asset / user asset precedence policy | 可支持；AgentDash 可采用自己的 asset scope 命名，但必须有等价优先级规则 |
| 运行进度树展示阶段、agent 数、token、耗时 | `LifecycleRunView.view_projection` 从 orchestration state / journal 投影 | 可支持；现有 Activity attempt projection 不够 |
| 深入查看 agent prompt、最近工具调用和结果 | `RuntimeTraceAnchor` 反查 `RuntimeSession` trace，node view 持有 prompt/result refs | 可支持；anchor 必须带 `orchestration_id` / node path |
| workflow 运行中无普通用户中途输入，只有权限提示可能暂停 | orchestration runtime 不等待 arbitrary user input；需要签核则拆成 human gate 或多段 workflow | 可支持；如果要支持 human gate，必须作为显式 node |
| 脚本本身不能直接访问文件系统或 shell | Claude 的产品边界是脚本不直接访问宿主；AgentDash 的目标边界应是 script runtime 无未建模 raw host access，本机/system bridge 能力必须声明为 `PlanNode(kind=function\|local_effect\|extension_action)` 或等价 effect invocation，并携带 capability、permission、workspace root、audit、trace | 必须支持更广义的受控 effect 模型；当前已有 `FunctionActivityExecutorSpec::BashExec`、`FunctionRunner`、`shell_execute` / `shell_exec`、relay shell exec 与 extension `process.execute` 事实 |
| 子 agent 继承当前工具 allowlist / capability surface | `AgentFrame` 构造从 Lifecycle context 与 current frame 派生 capability policy | 可支持；需要明确继承和收窄规则 |
| shell / network / MCP 权限仍可触发确认 | AgentRun 或 typed effect invocation 的 permission gate / LifecycleGate，不由 script 直接绕过 | 可支持；需要把 permission gate 纳入 Lifecycle control projection |
| token 成本和模型选择可观察 / 可控 | node-level execution profile + budget projection + model routing policy | 可支持；需要在 plan/node 上表达 model route |
| `/deep-research` 类多路搜索、交叉核对、投票、过滤输出 | 只是上述原语组合：parallel fan-out + schema result + verify pipeline + synthesis node | 可支持；可作为架构 smoke test |

## 对目标模型的直接压力

1. `OrchestrationInstance` 必须是复数集合

   Claude workflow 行为里存在子 workflow、暂停恢复、重跑、阶段化执行和可能的多段签核。AgentDash 不能把 Lifecycle 内部编排压成单个 `orchestration_state`。同一 `LifecycleRun` 必须能拥有 0..N 个 `OrchestrationInstance`，每个 instance 有自己的 source、plan snapshot、activation、journal、snapshot、limits 和 status。

2. Runtime IR 必须比静态 DAG 更强

   `parallel()`、`pipeline()`、`workflow()`、schema retry、budget、cache key、dynamic fan-out 都不是简单 activity edge 能自然表达的。静态 `WorkflowGraph` 可以编译为 IR 的子集，但 IR 不能被现有 graph shape 限死。

3. Journal / snapshot 是必要能力

   如果没有 `OrchestrationJournal` 和 `StateExchangeSnapshot`，无法支持恢复、缓存命中、脚本编辑后部分重跑、进度树、token/cost 逐节点统计，也无法把中间结果从主 Agent 上下文中移走。

4. 执行身份必须 typed，而不是只有 Agent

   `agent()` 不是直接创建 session event，也不是伪造 ActivityAttempt。它应创建或复用 Lifecycle 内的 `AgentRun`，再由 `RuntimeTraceAnchor` 把 runtime session trace 关联到 `orchestration_id`、node path、agent run 和 frame。function / bash / API / extension action 等非 Agent 节点则应有 `FunctionRun` 或 effect invocation identity，并同样进入 node state、journal、权限和审计。

5. 权限边界必须在执行 surface / LifecycleGate，而不是脚本 runtime

   脚本只协调平台原语，不拥有未建模 raw host access。Agent 工具调用通过 AgentRun 工具面完成；本机 shell、API request、extension action 等通过受控 effect executor 完成；二者都必须接受现有 permission / capability / workspace root / trace 控制。

## 结论

当前 `LifecycleRun + AgentRun + FunctionRun / effect invocation + OrchestrationInstance + RuntimeTraceAnchor` 的方向可以承载 Claude Workflow 的核心行为语义，但前提是后续 design 把上表行为族作为架构验收。若实现时只做到“脚本可以创建几个 AgentRun”，却没有 plan IR、journal/snapshot、cache key、limits、progress projection、权限 gate、本机 effect 执行边界和保存/复用资产，就没有真正吸收 research 目录下的 Claude Workflow 架构参考，也无法用它证明 AgentDash 框架的可扩展性。
