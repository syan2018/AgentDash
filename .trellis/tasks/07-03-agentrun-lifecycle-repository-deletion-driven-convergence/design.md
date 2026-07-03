# 删除驱动架构设计

## Problem Statement

当前架构的主要问题不是领域名词数量本身，而是事实源、子事实、应用动作、read projection、runtime bridge 被放进相似的 repository/set 形状里。结果是服务依赖大集合，子事实被误读为同级聚合，RuntimeSession 和 AgentRun/Lifecycle 控制面互相穿透。

重构应先删除错误组合方式，再抽出能替代旧职责的新边界。

## Formal Decision Index

正式决策入口是 `decisions.md`。`research/` 下的对抗式审查文档作为证据库保留，但实现阶段不直接从 research 自由选择结论；所有结论必须先进入 `decisions.md` 的 Accepted / Conditional / Open 状态，再由 `work-items/` 中的具体执行项承接。

这能避免大型重构在执行阶段退回“谁记得哪份研究怎么说”的状态。每个工作项都必须绑定 decision IDs，并在偏离决策时先更新决策索引。

整体重构前后状态和最终收口检查目标见 `target-state.md`。设计评审时先用 `target-state.md` 判断目标图是否仍清晰，再用 `decisions.md` 和 `work-items/` 追踪具体执行。

WI-00 的执行前事实清点见 `inventory.md`。仓储、表、route、DTO、frontend state 的分类和 Q-001 到 Q-008 的执行结论以 `inventory.md` 为准。

## Prior Task Reassessment

参考 PRD `references/agentrun-runtime-session-repository-convergence.prd.md` 的核心视角是“事实源、投影和重复关系收口”。它的价值在于标出了以下不变量：

- `RuntimeSession` 是 runtime trace substrate，不是产品交互目标。
- `BackboneEnvelope` 是 runtime event fact，`session_events` 是 durable ordering/event log。
- `sessions.last_*`、`lifecycle_agents.current_delivery_*` 是 shell/read model 或 current pointer，不能反向成为 terminal truth。
- `RuntimeSessionExecutionAnchor` 是 runtime trace 到控制面的 durable evidence 索引。
- raw `/sessions/*` API 应定位为 trace/diagnostic，产品交互走 AgentRun scoped API。

旧任务的不足是，它倾向于把现有表和仓储都画成稳定节点，再讨论这些节点之间的重复是否合理。这个视角适合作为边界审计，但不适合作为本轮目标架构，因为它没有追问：

- 某个仓储是否真的具备独立事实源资格。
- 某个 child fact 是否只是父聚合的内部状态。
- 某个独立表是否只是父聚合 child table 的实现细节。
- 某个 application service 是否因为大 `RepositorySet` 而自行拼接跨聚合动作。

因此本任务对旧任务结论的处理方式是：

| 旧任务视角 | 本任务处理 |
| --- | --- |
| 事实源与投影必须区分 | 继承，作为删除时不能破坏的不变量 |
| current delivery 与 anchor 职责不同 | 继承，但写入与选择应收束到 `AgentRunDeliveryBindingPort` |
| mailbox 与 runtime accepted event 是双层事实 | 继承语义差异，但 mailbox 降级为 AgentRun child fact，不默认作为顶级仓储 |
| session lineage 与 agent run lineage 双层保留 | 保留为待评估结论；若 product lineage 没有独立查询需求，应降级或合回父聚合 |
| raw Session API 限定诊断用途 | 继承，并在 API/前端 cleanup 阶段删除产品级误用 |
| repository map 展示现有拓扑 | 仅作为现状输入，不作为目标拓扑 |
| 合理投影列表 | 逐项重新判定是独立仓储、父 child table、父 JSONB 子状态还是 application read model |

## Adversarial Review Corrections

`references/adversarial-first-principles-review.md` 把本任务目标从“删除错误组合方式”进一步修正为“重建事实源边界”。以下结论覆盖后续设计优先级：

| 优先级 | 当前错误状态 | 目标修正 |
| --- | --- | --- |
| P0 | RuntimeSession 暴露 fork/rollback/delete/tool approval/title patch 等产品写控制面 | raw Session 产品写入口删除或内部化，所有用户写命令进入 AgentRun |
| P0 | Mailbox message/state 被 `runtime_session_id NOT NULL` 拥有并随 session cascade | Mailbox owner 改为 `run_id + agent_id`；runtime session 只作为 delivery attempt/ref |
| P0 | RuntimeSession launch accepted 后 AgentRun frame/current commit 是 best-effort | `AgentRunTurnAccepted + FrameCommit` 作为同一 accepted boundary |
| P0 | ProjectAgent start 多步写入，没有原子 admission fact | `AgentRunAdmission` 原子产出 run/agent/frame/anchor/initial mailbox/receipt |
| P0 | Product fork 先创建 RuntimeSession fork，再后补 AgentRun | `AgentRunForkRecord` 成为唯一 product fork fact，RuntimeSession 只附属 trace |
| P1 | Lifecycle materialization 直接推进 `NodeStarted` | materialization 只产生 prepared/allocated，accepted turn 才推进 started |
| P1 | AgentFrame revision 可原地追加 runtime visibility，内部 capability/VFS/MCP 有覆盖式双源 | AgentFrame 改为 append-only typed surface revision，visibility 变更必须有明确 source |
| P1 | ContextFrame emission 多处构造，没有实际 model input 的唯一事实 | `ContextDeliveryRecord` 或等价 accepted input fact 作为唯一来源 |
| P1 | `LifecycleAgent.current_delivery_*` 是从 anchor/session 派生的第二事实源 | 删除身份聚合上的 persisted current delivery，另设 attachment/projection |
| P1 | anchor 名为 launch evidence 但 upsert 可改写坐标 | anchor insert-once/idempotent create，current selection 单独建模 |

## Target Ownership

```text
AgentRun
  owns: user-facing fact source, single-agent conversation aggregate, product workspace,
        command surface, command queue, current delivery choice, product lineage slice

LifecycleRun
  owns: control-plane aggregate over one or more AgentRuns, orchestration progress,
        subject ownership, gates, run-level coordination

RuntimeSession
  owns: internal stream state for an AgentRun delivery, execution status, event stream,
        context projection, connector turn lifecycle
```

`AgentRun` 是用户认知上的一等聚合。`LifecycleRun` 可以包含多个 `AgentRun` 并编排它们，二者允许存在控制面双向引用，但用户可见入口固定落在入口 `AgentRun`。

`AgentFrame` 是 Agent 实际能力和认知状态的基准事实源。ContextFrame emission 必须由 `AgentFrame` 的有效状态变化驱动。Frame 内部结构可以整理，但能力判断事实不能散落到 `RuntimeSession`、projection 或 transient cache。

`RuntimeSessionExecutionAnchor` 是 internal stream 到控制面的反向索引。它可以保持独立，因为查询入口天然来自 `runtime_session_id`，不是从父聚合顺序读取。

## Target Control Flow

```text
API / UI
  -> AgentRunAggregate
    -> AgentRunAdmission
    -> AgentRunCommand / CommandReceipt
    -> AgentRunCommandQueue
    -> DeliveryAttempt
    -> AgentFrameRevision / ContextDeliveryRecord
    -> RuntimeSession internal trace
    -> AgentRunTurnAccepted / AgentRunTurnTerminal
```

这条链路的目标是让每一步只表达一种能力：

- `AgentRunAdmission` 负责 start/fork admission 的原子边界。
- `AgentRunCommand / CommandReceipt` 负责用户命令幂等、stale guard 和 accepted refs。
- `AgentRunCommandQueue` 负责 AgentRun durable queue，不再由 RuntimeSession 拥有。
- `DeliveryAttempt` 负责一次队列项投递到 internal RuntimeSession 的执行尝试。
- `AgentFrameRevision / ContextDeliveryRecord` 负责 accepted surface 与实际 context delivery。
- `RuntimeSession` 只保存 internal trace。
- `AgentRunTurnAccepted / AgentRunTurnTerminal` 是推进 frame、mailbox、lifecycle node 的事实边界。

## Architecture Decisions

### AgentRun Is First-Class

`AgentRun` 是面向用户认知的事实源，也是单个 Agent 会话层级的一等聚合。重构时不应把 AgentRun 降级为 `LifecycleRun/LifecycleAgent` 的 facade。正确关系是：`LifecycleRun` 是可编排多个 AgentRun 的控制面 aggregate，用户拉起和返回工作台时看到的是入口 AgentRun。

设计影响：

- mailbox/command queue 默认是 AgentRun child fact。
- product lineage 默认从 AgentRun fork 入口收束。
- 权限、导航、workspace、conversation control 默认从 AgentRun identity 进入。

### AgentFrame Is Capability Source

`AgentFrame` 是 Agent 能力和认知状态的唯一基准事实源。RuntimeSession 可以缓存执行期投影，但不能成为 capability truth。Frame 内部可以继续整理，例如区分 capability state、context surface、visible workspace refs、launch binding refs，但这些整理必须保持 Frame 作为判断基准。

### RuntimeSession Is Internal

`RuntimeSession` 不应继续有任何产品外露信息或产品操作入口。它只描述 AgentRun 内部的当前会话流、event stream、execution status 和 connector turn lifecycle。raw Session route、前端 raw Session service、Session fork/rollback/tool approval 等用户操作入口都应删除或内部化。

### Admission Is Atomic

AgentRun start/fork 不能再由 API、receipt、Lifecycle materialization、RuntimeSession creation、initial mailbox 和 scheduler 分散拼接。`AgentRunAdmission` 必须是一个应用用例边界，原子产出：

- LifecycleRun / LifecycleAgent 或 child AgentRun 控制面记录。
- initial AgentFrame revision。
- immutable RuntimeSessionExecutionAnchor 或 delivery trace ref。
- initial mailbox envelope。
- outer command receipt accepted refs。

API 层只调用 admission，不调度首条消息，不负责补偿半成品。

### Accepted Turn Is The Commit Boundary

RuntimeSession connector accepted 不能先写 trace 再 best-effort 回写 AgentRun。accepted boundary 必须同时完成：

- accepted turn / delivery attempt 状态。
- AgentFrame commit 或 applied frame binding。
- mailbox item accepted refs。
- command receipt outcome。
- Lifecycle node started 事件。

任何一项失败都不能对外表现为 accepted success。

### Command Lifecycle Needs One Vocabulary

当前 `AgentRunCommandReceipt`、`AgentRunMailbox`、`SessionRuntimeCommand` 可能分别表达三件事：

```text
User instruction
  -> Queue state
    -> Queue execution operation
```

清理目标不是强行合并三者，而是让它们只覆盖各自事实：

- `AgentRunCommandReceipt`：外部用户输入指令、幂等、身份、原始输入、accepted refs。
- `AgentRunCommandQueue`：AgentRun 内部排队、暂停、排序、召回、移动、pending/consuming state。
- `RuntimeDeliveryOperation`：把某个队列项投递到 RuntimeSession 内部 stream/turn 的执行尝试、applied/failed/retry。

如果某个字段同时出现在两层，需要明确哪个是事实源，另一个是否可重建或只作为快照。

### Fork Should Collapse To AgentRun

Fork 是产品操作，应优先由 AgentRun 统一入口处理。Fork baseline 必须有单一事实源，包含 parent AgentRun、message/turn boundary、child AgentRun、child baseline、fork owner。RuntimeSession lineage 只能保留为 internal trace provenance，或作为可重建派生。

旧结构中 child model context baseline 来自 session projection，child runtime surface baseline 来自 parent AgentFrame clone，lineage 又分别存在 `session_lineage` 和 `agent_run_lineages`。这个三源结构应删除。

### Projection Must Declare Rebuildability

所有 projection/read model 必须标注：

- 是否可丢失后重建。
- 从哪些事实重建。
- 是否参与业务决策。
- 若参与业务决策，为什么不是 state/binding。

`current_delivery` 这类不能随意丢失的 current pointer 不应只叫 projection；它应被命名为 binding/state，并拥有单一写入边界。

### Permission Belongs To Control Plane

权限事实属于 AgentRun/Lifecycle 控制面。RuntimeSession trace 访问可以通过 anchor 回查控制面权限，但不能让 raw RuntimeSession 路径成为权限入口或产品控制入口。

### Lifecycle Naming

`Lifecycle` 作为总控制面名称可以保留。它的职责应被描述为 control-plane ledger/orchestrator，而不是换成更业务化的名称。若需要解释，文档使用 `Lifecycle control-plane`；代码级重命名不是本任务目标。

## Deletion Matrix

| 要删除的旧概念或组合 | 删除原因 | 替换边界 |
| --- | --- | --- |
| 业务服务依赖全量 `RepositorySet` | 隐藏 use case 真实依赖，扩大每次改动的理解面 | 小型 deps struct，例如 `ProjectAgentRunStartDeps`、`AgentRunMailboxSchedulerDeps` |
| AgentRun/Lifecycle crate 各自复制全量仓储集合 | 让组合根泄漏到业务层，并形成多套准 service locator | 单一 composition root，业务层只接收能力级 deps |
| AgentRun start 直接持有 lifecycle/frame/anchor/runtime creator 底层仓储 | start 用例越过 launch port 自行拼装控制面事实 | `ProjectAgentLifecycleLaunchPort` + receipt + command queue |
| 多处手写 anchor/current_delivery 同步 | `RuntimeSessionExecutionAnchor` 与 `LifecycleAgent.current_delivery` 写入规则分散 | `AgentRunDeliveryBindingPort` |
| RuntimeSession builder 直接注入控制面仓储 | delivery substrate 反向依赖产品控制面事实存储 | 中性 runtime/application ports |
| AgentRun runtime route 转调 Session route handler | AgentRun public identity 正确，但内部复用让 Session 语义回流 | 共享 application runtime service，Session route 降级为 trace/diagnostic |
| raw Session 作为产品对象外露 | RuntimeSession 只能是 AgentRun 内部 stream state | 删除 raw Session 产品 route/service，只保留内部 trace/ref |
| RuntimeSession accepted 后 AgentRun commit best-effort | trace 先成功，产品事实可丢失 | accepted turn 与 frame commit 原子化 |
| ProjectAgent start 多步 saga | run/agent/frame/session/receipt/mailbox 可能半成品 | `AgentRunAdmission` |
| Lifecycle materialization 发 `NodeStarted` | allocation 被误当执行开始 | `DeliveryPrepared` 与 `AgentRunTurnAccepted` 分离 |
| `AgentFrameRepository.append_visible_*` 细碎 mutation | repository 混入局部业务命令，且 JSONB read-modify-write 容易掩盖并发语义 | frame surface command port 或 frame aggregate update |
| `SessionPersistence` 作为全量注入接口 | store 内部拆分合理，但外部注入面过宽 | 按消费者注入具体 `Session*Store` 或 runtime trace port |
| product fork 依赖 Session lineage 作为并列事实 | fork 是 AgentRun 产品操作，runtime provenance 不应并列成产品事实源 | AgentRun fork transaction + optional internal runtime trace provenance |
| fork receipt `result_json` 保存 child refs/lineage | receipt cache 变成 fork 事实源 | receipt 只保存 idempotent command outcome ref，fork record 才是事实 |

## Repository Classification Rules

### Independent Fact Source

保留独立仓储需要满足至少一条硬理由：

- 有独立身份并被外部按 ID 查询。
- 有独立状态机、锁、claim、恢复或扫描。
- 被多个父对象或多个模块引用。
- 是反向索引，例如 `runtime_session_id -> run/agent/frame`。
- 删除、权限、审计语义不属于单个父聚合的自然组成部分。

### Parent-Owned Child Fact

应合回父聚合或隐藏在父聚合内部 storage：

- 生命周期完全依附单个父事实源。
- 创建、删除、授权都不独立发生。
- 查询总是从父对象进入。
- 更新频率低，通常随父聚合 reducer 一起变化。
- 独立表没有带来索引、锁、扫描、分页或恢复收益。

### Parent-Owned Child Table

领域上仍属于父聚合，但物理表可以保留：

- 需要 claim/lock。
- 需要按状态扫描恢复。
- 需要稳定排序、分页或大 payload cleanup。
- 高频 append 或局部更新会让父聚合 JSONB 过重。

这种情况的服务入口应命名为父聚合能力，例如 `AgentRunCommandQueuePort`，而不是顶级 `MailboxRepository`。

## Candidate Evaluation

| 候选 | 初始判断 | 需要验证的事实 |
| --- | --- | --- |
| `AgentRunMailboxRepository` | 领域上应降级为 AgentRun child fact；物理表是否保留取决于队列能力 | 是否仍需要多 worker claim、expired consuming recover、ordering、payload cleanup、pause/resume 扫描 |
| `LifecycleGateRepository` | 倾向合回 `LifecycleRun` 或 orchestration state | gate 是否存在独立扫描、跨 run 查询或独立权限 |
| `AgentLineageRepository` | 倾向作为 run-scoped child fact 或由 agent parent refs 派生 | 是否需要跨 run/跨 project lineage 查询 |
| `LifecycleSubjectAssociationRepository` | 取决于 subject 反查能力 | 是否存在 subject -> run/agent 的高频入口和索引需求 |
| `AgentRunLineageRepository` | 倾向收束为 AgentRun fork slice；是否独立取决于查询和审计需求 | fork 是否能精确锚定 fixed turn/message ref，是否需要全局 fork tree 查询 |
| `AgentFrameRepository` | frame revision surface 应独立；append helper 需要删除或改名 | 如何整理 frame 内部结构，同时保持能力与认知状态唯一事实源 |
| `RuntimeSessionExecutionAnchorRepository` | 保持独立 | 反向索引语义明确 |
| `AgentRunCommandReceiptRepository` | 倾向保持独立 | 幂等命令回执按 client command 查询，生命周期不只是 run JSONB 子字段 |
| Session stores | 保持 runtime trace substrate；外部注入面收窄 | 哪些消费者只需要 event/meta/projection 子能力 |

## Commit Boundary Review

| Boundary | 当前问题 | 目标 |
| --- | --- | --- |
| AgentRun admission | start/fork 多步写入，API 参与调度 | 单一 admission use case 原子创建初始控制面和 command facts |
| Delivery attempt | mailbox、runtime command、session meta 各自定义状态 | queue item + delivery attempt process manager |
| Accepted turn | RuntimeSession trace 先成功，AgentRun commit best-effort | `AgentRunTurnAccepted + FrameCommit` 同一边界 |
| Lifecycle node start | materialization 即 started | accepted turn 推进 started |
| Fork | RuntimeSession fork 先持久化，AgentRun 后补 | AgentRun fork record 先定义 product fact |
| Context delivery | 多处手动构造 ContextFrame | `ContextDeliveryRecord` 唯一来源 |

## Projection Review

| Projection / state | 初始判断 | 需要验证 |
| --- | --- | --- |
| `sessions.last_*` | RuntimeSession trace shell，可重建 | 是否仍被业务 terminal 判断使用 |
| `lifecycle_agents.current_delivery_*` | current binding/state，不应只视为 projection | 单一写入边界、reconcile 规则 |
| AgentRun workspace projection | 产品 read model，理论可重建 | 是否存在不可重建 UI-only state 混入 |
| context projection/head/segments | RuntimeSession 内部上下文投影，按 event/compaction 重建 | rebuild 输入和 fork 初始上下文语义 |
| Lifecycle view projection | 控制面 read model，是否可重建待评估 | 是否被业务逻辑直接依赖 |

## Database Strategy

预研期允许直接修改 schema。实施时按以下顺序处理：

1. 先通过使用点清点决定仓储分类。
2. 对合回父聚合的仓储，设计 migration 将数据迁入父 JSONB 或父 owned child structure。
3. 对保留 child table 的仓储，重命名代码入口为父聚合能力，物理表只作为 implementation detail。
4. 对删除的 public route 和 DTO，直接调整调用方，不保留兼容入口。

物理表也进入删除驱动审查。若一张表只是历史上被独立出来的子状态、重复 projection、重复反向索引或错误 owner 下的派生缓存，并且不满足锁、claim、扫描、分页、恢复、审计或不可重建 state 需求，应在 WI-12 中设计删除、合并或降级 migration。

保留表的理由必须是正向理由：它保护了不可约事实、必要 child table 行为、runtime trace substrate、state/binding 或明确可重建 read model 的性能边界。

## Read Model Strategy

Lifecycle read model 只表达控制面执行事实。AgentRun workspace/presentation 统一走 AgentRun control-plane projection。RuntimeSession read model 只表达 trace/debug。

目标是让前端产品路径只从 `run_id + agent_id` 进入；`runtime_session_id` 可以作为 trace ref 和 stale guard，但不作为产品 command identity。

## Risks

- 仓储合并若早于使用点清点，可能把实际需要索引或锁的子事实错误合回 JSONB。
- 只做命名替换而不删除旧入口，会让复杂度继续叠加。
- RuntimeSession route 降级需要同步前端 trace/debug 面，避免产品能力仍偷偷走 raw session。
