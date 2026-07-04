# AgentRun 生命周期仓储删除驱动收敛规划

## Goal

以删除旧概念和错误组合方式为起点，重构 `Lifecycle`、`AgentRun`、`RuntimeSession`、`AgentFrame`、command/mailbox/delivery、fork/lineage、projection/permission/API/frontend 之间的事实源和仓储边界。

本任务当前已完成规划评估，处于可启动实现前的待确认状态。实现前不再有阻塞性架构开放问题；仍需用户明确批准进入实现阶段。

## Background

项目处于预研期，可以直接切换到正确结构，不需要保留旧 API 或旧 schema 兼容。数据库表、字段和 migration 可以破坏式调整。

当前评估确认的问题不是单纯“概念太多”，而是不同性质的事实被做成了相似的 repository / table / service dependency 形状：

- `RuntimeSession` 同时承担 internal trace 和 raw product write surface。
- `AgentRun` 是用户认知入口，但 API、contracts、frontend 仍有 runtime session product identity 残留。
- `Mailbox` 领域上依附 AgentRun，但当前表和 repository 仍混入 runtime session ownership。
- accepted turn、frame commit、mailbox accepted refs、command receipt outcome、Lifecycle `NodeStarted` 不是同一提交边界。
- `AgentFrame` 是能力与认知事实源，但存在多列 surface、runtime append visible refs 和 highest revision current truth 旁路。
- fork baseline 同时来自 RuntimeSession projection、session lineage、AgentRun lineage、receipt result cache。
- `Projection`、state、binding 命名混用，不可丢失 current pointer 被当作 projection 处理。
- 大 `RepositorySet` 泄漏到 application service，隐藏 use case 的真实依赖。
- 部分物理表来自历史拆分，必须按事实所有权、查询入口、锁/扫描/恢复需求重新判定。

## Target Model

核心目标是减少事实源数量，而不是机械减少表数量：

```text
UI / API
  -> AgentRun control surface
    -> CommandReceipt
      -> AgentRun Queue / Mailbox
        -> RuntimeDeliveryOperation
          -> RuntimeSession internal trace
    -> AgentFrameRevision / ContextDeliveryRecord
    -> AgentRunTurnAccepted + FrameCommit
      -> LifecycleRun control-plane advancement
```

目标归属：

- `AgentRun`：产品一等聚合、用户工作区、单 Agent 会话身份、用户命令、fork、cancel、delete、workspace read model 主入口。
- `LifecycleRun`：多 AgentRun control-plane ledger / orchestrator，负责编排、节点进度、gates、subjects、治理语义。
- `RuntimeSession`：internal delivery stream、event log、connector turn、trace/debug substrate，不作为产品 URL、权限入口或用户写控制面。
- `AgentFrame`：Agent 能力与认知状态的 append-only canonical surface revision。
- `ContextDeliveryRecord`：ContextFrame emission 与 connector input 的唯一 accepted input fact。
- `Projection / Binding`：可重建 read model 或不可丢失 state/binding，命名和重建策略必须明确。

## Requirements

### R1 删除驱动

每个新增或保留边界必须说明它替换了哪个旧概念、旧组合方式或旧调用路径。不得通过叠加新抽象掩盖旧复杂度。

### R2 仓储资格分类

所有相关仓储归入五类之一：

- independent fact source。
- parent-owned child fact。
- parent-owned child table。
- application command/query port。
- runtime trace store。

`inventory.md` 是本轮分类基准。

### R3 RuntimeSession 内部化

`RuntimeSession` 不作为产品 URL identity、前端主状态 key、产品权限入口或用户可见写控制面。raw Session write route 删除或 diagnostic/internal 化。产品写操作收束到 AgentRun scoped API。

### R4 AgentRunAdmission 原子边界

ProjectAgent start / AgentRun start / fork materialization 必须通过 `AgentRunAdmission` 或等价 use case 原子创建 run、agent、initial frame、immutable anchor、initial mailbox envelope、outer command receipt accepted refs。

API 层不调度首条消息，不补偿半成品。

### R5 Accepted Turn + FrameCommit 同边界

Runtime connector accepted 不能先于 AgentRun 产品事实提交成功而对外成立。accepted boundary 必须同步完成 accepted turn、frame commit/applied binding、mailbox accepted refs、command receipt outcome、delivery attempt terminal state、Lifecycle node advancement。

### R6 Lifecycle 状态推进

Lifecycle materialization 只表达 prepared / allocated。`NodeStarted` 只由真实 `AgentRunTurnAccepted` 推进，terminal state 只由真实 terminal fact 推进。

### R7 Command / Mailbox / Delivery 三层事实

命令生命周期固定为：

```text
User instruction / CommandReceipt
  -> AgentRun queue item / Mailbox
    -> RuntimeDeliveryOperation / DeliveryAttempt
```

receipt 不承担 queue state，mailbox 不承担外部命令幂等，runtime delivery operation 不表达用户命令事实。

### R8 Mailbox AgentRun ownership

Mailbox 是 AgentRun-owned durable queue。物理表可以保留，因为 claim、recover、ordering、payload cleanup、pause/resume 需要独立索引和锁；但 owner 必须是 `run_id + agent_id`，runtime session 只能是 nullable delivery/correlation ref。

### R9 AgentFrame canonical surface

AgentFrame 保持 append-only/versioned capability+cognition surface。采用 canonical typed surface document；generated/read-only projection columns 可选但不能作为写源。历史 revision 不被 runtime path 原地 mutate。

### R10 ContextDelivery 唯一输入事实

ContextFrame emission 从 `ContextDeliveryRecord` 或等价 accepted input fact 派生，不能由 launch、commit、transition、compaction 多处各自构造。

### R11 Fork / Lineage 收束

Fork 是 AgentRun 产品操作。`AgentRunForkRecord` 或等价 record 是唯一 product fork fact，包含 parent AgentRun、fixed turn/message boundary、child AgentRun、child baseline、fork owner。RuntimeSession lineage 只保留 internal trace provenance。

### R12 Projection 可重建性

所有 projection/read model 必须声明是否可重建、由哪些 facts 重建、是否参与业务决策。不可重建且参与决策的 projection 必须升格为 state/binding。

### R13 权限边界

产品权限属于 AgentRun/Lifecycle control plane。RuntimeSession trace 访问只能由 control-plane 权限派生，不能反向成为产品授权入口。

### R14 RepositorySet 收敛

大 `RepositorySet` 只属于 composition root。application service 只接收当前 use case 所需的窄 deps；跨聚合写入通过显式 command port / unit of work。

### R15 冗余物理表清理

物理表按 D-016 / D-017 / D-019 审查。没有独立事实源资格、没有 child table 所需锁/claim/扫描/分页/恢复需求、也不是 runtime trace store 或不可丢失 binding 的表，必须删除、合并或降级。

### R16 数据库 migration

所有表级清理、重命名、字段删除、FK/cascade、索引、backfill 通过 migration 完成。本项目不保留旧 schema 兼容。

### R17 执行拆分

实现按 `work-items/` 拆分推进，依赖关系以 `work-items/README.md` 为准。每个工作项必须能独立说明删除目标、决策依据、验收和验证方式。

## Resolved Planning Decisions

执行前开放问题已回填到 `decisions.md` 和 `inventory.md`：

- 删除 `LifecycleRun.context` / `lifecycle_runs.context`。
- `LifecycleGate` 保留为 Lifecycle-owned child table。
- `LifecycleSubjectAssociation` 保留为 indexed relationship table。
- current delivery 迁出 `LifecycleAgent.current_delivery_*`，落为 AgentRun-owned delivery binding/state。
- RuntimeSession trace 表破坏式重命名为 `runtime_session_*`。
- AgentFrame surface 收束为 canonical typed document。
- tool approval 继续是 runtime connector approval，但产品路径只走 AgentRun scoped endpoint。
- 冗余物理表进入 WI-12 redundant table ledger。

## Deliverables

- `prd.md`：最终规划入口。
- `design.md`：目标架构、删除矩阵、仓储分类、数据流、数据库策略。
- `decisions.md`：D-001 到 D-019 的正式决策。
- `inventory.md`：WI-00 清点结果、执行结论、仓储/表/API/frontend 分类。
- `target-state.md`：重构前后状态图和 C-001 到 C-013/C-012B 收口检查。
- `implement.md`：全局阶段顺序和 work item mapping。
- `work-items/`：WI-00 到 WI-12 的可分发执行项。
- `implement.jsonl` / `check.jsonl`：sub-agent dispatch 所需真实 spec/research manifest。

## Implementation Acceptance

- 每个保留 application port 都能映射到被删除的旧组合方式。
- 每个候选仓储都有分类结论，且结论基于生命周期、查询模式、并发需求和父聚合归属。
- 每个保留物理表都有 D-016 / D-017 / D-019 正向资格。
- `RuntimeSession` 的产品写入口已删除或 internal diagnostic 化。
- ProjectAgent/AgentRun start 形成原子 `AgentRunAdmission`。
- RuntimeSession accepted 与 AgentRun frame commit 不再是 best-effort 副作用关系。
- Lifecycle `NodeStarted` 只由真实 accepted turn 推进。
- AgentFrame revision append-only，历史 revision 不再被 runtime path 原地 append/mutate。
- command lifecycle 被拆解为 instruction、queue item、delivery attempt。
- mailbox owner 是 AgentRun，runtime session 不再 cascade 删除 mailbox durable intent。
- product fork replay 读取 canonical AgentRun fork record。
- 每个 projection/read model 标注 rebuildability；不可重建决策状态升格为 state/binding。
- 权限入口收束到 AgentRun/Lifecycle control plane。
- raw `runtime_session_id` 不再是前端 product workspace 主状态 key。
- RepositorySet 不再作为 application service locator。
- migration 覆盖表重命名、字段删除、FK/cascade、索引和数据迁移。
- `target-state.md` 的 C-001 到 C-013/C-012B 可逐项通过。

## Out Of Scope

- 不做兼容旧 API / 旧 schema 的回退路径。
- 不把 runtime trace/debug 能力从系统中删除；只删除其产品控制面身份。
- 不为了减少表数量而合并 event log、frame revision、command receipt、fork record、runtime trace 等不可约事实。
- 不在用户批准前启动实现阶段。

## Execution Gate

规划现在满足启动条件：

- `prd.md` / `design.md` / `implement.md` 已完成。
- `decisions.md` 无阻塞 Open/Conditional 架构问题。
- `inventory.md` 完成 WI-00 清点和执行结论。
- `implement.jsonl` / `check.jsonl` 已替换 seed，包含真实 spec/research manifest。
- 工作项已拆分到 `work-items/` 并写明依赖。

下一步只有一个门槛：用户明确批准后，才能运行 Trellis start 并进入实现。
