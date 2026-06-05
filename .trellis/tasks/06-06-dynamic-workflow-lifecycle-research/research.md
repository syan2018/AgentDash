# Dynamic Workflow 与 Lifecycle Activity 对齐预研

## 背景

用户提供的 Claude Code Dynamic Workflows 资料显示，该能力不是传统静态 DAG，也不是普通 subagent fan-out。它的核心是：Claude 先为当前任务生成一段可审 JavaScript orchestration script，隔离运行时在后台执行脚本，脚本通过 `agent()`、`parallel()`、`pipeline()`、`phase()`、`log()`、`workflow()` 等原语调度子 agent，并把循环、分支、中间结果保留在脚本变量或 workflow journal 中，主对话上下文主要接收最终汇总。

本任务的目标口径已收敛为：覆盖 research 目录下两份 Claude Code Workflow 参考资料描述的核心 workflow 语义，并以此鉴定 AgentDash 当前框架与目标架构的可扩展性。后续不是“借鉴一点动态编排”，也不是一比一复刻 Claude Code 的命令、路径、UI 或限制数值，而是逐项验证这些行为家族能否真实落入 `LifecycleRun` / `OrchestrationInstance`，并通过 `AgentRun`、`FunctionRun`、受控本机 effect invocation、`RuntimeTraceAnchor` 等执行与 trace surface 表达。

这与 AgentDash 当前 Lifecycle Activity 模块相似之处在于：二者都试图把多 Agent、多步骤工作从主会话上下文中拆出来，并提供运行态可观察性。差异在于：AgentDash 当前主要运行已保存的 `WorkflowGraph` / Activity definition；Dynamic Workflows 支持由模型按任务现场生成脚本，再由确定性运行时动态展开执行轨迹。

## Dynamic Workflows 值得学习的点

1. 编排逻辑代码化

   关键价值不是“多开几个 agent”，而是把循环、分支、重试、汇总、交叉验证等编排策略从主 Agent 的逐轮上下文里移到可读、可审、可保存的脚本中。

2. 中间结果不污染主上下文

   子 agent 的完整输出先进入脚本变量或运行 journal，脚本做过滤、投票、去重和汇总后，只把最终产物交还主会话。这正好对应 AgentDash 对 runtime trace、projection、control ledger 分层的长期方向。

3. 运行时负责工程约束

   官方能力内置并发上限、agent 总数上限、schema 校验重试、进度树、token 统计、暂停/恢复、失败 agent 重启和同会话缓存。这些工程边界比 prompt 约束可靠。

4. 代码可见与运行前审批

   脚本生成后用户能审查，运行过程也能查看阶段、agent prompt、工具调用和结果。AgentDash 若引入同类能力，审批与可观察性应是一等能力，而不是日志补丁。

5. 保存为可复用命令

   一次任务生成的 orchestration script 可以保存到项目级或个人级位置，之后作为命令复用。这和 AgentDash Shared Library / Project assets 可以形成自然连接。

## AgentDash 当前相关模型

当前权威模型大致是：

- `AgentProcedure`：单个 Agent Activity 的 behavior / capability / context / hook / port contract。
- `WorkflowGraph`：可执行 Activity graph definition，包含 `entry_activity_key`、`activities`、`transitions`。
- `LifecycleRun`：tracked life process / control ledger。
- `WorkflowGraphInstance`：一个 run 内 graph 生效实例与 activity state namespace。
- `ActivityLifecycleRunState` / `ActivityAttemptState`：activity attempt 状态、outputs、inputs。
- `ActivityExecutionClaim`：scheduler 对 ready attempt 的 durable claim。
- `AgentAssignment`：activity attempt 与 `LifecycleAgent` / `AgentFrame` 的绑定。
- `RuntimeSessionExecutionAnchor`：runtime trace 反查 run / agent / frame / assignment / attempt 的权威索引。

实现链路上，`LifecycleDispatchService` 会先物化 run、graph instance、agent、frame、runtime session anchor，再由 `ActivityLifecycleRunService` / `LifecycleEngine` / `ActivityExecutorScheduler` 根据已保存的 `WorkflowGraph` 推进 attempt。前端 `workflowStore` 和 `LifecycleDagCanvas` 维护的是 definition draft 与 ReactFlow 图编辑器；MCP `WorkflowMcpServer` 暴露 `upsert_workflow_tool` 与 `upsert_lifecycle_tool`，允许具备 workflow_management 能力的 Agent 创建或更新项目级定义。

## 关键差距

1. 现有 `WorkflowGraph` 是项目资产，不是一次运行的脚本

   `workflow_graphs` 按 project/key 唯一存储，适合可复用定义，不适合每次任务临时生成脚本。若让 Agent 为每次复杂任务 upsert 一个 graph，会污染官方资产列表，并留下大量短生命周期定义。

2. 现有运行态依赖已知 topology

   `WorkflowGraphInstance.activity_state` 在初始化时根据 definition 创建 entry attempt 和 pending attempts。Dynamic Workflows 的拓扑是脚本运行时根据上一步结果动态展开，不能直接塞进当前 `activities + transitions` 静态结构。

3. 当前动态能力是 transition 层，不是 orchestration 层

   代码已经支持 condition、artifact binding、max_traversals、max_attempts 和 bounded loop 的迹象，前端也会提示未设阈值的环。这能表达重试/回评，但不能表达“根据上一批 agent 输出动态生成下一批 agent prompt 和数量”。

4. 当前没有 script journal / cache 模型

   Dynamic Workflows 的同会话恢复依赖 run journal 和 agent call 缓存。AgentDash 当前有 session events、compaction projection、lifecycle execution log、activity claim，但没有面向脚本 step 的 cache key、变量快照、phase 状态、agent call result store。

5. 当前没有统一的脚本运行时与本机 effect 边界

   Claude 的脚本本身没有直接文件系统或 shell 访问，这是 Claude Code 的产品安全边界。AgentDash 现有 workflow 已经支持 `FunctionActivityExecutorSpec::ApiRequest` / `BashExec`、`FunctionRunner`、`shell_execute` / `shell_exec`、relay shell exec 以及 extension `process.execute`。因此目标模型不应把本机执行排除在 workflow 之外，而应要求脚本不能拥有未建模的 raw host access；本机执行必须作为受控 `PlanNode(kind=function|local_effect|extension_action)` 或等价 executor/effect invocation 进入权限、workspace root、审计、trace 与 journal。

6. 当前进度 UI 以 lifecycle/activity 为中心

   Dynamic Workflows 需要 phase tree、agent call tree、token/cost、暂停/恢复、重启单个 agent、查看 prompt/result。现有 lifecycle run view 可以承载一部分，但不能完整表达脚本变量和 pipeline 状态。

## 修正后的推荐方向

不要把现有 `WorkflowGraph` 直接改造成脚本 workflow，但也不应该让静态 graph runtime 和动态 script runtime 形成两套运行规则。更稳妥的方向是引入一个统一编译层：

```text
WorkflowGraph definition
  -> Workflow compiler
  -> Runtime scripted rule plan + state exchange snapshot
  -> Common orchestration runtime

Dynamic workflow script
  -> Script validator / compiler
  -> Runtime scripted rule plan + state exchange snapshot
  -> Common orchestration runtime
```

目标不是“图一套、脚本一套”，而是让静态 workflow 与动态 workflow 在 runtime 层共享同一种执行规则、状态快照、journal、调度、权限和观察模型。`WorkflowGraph` 仍然可以保留为可视化、可治理、长期复用的定义态；动态脚本则作为另一种定义态或一次性 run artifact。二者都应编译到同一份 runtime IR，而不是各自拥有仓储、状态机和 scheduler。

这个方向接受用户补充的判断：当前不少过程仓储有过重嫌疑，尤其是 `WorkflowGraphInstance.activity_state`、`ActivityExecutionClaim`、`AgentAssignment`、`RuntimeSessionExecutionAnchor`、`LifecycleRun.execution_log`、session events 等状态分散后，后续复杂 workflow 会更难恢复、调试和收敛。引入脚本化规则时应顺手审视这些仓储的边界，把“过程状态”收敛到少数明确职责。这里的职责不是物理表清单；能由 owning aggregate 按读取粒度用结构化 JSON 管好的状态，应优先内聚在 run aggregate 内：

- Definition store：保存 `WorkflowGraph`、可复用 script workflow、AgentProcedure 等定义态资产。
- Runtime plan：保存编译后的 runtime scripted rule plan，作为静态 graph 与动态 script 的共同执行输入；早期可直接作为 `lifecycle_runs.orchestrations[].plan_snapshot`。
- Runtime snapshot / journal：保存状态交换快照、agent call result、phase 进度、cache key、resume cursor 和调度事件；snapshot 优先内聚在 run aggregate，只有无界 append journal 或实际查询压力才拆表。
- Lifecycle context：由 `LifecycleRun` 作为完整上下文容器，管理 subject、project、主 Agent、所有 AgentRun、权限、预算、frame refs 与 trace refs。
- Orchestrations：由 `LifecycleRun.orchestrations[]` 承载 0..N 个 `OrchestrationInstance`。每个 instance 承载编译后的 plan activation、runtime node tree、dispatch、agent invocation、cache/cursor、artifact exchange；它们是 Lifecycle 内部状态容器，不是顶层 run。

这样可以保留 AgentDash 已经正确建立的 lifecycle / agent / frame / runtime anchor 控制面，同时引入 Claude Dynamic Workflows 的核心能力：编排代码化、运行时约束、可恢复 journal 和可审进度树。关键收敛点是：`WorkflowGraph` 不直接等于 runtime state；它应先编译成可执行规则，再与动态脚本共享同一个 runtime。

目标命名与仓储草案见 `target-model-sketch.md`。该草案已修正为：`Lifecycle` 是项目核心概念，不重命名；`LifecycleRun` 是面向主 Agent 的完整上下文容器；`OrchestrationInstance` 是 Lifecycle 内部可 0..N 并发存在的状态容器，内部用 `OrchestrationPlanSnapshot`、`PlanActivation`、`RuntimeNodeState`、`OrchestrationJournal` 表达编排运行态。仓储上不建议按每个概念拆表，而是以 `lifecycle_runs` 作为主要 aggregate，辅以 append journal 和必要反向索引。领域字段不使用 `_jsonb` 后缀；物理列名和列类型留给 migration 设计决定。

行为覆盖矩阵见 `research/claude-workflow-behavior-coverage.md`。该矩阵是后续 design 的架构压力测试：如果某类核心 workflow 语义只能通过新增平行 runtime 支持，或无法映射到 Lifecycle / Orchestration / typed execution identity / trace surface，就说明目标架构还不妥善。这里的 typed execution identity 至少包括 `AgentRun` 与 `FunctionRun`，后续也可以扩展出本机 bridge / extension action 的 effect invocation 引用。

## 路线收敛

早期路线已经收敛到 `design.md` 与 `implement.md`：

- 第一批：session-scoped API 命名迁移、AgentRun 外露语义收敛、最小 Orchestration domain contract 与 migration。
- 第二批：`WorkflowGraph -> OrchestrationPlanSnapshot` compiler，证明静态 graph 能落入 common runtime IR。
- 第三批：common orchestration runtime 承载静态 graph，生成兼容 projection，并收敛旧过程仓储职责。
- 第四批：动态 script artifact / reusable script definition / script compiler，输出同一 `OrchestrationPlanSnapshot`。
- 后续 UI 与治理：progress tree、pause/resume/stop/restart、token/cost、权限提示、保存为 Project/Shared Library asset。

本文件保留研究结论和路线形成原因；具体合同、阶段边界、风险文件和验证命令以 `design.md` / `implement.md` 为准。

## 需要后续决策的问题

1. 脚本语言选择

   JS/TS 表达力最高，贴近 Claude feature；Rhai 更贴近现有 hook script；自定义 DSL 安全但表达力弱。推荐先设计成受限 JS/TS 运行时，但执行能力必须通过平台原语暴露。

2. 运行时 IR 边界

   需要决定 runtime scripted rule plan 的表达能力：是否支持循环、动态 fan-out、barrier、pipeline、condition、artifact exchange、join policy、schema retry。推荐以现有 `WorkflowGraph` 可表达语义为最低闭包，再逐步加入 dynamic workflow 原语。

3. 资产归属

   一次性脚本应作为 run artifact；可复用脚本应作为 Project asset；个人级脚本是否需要独立用户空间可后置。推荐先做 Project asset + run artifact。

4. 执行节点身份

   脚本 `agent()` 默认应创建或复用 Lifecycle 内的 `AgentRun`，并通过 unified runtime node binding 关联到某个 `OrchestrationInstance`。function / bash / API / extension action 等非 Agent 节点应使用 `FunctionRun` 或更通用的 effect invocation identity，并同样进入 node state、journal、权限与 trace。只有编译后的 rule plan 明确需要 Activity 绑定时才生成 Activity-compatible projection。

5. 权限继承

   脚本 runtime 不应拥有未建模的 raw host access。Agent 工具调用、本机 shell、API request、extension action 都应通过声明式 capability / permission / workspace root / budget surface 执行。是否允许“自动接受编辑”或自动执行 shell 需要产品级明确，推荐先继承并收窄当前 permission/capability，不默认扩大。

6. 持久化粒度

   需要新增或收敛 journal/cache/snapshot，而不是复用 `LifecycleRun.execution_log` 塞大 JSON。`execution_log` 适合摘要事件，journal 适合完整 agent call、result、cache、phase 状态。

7. 仓储收敛策略

   需要逐项判断现有过程仓储哪些是事实源、哪些只是索引、lease 或 projection。推荐以后续设计文档单列 repository convergence matrix，避免引入 dynamic workflow 后再加一套平行仓储。

## 风险

- 如果把动态脚本直接落进 `workflow_graphs`，会污染长期资产，并把一次性任务和可复用业务流程混在一起。
- 如果静态 graph 和动态 script 各自拥有运行时规则，会形成新的双轨 workflow runtime，未来调试、恢复、权限和 UI 都会翻倍复杂。
- 如果只新增仓储而不收敛现有过程状态，会加重当前过程仓储过重的问题。
- 如果跳过 journal/cache，只做“脚本里循环开 agent”，会失去 Dynamic Workflows 最重要的恢复、调试和成本可观测价值。
- 如果脚本拥有未建模的宿主文件系统或 shell 权限，会绕过 AgentDash 已经建立的 capability / permission / workspace root / runtime trace 控制面；但受控本机执行本身应作为 workflow runtime 的一等 effect 能力被支持。
- 如果 UI 仍只展示 Activity graph，用户无法审查脚本生成的动态执行轨迹，也无法定位失败 agent。

## 关键事实来源复核索引

后续继续设计时，优先从这些位置复核当前结论，不要只依赖本文转述。

### 外部 Dynamic Workflows 事实

- 用户本轮贴入的 Claude Code Dynamic Workflows 官方文档转码文本：`research/claude-dynamic-workflows-official-doc-zh-cn.md`。
  - 原始 attachment：`C:\Users\Syan\.codex\attachments\eb234242-cfb0-41b0-a46b-98ed35c00340\pasted-text.txt`。
  - 复核点：`agent()`、`parallel()`、`pipeline()`、`phase()`、`log()`、`workflow()` 原语；运行前审批；保存 workflow；`args`；运行限制；pause/resume；成本。
- 用户本轮贴入的中文调研文章：`research/claude-dynamic-workflows-article-zhihu-simpread.md`。
  - 原始 attachment：`C:\Users\Syan\.codex\attachments\79de185a-0bc7-414b-8d05-87a4e2392039\pasted-text.txt`。
  - 复核点：Claude workflow 与 subagent / skills / agent teams 的差异；中间结果留在脚本变量；Claude 产品边界中的脚本不直接访问文件系统或 shell；并发和 agent 数上限；journal/cache/resume；DAG 与命令式脚本差异。
- 若需要联网确认，优先查官方 Claude Code docs 的 workflows 页面和 agents 页面。

### AgentDash 目标契约

- `.trellis/spec/backend/workflow/architecture.md`
  - 复核点：`WorkflowGraph`、`LifecycleRun`、`WorkflowGraphInstance`、`ActivityAttemptState`、`ActivityExecutionClaim`、`AgentAssignment`、`RuntimeSessionExecutionAnchor` 的目标职责。
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
  - 复核点：Activity runtime identity、graph instance namespace、executor launcher、function executor、artifact contract、template install/update。
- `.trellis/spec/backend/workflow/lifecycle-edge.md`
  - 复核点：edge kind、artifact implies flow、runtime advancement、validation。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`
  - 复核点：前端 definition/run/store 边界，`workflowStore` 与 `lifecycleStore` 的职责分离，`LifecycleRunView` 的 graph instance 结构。

### AgentDash 源码事实

- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/target-model-sketch.md`
  - 复核点：目标模型草案、当前/目标 Mermaid 图、`WorkflowGraphInstance -> OrchestrationInstance` 命名讨论、definition/runtime/projection 仓储边界。
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/claude-workflow-behavior-coverage.md`
  - 复核点：Claude Workflow 行为覆盖矩阵；后续设计必须逐项说明支持方式和架构落点。
- `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/current-code-context.md`
  - 复核点：2026-06-06 重新拉取的 Lifecycle / WorkflowGraph / ProjectAgent / MCP / persistence 当前代码事实地图；后续设计时应优先用它恢复上下文，再回源码复核。
- `crates/agentdash-domain/src/workflow/entity.rs`
  - 复核点：`AgentProcedure`、`WorkflowGraph`、`ActivityExecutionClaim`、`LifecycleRun` 当前 domain shape。
- `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs`
  - 复核点：`WorkflowGraphInstance` 如何承载 `activity_state`，以及 `replace_activity_state` 的写入边界。
- `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs`
  - 复核点：Activity executor、completion policy、iteration policy、join policy、transition condition、artifact binding。
- `crates/agentdash-domain/src/workflow/value_objects/run_state.rs`
  - 复核点：`ActivityLifecycleRunState`、attempt/output/input、`ExecutorRunRef`、claim status。
- `crates/agentdash-spi/src/platform/function_runner.rs`
  - 复核点：workflow function executor 的平台端口，`ApiRequest` / `BashExec` 如何作为非 Agent 执行身份进入 runtime。
- `crates/agentdash-infrastructure/src/function_runner.rs`
  - 复核点：`DefaultFunctionRunner` 对 HTTP request 与 bash exec 的当前实现边界。
- `crates/agentdash-api/src/mount_providers/relay_fs.rs`
  - 复核点：relay-backed shell execution 如何携带 `mount_root_ref`、cwd、timeout 并进入本机后端。
- `crates/agentdash-relay/src/protocol.rs`
  - 复核点：`command.tool.shell_exec` / `response.tool.shell_exec` / `event.tool.shell_output` 的协议事实。
- `packages/extension-sdk/src/index.ts`
  - 复核点：extension `process.exec` / `process.shell` 与 `process.execute` permission 作为本机 Host API 执行面。
- `crates/agentdash-application/src/workflow/engine.rs`
  - 复核点：`initialize`、`apply_event`、`advance_successors`、`create_ready_attempt`、`transition_condition_matches` 当前状态机规则。
- `crates/agentdash-application/src/workflow/scheduler.rs`
  - 复核点：ready attempt claim、executor launch、claim lease/idempotency。
- `crates/agentdash-application/src/workflow/activity_run.rs`
  - 复核点：load definition + run + graph instance + state 后应用 event / launch ready attempts 的当前模式。
- `crates/agentdash-application/src/workflow/dispatch_service.rs`
  - 复核点：`LifecycleDispatchService` 如何创建 run、graph instance、agent、frame、assignment、runtime session anchor；graphless 与 workflow graph 两条路径。
- `crates/agentdash-api/src/routes/workflows.rs`
  - 复核点：definition CRUD、validate、start lifecycle run、human decision API 如何接入 application service。
- `crates/agentdash-mcp/src/servers/workflow.rs`
  - 复核点：Agent 可通过 MCP 创建/更新 AgentProcedure 与 WorkflowGraph；这是“模型写静态定义”的现有入口。
- `crates/agentdash-spi/src/platform/tool_capability.rs`
  - 复核点：workflow_management capability 暴露哪些 workflow MCP 工具。
- `packages/app-web/src/stores/workflowStore.ts`
  - 复核点：前端 `WorkflowGraph` definition draft、validation、save bundle，以及 cycle warning。
- `packages/app-web/src/features/workflow/ui/lifecycle-dag-canvas.tsx`
  - 复核点：ReactFlow graph editor 允许回环、自连和 artifact/flow edge 创建。
- `packages/app-web/src/features/workflow/model/dag-layout.ts`
  - 复核点：`findUnboundedCycles` 客户端 warning 逻辑。

### 持久化事实

- `crates/agentdash-infrastructure/migrations/0001_init.sql`
  - 复核点：`workflow_graphs` 是定义态表；`lifecycle_runs`、`lifecycle_workflow_instances`、`activity_execution_claims`、`agent_assignments`、`runtime_session_execution_anchors` 是过程/运行事实表；`idx_lwi_run_root` 与 `ux_activity_execution_claims_active_attempt` 表达当前运行约束。
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
  - 复核点：workflow graph、lifecycle run、activity claim 仓储当前被放在同一个 repository 文件中。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`
  - 复核点：agent assignment 与 runtime session execution anchor 的 persistence 边界。

### 历史上下文

- `C:\Users\Syan\.codex\memories\MEMORY.md`
  - 复核点：此前关于 runtime control plane、RuntimeSessionExecutionAnchor、空 lifecycle 历史污染、create-plus-first-send 的讨论摘要。

## 结论

AgentDash 应该以 research 目录两份 Claude Code Workflow 资料为核心行为参照，学习其“编排代码化 + 确定性运行时 + journal/cache + 可审进度树”，并用它检验自身架构是否能自然扩展，而不是复制 Claude Code 的所有产品细节。当前 Lifecycle control plane 是优势：Lifecycle、AgentRun、FunctionRun、本机 bridge/effect identity、frame、runtime anchor 能共同承载真实执行身份与 trace 归属。正确的逼近方式是把 `WorkflowGraph` 和动态脚本都编译为统一 runtime scripted rule plan，把执行态放进 `LifecycleRun` 内部的 0..N 个 `OrchestrationInstance`，通过持久化状态交换快照和 journal 执行，让 Lifecycle 负责身份、权限、持久化、观察和审计，同时收敛现有过重的过程仓储。
