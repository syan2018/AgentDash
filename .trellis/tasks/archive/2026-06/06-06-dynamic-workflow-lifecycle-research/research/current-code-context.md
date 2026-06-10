# Lifecycle / WorkflowGraph 当前代码上下文复原

本文记录 2026-06-06 针对 Dynamic Workflow 预研重新拉取的 AgentDash 当前代码事实。评估时需要保持谨慎：当前实现是快速重构后的过渡状态，只能作为“系统现在如何工作”和“迁移需要面对哪些事实”的依据，不能反推出这就是目标架构。

## 评估口径

- 当前代码里的表、仓储和状态容器不默认等同于最终边界。尤其是 `WorkflowGraphInstance.activity_state`、`ActivityExecutionClaim`、`AgentAssignment`、`RuntimeSessionExecutionAnchor`、`LifecycleRun.execution_log` 分散保存过程信息，应先判断事实源、索引、lease、projection 的职责，再决定保留或收敛。
- 项目仍处预研期，后续实现可以选择干净迁移，不需要为了旧 API 或旧数据库字段保留兼容层。
- Dynamic Workflow 的核心不是“Agent 能创建静态 WorkflowGraph”，而是“脚本化编排规则、受控运行时、journal/cache/snapshot 和可审进度树”。现有静态 graph 能提供语义下限，但不应被直接扩展成唯一运行态。

## 本轮追加复核索引

本轮继续讨论前，重新复核了这些上下文：

- Trellis 规划规则：`.agents/skills/trellis-brainstorm/SKILL.md`、`.trellis/workflow.md`。当前任务仍是 planning，本文只服务研究与设计，不进入实现。
- 任务内文档：`prd.md`、`research.md`、`discussion-journal.md`、`target-model-sketch.md`、`research/README.md`。目标模型已修正为 `LifecycleRun = 全部上下文容器`，`LifecycleRun.orchestrations[] = 0..N 个内部 OrchestrationInstance 状态容器`。新增目标字段和新增列不使用 `_json` / `_jsonb` 后缀，JSON 文本只是存储方式。
- 外部资料副本：`research/claude-dynamic-workflows-official-doc-zh-cn.md`、`research/claude-dynamic-workflows-article-zhihu-simpread.md`。复核重点是脚本化编排、`agent()` / `parallel()` / `pipeline()` / `phase()` 原语、运行前审批、保存 workflow、暂停恢复、并发和 agent 总数上限，以及 Claude 产品中“脚本无直接文件系统或 shell 访问”的安全边界。AgentDash 不能简单照搬这条边界，因为当前系统已有受控本机执行面。
- 项目 specs：`.trellis/spec/backend/workflow/architecture.md`、`activity-lifecycle.md`、`lifecycle-edge.md`、`lifecycle-run-link.md`、`.trellis/spec/frontend/workflow-activity-lifecycle.md`、`.trellis/spec/backend/session/architecture.md`、`runtime-execution-state.md`、`.trellis/spec/backend/repository-pattern.md`、`database-guidelines.md`。这些 specs 描述的是当前 graph/activity runtime 与 repository 约束，后续正式设计需要把它们更新到新的 Lifecycle / Orchestration 分层。
- 源码与 migration：`crates/agentdash-domain/src/workflow/*`、`crates/agentdash-application/src/workflow/*`、`crates/agentdash-api/src/routes/lifecycle_agents.rs`、`packages/app-web/src/services/lifecycle.ts`、`crates/agentdash-infrastructure/src/persistence/postgres/*`、`crates/agentdash-infrastructure/migrations/0001_init.sql`。复核结论是当前 `LifecycleAgent` 命名暴露在 domain/application/API/generated TS/frontend，当前 `/lifecycle-agents/by-runtime-session/...` 是 runtime-session 反查 Agent 控制面的命令入口。

## 定义态入口

当前 `WorkflowGraph` 是项目级可复用定义资产，包含 `project_id`、`key`、`source`、`version`、`entry_activity_key`、`activities`、`transitions`，见 `crates/agentdash-domain/src/workflow/entity.rs:72-89`。`WorkflowGraph::new` 会在构造时执行结构校验，见 `crates/agentdash-domain/src/workflow/entity.rs:133-183`。

单个 Activity 的定义闭包比普通 DAG 丰富：`ActivityDefinition` 包含 executor、input/output ports、completion、iteration、join policy，见 `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:6-22`。executor 已覆盖 agent / function / human，agent executor 又区分 create activity agent 与 continue current agent，见同文件 `:24-91`；transition 已覆盖 condition、artifact binding、max traversal，见同文件 `:185-241`。因此 runtime IR 的最低闭包不能只表达节点和边，至少要覆盖 attempt、executor surface、condition、artifact exchange、join/iteration policy。

Agent 现在可以通过 Workflow MCP 管理定义，但这个入口是 Project 资产 CRUD，而不是 per-run 编排脚本入口。`WorkflowMcpServer` 声明为 Project 级 Workflow / Lifecycle 创建编辑工具，见 `crates/agentdash-mcp/src/servers/workflow.rs:1-8`、`:165-174`；`upsert_workflow_tool` 创建或更新 `AgentProcedure`，见同文件 `:531-555`；`upsert_lifecycle_tool` 创建或更新 `WorkflowGraph`，见同文件 `:558-591`。`workflow_management` capability 暴露 list/get/upsert workflow/lifecycle 工具，见 `crates/agentdash-spi/src/platform/tool_capability.rs:500-535`。

前端 workflow 编辑器仍是 definition draft。`workflowStore` 为每个 agent activity 维护一份 `AgentProcedure` draft，并在保存时先保存 procedure，再保存 lifecycle graph；相关 draft 索引、校验与保存入口可从 `packages/app-web/src/stores/workflowStore.ts:187-194`、`:1033-1043`、`:1057-1121` 复核。前端 payload 中仍有 `target_kinds`，见 `packages/app-web/src/services/workflow.ts:431-453`、`:515-576`；后续设计前应顺手复核它与当前 Rust domain 的契约是否仍一致。

## 当前运行态链路

`LifecycleRun` 现在是运行控制 ledger，区分 `graphless` 与 `workflow_graph` topology，保存 `root_graph_id`、`status`、`execution_log` 和活动时间，见 `crates/agentdash-domain/src/workflow/entity.rs:195-268`。`WorkflowGraphInstance` 是 run 内 graph 生效实例，并以内嵌 `activity_state` 保存运行状态 JSON，见 `crates/agentdash-domain/src/workflow/workflow_graph_instance.rs:7-22`；`replace_activity_state` 会检查 graph instance 归属并用 activity state 推导 instance status，见同文件 `:57-71`。

`ActivityLifecycleRunState` 当前是 Activity runtime state 的主要容器，包含 attempts、outputs、inputs，见 `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:69-79`。attempt status、executor run ref 与 claim status 见同文件 `:6-37`、`:92-123`。

`LifecycleDispatchService::start_lifecycle_run` 会解析静态 graph，创建 `LifecycleRun`、root `WorkflowGraphInstance`，调用 `LifecycleEngine::initialize` 初始化 activity state，再同步 run projection，见 `crates/agentdash-application/src/workflow/dispatch_service.rs:318-349`。graph dispatch 路径会继续创建 agent、subject association、runtime session、frame，并两段写入 execution anchor，见同文件 `:351-477`；graphless 路径仍创建 run、agent、frame、runtime session anchor，但没有 graph instance / assignment，见同文件 `:479-551`。

`ActivityLifecycleRunService` 每次推进都会加载 definition + run + graph instance + state，应用 `ActivityEvent`，整体替换 `activity_state`，同步 run projection，并写回 graph instance 与 run，见 `crates/agentdash-application/src/workflow/activity_run.rs:48-67`、`:83-101`、`:104-199`。这说明当前 Activity runtime 接近“事件驱动的状态机”，但事实没有 append-only journal，而是事件应用后重写 snapshot。

`LifecycleEngine` 负责初始化 entry ready / 其他 pending、应用 Scheduler/Executor/Activity/Human 事件，并推进后继，入口可从 `crates/agentdash-application/src/workflow/engine.rs:119-170`、`:214-250`、`:357-453` 复核。`ActivityExecutorScheduler` 扫描 Ready attempt，创建或获取 `ActivityExecutionClaim`，再调用 executor launcher，见 `crates/agentdash-application/src/workflow/scheduler.rs:95-141`、`:143-222`；claim 启动成功或失败再反向写回 state，见同文件 `:224-274`。

`LifecycleOrchestrator` 是 session terminal / `complete_lifecycle_node` 到 ActivityEvent 的桥接，不维护自己的状态，见 `crates/agentdash-application/src/workflow/orchestrator.rs:1-10`。session terminal 回调通过 runtime session 反查 assignment，再 apply event 与 launch successors，见同文件 `:120-186`；Agent 主动推进工具 `complete_lifecycle_node` 通过 orchestrator 校验并推进 lifecycle，见 `crates/agentdash-application/src/workflow/tools/advance_node.rs:21-24`、`:76-139`。

## Agent / Runtime 身份骨架

`RuntimeSessionExecutionAnchor` 是 runtime session 到 run / agent / launch frame / assignment / activity attempt 的 launch evidence，见 `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20-44`。它设计为创建时刻证据，不被后续 frame revision 覆盖，见同文件 `:20-27`；dispatch 先写 frame/agent，再在 assignment 创建后回填 attempt，见 `crates/agentdash-application/src/workflow/dispatch_service.rs:419-456`。

`AgentAssignment` 是 activity attempt 与 lifecycle agent/frame 的桥接，key 包含 `graph_instance_id + activity_key + attempt`，见 `crates/agentdash-domain/src/workflow/agent_assignment.rs:5-22`。这对当前静态 graph attempt 很合适，但动态脚本若引入 phase / agent call / dynamic fan-out，不应直接伪造旧 attempt 结构作为唯一事实源。

Agent activity executor 已经把 Activity 映射到真实 runtime session：`start_spawn_child` 创建 assignment、frame、runtime session 并 launch workflow prompt，见 `crates/agentdash-application/src/workflow/agent_executor.rs:770-835`；`start_continue_root` 复用当前/root runtime session 并禁止并行 running ContinueRoot，见同文件 `:838-908`；function executor 返回即时 completion event，见同文件 `:910-927`。后续动态脚本的 `agent()` 原语应优先复用这条 agent/frame/session/anchor 能力，而不是绕过控制面直接开 agent。

## 本机执行 / System Bridge 事实

当前 workflow 不是纯 AgentRun 副作用模型，已经存在多条受控本机/system bridge 执行面：

- `ActivityExecutorSpec` 包含 `Function(FunctionActivityExecutorSpec)`，而 `FunctionActivityExecutorSpec` 直接覆盖 `ApiRequest` 与 `BashExec`，见 `crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:28`、`:95-109`。因此 common runtime 的 node 不能只表达 `agent_call`。
- `ExecutorRunRef` 已经区分 `RuntimeSession`、`FunctionRun`、`HumanDecision`，见 `crates/agentdash-domain/src/workflow/value_objects/run_state.rs:94-97`。`FunctionRun` 是现有非 Agent 执行身份，应被目标模型保留或泛化，而不是并入 AgentRun。
- `.trellis/spec/backend/workflow/activity-lifecycle.md:17`、`:137` 明确要求 Function executor 即使立即完成，也必须产出 Activity terminal event，并返回 `ExecutorRunRef::FunctionRun`。这说明 function executor 是 Activity runtime contract 的一等 executor。
- `FunctionRunner` SPI 定义在 `crates/agentdash-spi/src/platform/function_runner.rs:36-37`，`DefaultFunctionRunner` 在 `crates/agentdash-infrastructure/src/function_runner.rs:25-26` 实现 HTTP request / bash exec 的平台边界。`agent_executor.rs:931-989` 将 function executor outcome 映射回 workflow terminal event。
- capability 层已有 `shell_execute`，并映射到 Execute cluster 的 `shell_exec` 工具，见 `crates/agentdash-spi/src/platform/tool_capability.rs:79-117`、`:297-302`、`:849-872`。workflow/tool 可见性还要经过 `auto_granted` / `agent_can_grant` / `workflow_can_grant` 规则，见同文件 `:688-812`。
- relay FS mount provider 的 shell 执行会发送 `RelayMessage::CommandToolShellExec`，payload 携带 `call_id`、`command`、`mount_root_ref`、`cwd`、`timeout_ms`，见 `crates/agentdash-api/src/mount_providers/relay_fs.rs:594-599`；relay protocol 也有 `CommandToolShellExec`，见 `crates/agentdash-relay/src/protocol.rs:164-166`。
- extension SDK 的 Host API 暴露 `process.exec` / `process.shell`，见 `packages/extension-sdk/src/index.ts:231-233`；权限模型包含 `{ kind: "process"; access: ... }`，见同文件 `:52`。共享库契约也把运行时 Host API permission 写成 `process.execute`，见 `.trellis/spec/cross-layer/shared-library-contract.md:202`。

直接含义是：目标脚本 runtime 的安全边界应写成“脚本不能拥有未建模 raw host access；Agent、function、本机 shell、API request、extension action 都作为 typed runtime node / effect invocation，经 capability、permission、workspace root、audit、trace、journal 执行”。这能更自然支持后续多个 `OrchestrationInstance` 并发时的 effect 归属与恢复。

ProjectAgent draft 首条消息链路已经体现了一个重要经验：不要先创建空 runtime/lifecycle 历史再等待输入。`ProjectAgentSessionStartService::start_session` 在同一个后端动作中创建 linked run/runtime session，然后立刻 dispatch 用户首条消息，见 `crates/agentdash-application/src/workflow/project_agent_session_start.rs:131-268`；失败且 session 仍无 events 时会删除 anchor、session 和 run，见同文件 `:293-312`。前端 draft action 调用 `/projects/{id}/agents/{agent}/sessions` 后再跳转正式 session，见 `packages/app-web/src/pages/SessionPage.tsx:469-490`，API 路由见 `crates/agentdash-api/src/routes/project_agents.rs:243-310`。

## 观察与投影

`LifecycleRunView` 当前以 Activity attempt 为中心。builder 读取 graph instances、agents、assignments、runtime anchors，并把 `activity_state.attempts` 映射成 `WorkflowGraphInstanceView.activities`，入口与关键函数见 `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:35-82`、`:342-363`。这能展示静态 Activity 进度，但不足以表达脚本变量、phase tree、agent call cache、动态 fan-out 结果。

runtime session 到 active workflow projection 的解析路径是 `RuntimeSession -> RuntimeSessionExecutionAnchor -> AgentAssignment -> ActivityAttemptState -> WorkflowGraph/AgentProcedure`，见 `crates/agentdash-application/src/workflow/projection.rs:72-131`。hook target 解析也依赖 assignment 与 `activity_state`，见同文件 `:133-212`。这说明现有 hooks / tool surface 已绑定 Activity attempt 语义，动态运行时若要共享能力投影，需要提供等价但更通用的 runtime node binding。

session bootstrap 的 workflow context 只解析 ProjectAgent/Story 绑定的 lifecycle entry activity，再读取对应 `AgentProcedure.contract.capability_config.tool_directives`，见 `crates/agentdash-application/src/capability/session_workflow_context.rs:53-74`、`:139-247`。这适合作为启动时能力注入，但不是动态脚本运行时的权限模型。

取消控制现在按 subject 找 active agent/assignment，apply `ActivityCancelled`、abandon claim、release assignment，再选择 runtime session 投递 cancel，见 `crates/agentdash-application/src/workflow/subject_execution_control.rs:83-116`、`:249-359`。未来 pause/resume/restart single agent 应考虑是否沉到统一 runtime journal/control command，而不是只面向 Activity assignment。

## 持久化事实

当前定义态与运行态表分散在多处：

- `activity_execution_claims`：`crates/agentdash-infrastructure/migrations/0001_init.sql:1`
- `agent_assignments`：同文件 `:15`
- `lifecycle_runs`，含 `execution_log`：同文件 `:282-288`
- `lifecycle_workflow_instances`，含 `activity_state_json`：同文件 `:305-311`
- `runtime_session_execution_anchors`：同文件 `:533`
- session event/projection/outbox 系列表：同文件 `:575-668`
- `workflow_graphs`：同文件 `:764`
- root graph instance unique index：同文件 `:1098`
- active attempt claim unique index：同文件 `:1198`

session 侧已经有更清晰的事实日志 / projection 模式：`PostgresSessionRepository::append_event` 先增加 `sessions.last_event_seq`，再插入 `session_events`，见 `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:319-408`；`SessionEventingService::persist_notification` 先提交可能的 compaction projection，再 append event、推进 projection head、广播，见 `crates/agentdash-application/src/session/eventing.rs:102-150`。这可以作为 workflow runtime journal/snapshot 的参考，但不应把 workflow 编排事实直接塞进 `session_events`，因为 session event 的坐标是 runtime conversation，不是 workflow run/node。

## 对 Dynamic Workflow 实现的直接含义

1. 不应把一次性动态脚本直接 upsert 到 `workflow_graphs`。那会污染 Project 长期资产，并混淆“可复用定义”和“本次 run artifact”。
2. 不应继续围绕 `WorkflowGraphInstance.activity_state` 叠加脚本变量、phase、cache、journal。它当前是静态 Activity snapshot，适合作为迁移来源或兼容 projection 候选，不适合作为动态编排事实源。
3. 应优先设计 common runtime rule plan + runtime snapshot / journal：静态 `WorkflowGraph` 编译进去，动态 script 也编译进去，二者共享调度、权限、resume、cache、观察和失败恢复规则。
4. 保留并复用 `LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`RuntimeSessionExecutionAnchor` 这类身份/归属/trace 反查骨架的价值较高；`ActivityExecutionClaim`、`AgentAssignment`、`WorkflowGraphInstance.activity_state` 更需要进入 repository convergence matrix，逐项判断它们是事实源、lease、索引还是 projection。
5. 第一阶段最好先证明“静态 graph -> common runtime rule plan -> common runtime snapshot/journal”能覆盖现有语义，再接动态脚本。这样能避免为 dynamic workflow 新增一套平行 scheduler。

## 后续设计前的复核清单

- `WorkflowGraph` 当前语义闭包：`crates/agentdash-domain/src/workflow/entity.rs:72-183`、`crates/agentdash-domain/src/workflow/value_objects/activity_def.rs:6-241`。
- 当前 state rewrite 模式：`crates/agentdash-application/src/workflow/activity_run.rs:48-101`、`crates/agentdash-application/src/workflow/engine.rs:119-250`。
- 当前 claim / assignment / anchor 关系：`crates/agentdash-application/src/workflow/scheduler.rs:95-274`、`crates/agentdash-domain/src/workflow/agent_assignment.rs:5-22`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20-44`。
- 当前 ProjectAgent create-plus-first-send 经验：`crates/agentdash-application/src/workflow/project_agent_session_start.rs:131-312`、`packages/app-web/src/pages/SessionPage.tsx:469-490`。
- 当前 MCP 定义管理入口：`crates/agentdash-mcp/src/servers/workflow.rs:531-591`、`crates/agentdash-spi/src/platform/tool_capability.rs:500-535`。
- session event/projection 可借鉴模式：`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:319-408`、`crates/agentdash-application/src/session/eventing.rs:102-150`。
