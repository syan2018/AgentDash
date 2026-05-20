# Lifecycle 系统 Activity/Executor 重新设计

## Goal

完整重新设计 AgentDash 的 Workflow Lifecycle 系统抽象，使其从“围绕主 session 与 node type 的 DAG 编排”演进为“目标导向的工作过程实例”。

新的设计需要明确回答：

- Lifecycle 究竟是什么：不是 session 生命周期，而是一次目标导向的工作过程实例。
- 主 session 与 LifecycleRun 的关系是什么：主 session 是宿主、交互入口、上下文锚点和可能的执行载体之一。
- 图中的节点是什么：节点应统一表示 Activity，而不是把 AgentNode / PhaseNode / FunctionNode 等实现机制混作 node type。
- 不同执行方式如何表达：Activity 通过 ExecutorSpec 选择 Agent / Function / Human 等执行器。
- 审批退回、条件分支、循环修订如何表达：通过 Transition / ActivityAttempt / structured decision artifact 表达，而不是把 Completed 节点重置回 Ready。

本任务先产出完整系统设计，不直接实现。设计完成后再拆分具体实现任务。

## Intent Alignment

这次重构的目的不是为了换一组命名，也不是为了提前追求更复杂的 workflow engine。真正要解决的是：当前 Lifecycle 已经开始承载多种执行主体，但系统本体仍然围绕 `node_type + session terminal` 心智组织，导致后续每新增一种能力都会扩大语义歧义。

需要重构的核心原因：

- **节点语义不统一**：AgentNode 是工作单元，PhaseNode 是 session 运行态切换，FunctionNode 是平台动作；三者并列会让 DAG 中的“节点”到底是什么变得含混。
- **主 session 角色不清**：它既像 lifecycle 的宿主，又像执行者，又像推进控制器；如果不拆清楚，ContinueRoot、审批、平台动作都会被迫伪装成 session 行为。
- **推进入口分散**：session terminal、agent tool、hook、function result、human decision 都可能推进流程，但现在缺少统一的 ActivityEvent / LifecycleEngine 边界。
- **退回与修订缺少正确表达**：审批不通过后重新规划不是把 Completed 节点重置，而应保留历史 attempt 并创建下一次 attempt。
- **UI 心智会持续变复杂**：如果继续用 node type 表达执行机制，编辑器和运行视图会不断增加例外分支，而不是呈现统一的“Activity + Executor + Attempt”模型。

这次重构希望达成的对齐是：

> Lifecycle 是工作过程实例；Activity 是过程里的工作单元；Executor 是完成 Activity 的方式；Session 是 Executor 可使用的执行载体之一。

## Expected Outcomes

重构完成后，系统应收获以下结果：

- **概念稳定**：后续新增 API、Bash、Human Approval、JSON Transform、Agent ContinueRoot 等能力时，都作为 executor 扩展，而不是继续增加顶层 node type。
- **状态可解释**：每个 Activity 有 attempt 历史，审批退回、自动重试、人工修订都能被审计，而不是修改或覆盖旧状态。
- **推进统一**：所有执行结果都转成 ActivityEvent，由 LifecycleEngine 统一写 artifact、评估 completion、计算 transition、调度后继。
- **主 session 边界清晰**：root session 是宿主和交互入口，也可以在 ContinueRoot policy 下参与执行，但不再等同于 lifecycle 本身。
- **UI 更好理解**：用户看到的是 Activity 图、executor 配置和 attempt timeline，而不是混杂的 Agent/Phase/Function 节点规则。
- **MVP 能证明价值**：Plan -> Approval -> Replan -> Implement 作为首个闭环 case，直接验证 Human Approval、Conditional Transition、ActivityAttempt、artifact latest/history 的组合价值。

## Background

现有系统已经具备：

- `WorkflowDefinition`：描述单 session 的 workflow contract。
- `LifecycleDefinition`：描述多个 step 构成的 DAG。
- `LifecycleEdge`：区分 `flow` 与 `artifact`，artifact edge 隐含 node 级依赖。
- `LifecycleStepDefinition.node_type`：当前为 `agent_node` / `phase_node`，计划中的 Function Node 会新增平台执行动作。
- `LifecycleRun`：维护 step states、active node keys、execution log。
- Orchestrator：当前主要由 session terminal / advance tool 等事件触发，扫描 Ready node 并启动 AgentNode 或激活 PhaseNode。

最新 Function Node 规划暴露出更基础的问题：三类 node 不在同一抽象层。

- `agent_node` 表达一个由 child session 执行的工作单元。
- `phase_node` 表达 root session 内的 contract / capability transition。
- `function_node` 表达平台直接执行的确定性动作。

它们混在 Lifecycle DAG 内，会让节点语义、完成事件、运行日志、权限边界和 UI 心智变得不清晰。

## Requirements

### Functional Requirements

- 重新定义 Lifecycle：LifecycleDefinition 表达可复用过程定义，LifecycleRun 表达一次过程实例。
- 重新定义节点：图中的节点统一为 Activity，Activity 是可调度、可观测、可完成、可失败的工作单元。
- 引入 `ActivityExecutorSpec`：
  - `Agent`：由 Agent workflow contract 执行。
  - `Function`：由平台执行 API request、Bash exec 等确定性动作。
  - `Human`：由用户审批、决策、输入等人工交互完成。
- 将现有 `agent_node` 映射为 `AgentExecutor + SpawnChild`。
- 将现有 `phase_node` 重新定义为 `AgentExecutor + ContinueRoot`，而不是独立 node type。
- 将 planned `function_node` 重新定义为 `FunctionExecutor` 的 kind，而不是顶层 node type。
- 明确 root session 与 LifecycleRun 的关系：`LifecycleRun.session_id` 是 root session / host session。
- 引入 Activity execution instance / attempt 概念，支持同一个 ActivityDefinition 在同一个 run 内多次尝试。
- 支持审批退回 case：
  - Plan activity 产出固定格式 proposal artifact。
  - Human approval activity 消费 proposal 并产出 structured decision artifact。
  - approved 时进入后续 activity。
  - rejected 时带反馈创建 plan activity 的下一次 attempt。
- 引入 Transition 模型，支持：
  - unconditional transition。
  - artifact / decision 驱动的 conditional transition。
  - 默认 all-complete join。
  - 显式 join policy 的设计预留。
- 明确 ActivityEvent 模型，统一 session terminal、function result、human decision、agent tool advance 等推进来源。
- 重新设计 Orchestrator / LifecycleEngine 边界：
  - LifecycleEngine 负责状态转移、attempt、artifact、execution log、successor ready 计算。
  - Executor scheduler 只负责启动或恢复具体执行器。
- MVP 纳入最小 Human Approval executor，用于支撑审批通过 / 退回重规划的闭环验证。
- 明确运行视图与编辑器心智：
  - DAG 展示 Activity。
  - Inspector 选择 executor，而不是 node type。
  - 运行视图按 Activity 展示 attempts 和执行实例。

### Non-Functional Requirements

- 使用强类型 schema 表达 Activity / Executor / Transition / Attempt，减少 optional 字段组合出的非法状态。
- 项目处于预研期，不要求兼容旧 schema；可以通过迁移直接演进到更正确模型。
- 设计必须考虑云端 / 本机双后端边界：云端不直接访问本机文件系统，本机不直接读写业务数据库。
- 设计必须保留可观测性：每次 attempt、decision、artifact、execution event 都可追踪。
- 设计必须支持后续拆分实现，避免一次性大爆炸式落地。
- 设计应明确哪些是 MVP，哪些只是模型预留。

## Key Scenario

核心 case：

```text
PlanDraft
  -> ApprovalGate
      approved  -> Implement
      rejected  -> PlanDraft(next attempt, with feedback)
```

期望运行态不是把 `PlanDraft` 从 Completed 重置为 Ready，而是：

```text
plan_draft#1 completed -> proposal_v1
approval_gate#1 rejected -> feedback_v1
plan_draft#2 completed -> proposal_v2, consumes feedback_v1
approval_gate#2 approved -> approval_v2
implement#1 ready
```

这要求新模型显式支持 ActivityAttempt、conditional transition 和 structured human decision artifact。

## Acceptance Criteria

- [ ] `prd.md` 明确 Lifecycle / root session / Activity / Executor / Attempt / Transition 的产品语义。
- [ ] `design.md` 给出新的领域模型草案，包括 Rust / JSON schema 示例。
- [ ] `design.md` 明确现有 AgentNode / PhaseNode / FunctionNode 到新模型的迁移映射。
- [ ] `design.md` 完整描述审批退回 case 在新模型下的定义态和运行态。
- [ ] `design.md` 明确 LifecycleEngine 与 ExecutorScheduler 的职责边界。
- [ ] `design.md` 明确 project scope、权限 gate、scheduler claim、attempt 状态机、run 派生状态等实现不变量。
- [ ] `design.md` 明确 conditional transition、loop/attempt、join policy 的 MVP 与预留范围。
- [ ] `design.md` 将最小 Human Approval executor 纳入 MVP，并说明审批退回闭环。
- [ ] `design.md` 说明主 session、child session、function run、human decision 与 LifecycleRun 的关系。
- [ ] `design.md` 说明前端编辑器与运行视图的目标心智。
- [ ] `implement.md` 给出分阶段落地计划，先 schema / domain，再 engine，再 executor，再 UI，再迁移。
- [ ] `implement.md` 将重构拆成可独立 PR 的阶段，并为每阶段给出范围、验证命令和出口标准。
- [ ] `implement.md` 明确首批实际任务，避免直接进入大爆炸式重构。
- [ ] `implement.md` 列出关键验证点和建议拆分的后续实现任务。

## Out of Scope

- 本任务不直接修改代码。
- 本任务不实现 FunctionNode / HumanApproval / ConditionalTransition；但设计范围必须将 Human Approval 纳入 MVP。
- 本任务不设计完整低代码表达式引擎。
- 本任务不实现 cyclic graph runtime；循环修订通过 ActivityAttempt + guarded transition 表达。
- 本任务不设计完整 secret 管理产品。
- 本任务不处理已上线资产兼容；项目仍处于预研期，可直接演进 schema。

## Open Questions

- 首版是否允许多个 `ContinueRoot` Activity 并行 Ready？推荐不允许，同一 root session 同时只能执行一个 ContinueRoot Activity。
- Transition condition 首版使用结构化字段匹配，还是引入表达式语言？推荐先使用 typed condition，不引入通用 DSL。
- Attempt 的 artifact alias 默认策略是什么？推荐 `latest_and_history`：后继默认消费 latest，审计保留全部历史。
