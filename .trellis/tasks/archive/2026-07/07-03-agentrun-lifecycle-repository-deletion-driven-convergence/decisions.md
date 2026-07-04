# 重构决策索引

## Purpose

本文件把 `research/` 下的对抗式审查结论提升为正式重构决策。后续实现不直接从研究文档自由取用结论，而是通过本文件中的决策编号进入工作项、代码变更和验收。

研究文档继续作为证据库，保留 file:line、风险点和局部最优推导。正式执行时以本文件为准；若执行中发现代码事实推翻某项决策，需要先更新本文件，再调整对应工作项。

## Decision States

| 状态 | 含义 |
| --- | --- |
| Accepted | 已作为目标架构约束，实施中只能细化，不能绕开 |
| Conditional | 方向已接受，但物理形态或删除范围需要先完成使用点验证 |
| Open | 当前证据不足，只能作为工作项中的验证问题 |
| Rejected | 明确不进入本轮目标架构 |

## Research Absorption

| Research | 吸纳方式 | 主要决策 | 工作项 |
| --- | --- | --- | --- |
| `research/aggregate-ownership.md` | 作为聚合归属和仓储资格的主证据 | D-001, D-002, D-016, D-017 | WI-00, WI-10, WI-11 |
| `research/runtime-session-internal-model.md` | 作为 RuntimeSession 内部化、trace store、port 拆分的主证据 | D-003, D-004, D-010, D-015 | WI-01, WI-02, WI-06 |
| `research/command-mailbox-delivery.md` | 作为 command / mailbox / delivery 三层事实的主证据 | D-005, D-006, D-007, D-008 | WI-03, WI-04, WI-05 |
| `research/wi-04-command-mailbox-current-state.md` | 作为当前 CommandReceipt、Mailbox、runtime delivery/commands 使用点和 schema owner 偏差的 file:line 清点证据 | D-005, D-006, D-007, D-017 | WI-04, WI-12 |
| `research/agentframe-context-surface.md` | 作为 AgentFrame surface 和 ContextDelivery 的主证据 | D-011, D-012 | WI-05, WI-07 |
| `research/fork-lineage-baseline.md` | 作为 fork baseline 和 lineage 收敛的主证据 | D-013 | WI-08 |
| `research/projection-permission-api-frontend.md` | 作为 product identity、permission、API/frontend cleanup 的主证据 | D-014, D-015 | WI-01, WI-09 |
| `research/database-physical-design.md` | 作为 JSONB / child table / independent table 选择规则的主证据 | D-016, D-017 | WI-10, WI-12 |
| `research/wi-10-lifecycle-storage-usage-inventory.md` | 作为 Lifecycle context、view projection、gates、subjects、lineage、orchestrations、tasks、execution log 的 file:line 清点证据 | Q-001, Q-002, Q-003, D-016, D-017, D-019 | WI-10, WI-12 |
| `references/adversarial-first-principles-review.md` | 作为 P0/P1 偏离和删除优先级的总证据 | D-001 到 D-019 | 全部工作项 |
| `inventory.md` | 作为 WI-00 清点结果和执行前开放决策回填 | D-001 到 D-019 | 全部工作项 |

## Decisions

### D-001 AgentRun 是产品一等聚合

状态：Accepted

`AgentRun` 是用户可见工作区、单 Agent 会话身份、用户输入、fork、cancel、delete、workspace read model 的默认入口。`LifecycleRun/LifecycleAgent` 可以编排和承载控制面身份，但不能把 `AgentRun` 降级成 facade。

影响：

- mailbox、command queue、fork、permission check 的产品入口默认从 `run_id + agent_id` 进入。
- 前端主状态不以 `runtime_session_id` 作为产品 identity。
- API 路由的产品写入口收束到 AgentRun scoped surface。

### D-002 LifecycleRun 是多 AgentRun 控制面聚合

状态：Accepted

`LifecycleRun` 保留命名，定位为 control-plane ledger / orchestrator。它可以包含多个 `AgentRun`，负责编排、节点进度、gates、subjects、预算和治理语义。用户主观拉起和返回工作台时仍以入口 `AgentRun` 展示。

影响：

- Lifecycle materialization 表达 prepared / allocated，不表达真实 started。
- `NodeStarted` 由 accepted turn fact 推进。
- `LifecycleRun.context` 和 `view_projection` 的具体物理形态进入 Conditional 评估。

### D-003 RuntimeSession 是内部 trace substrate

状态：Accepted

`RuntimeSession` 只描述 delivery stream、event log、connector turn、runtime status、context projection 等内部执行状态。它可以被 diagnostic/debug 面读取，但不作为产品 URL identity、产品权限入口或用户可见写控制面。

影响：

- raw session 写入口内部化或删除。
- AgentRun scoped endpoint 不再通过复用 raw session route handler 来继承产品语义。
- contracts/frontend 中 runtime session id 降级为 trace meta 或 diagnostic ref。

### D-004 SessionPersistence mega trait 拆成窄 trace ports

状态：Accepted

`SessionPersistence` 作为全量注入接口扩大了 runtime trace store 的可见面。目标是按消费者拆成 event log、meta、projection、terminal effects、runtime commands、lineage、compaction 等窄 port，并由 composition root 装配。

影响：

- application service 不再接收可触达所有 session table 的 mega trait。
- RuntimeSession 内部 store 可以保持独立物理表，但不暴露为产品聚合仓储。

### D-005 Mailbox 是 AgentRun-owned durable queue

状态：Accepted

Mailbox 领域上依附 `AgentRun`，不是 `RuntimeSession` 子状态。它承载用户意图在 AgentRun 内部的排队、暂停、排序、移动、召回、恢复和消费状态。

影响：

- mailbox owner 是 `run_id + agent_id`，不是 `runtime_session_id`。
- 删除 RuntimeSession 不应删除 AgentRun 未完成用户意图。
- 物理表可保留为 AgentRun child table，因为 queue claim、排序、恢复、扫描可能需要独立索引和锁。

### D-006 Command lifecycle 分成三层事实

状态：Accepted

命令链路采用固定词汇：

```text
User instruction / CommandReceipt
  -> AgentRun queue item / Mailbox
    -> RuntimeDeliveryOperation / DeliveryAttempt
```

`CommandReceipt` 负责外部幂等和 accepted refs；Mailbox 负责 durable queue state；Runtime delivery operation 负责一次投递到 RuntimeSession 的执行尝试。

影响：

- cancel、move、reorder、retry、failure、stale guard 必须归属到三层之一。
- receipt 与 mailbox 不形成双向事实源；receipt 可以引用 mailbox result，mailbox 只保存 nullable correlation。
- tool approval 若只是 runtime connector approval，不进入 AgentRun mailbox；若成为可恢复产品决策，再另行定义产品事实。

### D-007 AgentRunAdmission 是 start/fork 原子边界

状态：Accepted

ProjectAgent start / AgentRun start / fork materialization 必须通过 admission 用例原子创建控制面和初始事实。API 层不负责首条消息调度，也不拼装半成品补偿。

最小产物：

- LifecycleRun / LifecycleAgent 或 child AgentRun control records。
- initial AgentFrame revision。
- immutable RuntimeSessionExecutionAnchor 或 delivery trace ref。
- initial mailbox envelope。
- outer command receipt accepted refs。

### D-008 Accepted Turn + FrameCommit 是同一提交边界

状态：Accepted

Runtime connector accepted 不能先于 AgentRun frame/current surface commit 成为对外成功。accepted turn、frame commit、mailbox accepted refs、command receipt outcome、delivery attempt 状态、Lifecycle node started 应在同一业务边界成功或失败。

影响：

- noop accepted launch commit 不能是生产路径。
- 失败恢复以 AgentRun accepted boundary 为准，而不是以 runtime event 已写入为准。

### D-009 Lifecycle node state 由 AgentRun facts 推进

状态：Accepted

Lifecycle materialization 只表达 delivery prepared / runtime allocated。`NodeStarted` 来自真实 `AgentRunTurnAccepted`，terminal 状态来自真实 terminal fact。

影响：

- Lifecycle control-plane 不再把 runtime allocation 误当执行开始。
- orchestration projection 可以由 accepted/terminal facts 重建。

### D-010 ExecutionAnchor 是 immutable evidence，current delivery 是 AgentRun binding

状态：Accepted

`RuntimeSessionExecutionAnchor` 是 runtime trace 到控制面的 durable evidence，应该 insert-once / idempotent create。current delivery selection 不能通过改写 anchor 坐标或在身份聚合里混入 live pointer 实现。

执行结论：

- current delivery 落为 `AgentRunDeliveryBinding` 或等价 AgentRun child state。
- `LifecycleAgent.current_delivery_*` 删除。
- anchor 只记录 runtime_session 到 run/agent/launch frame 的 immutable evidence。
- delivery selection 从 AgentRun binding 开始，再校验 anchor。

### D-011 AgentFrame 是 append-only capability/cognition surface

状态：Accepted

`AgentFrame` 是 Agent 实际能力与认知状态的基准事实源。RuntimeSession 可以缓存 execution projection，但不能成为 capability truth。Frame revision append-only；历史 revision 不被 runtime path 原地追加 visible refs 或覆盖 capability/VFS/MCP surface。

影响：

- `AgentFrameRepository` 保留 revision surface 价值。
- 细粒度 mutation helper 需要改成 frame aggregate update 或 surface command port。
- current/applied frame 不能只由最高 revision 推断。

### D-012 ContextDeliveryRecord 是 ContextFrame emission 的唯一输入事实

状态：Accepted

ContextFrame emission 需要来自唯一 accepted input fact。目标形态是 `ContextDeliveryRecord` 或等价记录，用于同时解释 connector input、frame revision、runtime turn 和 ContextFrame 输出。

影响：

- launch、commit、transition、compaction 不再各自构造 ContextFrame 事实。
- ContextFrame 可追溯到 accepted turn 和 applied frame。

### D-013 Product fork 以 AgentRunForkRecord 为唯一事实

状态：Accepted

Fork 是 AgentRun 产品操作。canonical fork record 精确记录 parent AgentRun、fixed turn/message boundary、child AgentRun、child baseline、fork owner。RuntimeSession lineage 只保留 internal trace provenance 或可重建派生。

影响：

- product fork transaction 不以 RuntimeSession fork 为第一持久事实。
- fork replay 读取 fork record，不读取 receipt result cache。
- `agent_run_lineages` 不强制 runtime session id 成为 product lineage identity。

### D-014 Projection 必须声明可重建性

状态：Accepted

所有 read model / projection 必须声明是否可重建、由哪些事实重建、是否参与业务决策。参与决策且不可丢失的 projection 应改名为 state / binding。

影响：

- `sessions.last_*`、workspace snapshot、context projection/head/segments、Lifecycle view projection、resource surface summary 都进入标注清单。
- terminal 判断、delivery selection、permission check 不依赖可丢失 projection。

### D-015 Permission 和 product API identity 收束到控制面

状态：Accepted

权限事实属于 AgentRun / Lifecycle 控制面。RuntimeSession trace 访问可以通过 AgentRun/Lifecycle 权限派生，但 raw session route 不成为权限入口。

影响：

- AgentRun workspace、command、tool interaction、fork/cancel/delete 走 AgentRun scoped API。
- raw `/sessions/*` 保留为 diagnostic/debug surface 时，不承载产品写语义。
- frontend product identity 使用 `AgentRunRef`，runtime trace meta 只作辅助。

### D-016 仓储资格按事实所有权判定

状态：Accepted

仓储分类只有五类：independent fact source、parent-owned child fact、parent-owned child table、application command/query port、runtime trace store。不能因为已有表或已有 repository trait 就默认保留顶级仓储。

影响：

- `AgentRunMailboxRepository`、`LifecycleGateRepository`、`AgentLineageRepository`、`LifecycleSubjectAssociationRepository`、`AgentRunLineageRepository`、`AgentFrameRepository` mutation helpers、`SessionPersistence` 都必须被分类。
- 服务依赖以能力命名，不以底层表命名。

### D-017 物理存储由锁、扫描、查询和重建需求决定

状态：Accepted

独立表、child table、JSONB 的选择由事实需求决定：

- 独立事实源：独立生命周期、独立身份、跨父引用、独立权限或审计。
- child table：依附父聚合，但需要锁、claim、扫描、排序、分页、大 payload cleanup 或高频局部更新。
- JSONB / embedded state：完全依附父聚合，低并发、低查询独立性、随父对象整体读写。

影响：

- “依托单个父事实源”不是自动 JSONB；队列和索引需求可以使其保留 child table。
- “已有独立仓储”不是自动独立事实源；若只是 child table implementation，应隐藏到父聚合能力 port 下。

### D-018 RepositorySet 只属于 composition root

状态：Accepted

大 `RepositorySet` 可以作为组合根装配细节，但不能泄漏进 application service。业务服务构造函数只接收当前 use case 需要的窄 deps。

影响：

- 删除 AgentRun/Lifecycle crate 中复制的大仓储集合。
- 跨聚合写入通过显式 command port / unit of work，不通过 service locator 临时抓仓储。

### D-019 冗余物理表可以删除或合并

状态：Accepted

本轮重构允许清理物理表冗余。判断标准不是表数量，而是事实所有权和运行需求：如果一张表没有独立生命周期、独立查询入口、锁/claim/扫描/分页需求、跨父引用、审计价值或不可重建 state 价值，就不应因为历史实现而继续保留。

允许的清理方向：

- 合回父聚合 JSONB / typed embedded state。
- 降级为父聚合 child table，并删除顶级 repository 语义。
- 删除可由 canonical facts 重建的 read projection 表。
- 合并表达同一事实的多张表或多组字段。

禁止的清理方向：

- 为了少表而合并 event log、frame revision、command receipt、runtime trace、fork record 等不可约事实。
- 把需要 claim/scan/recover/order 的 queue 类事实塞进父 JSONB。
- 删除 projection 前不定义 rebuild input 和 reconciliation 方式。

## Resolved Planning Items

| ID | 状态 | 问题 | 归属工作项 |
| --- | --- | --- | --- |
| Q-001 | Resolved | 删除 `LifecycleRun.context` 和 `lifecycle_runs.context`；permission/budget 未来若需要，必须以明确 control-plane state 重建 | WI-10, WI-12 |
| Q-002 | Resolved | `LifecycleGate` 保留为 Lifecycle-owned child table；open-by-agent、correlation、wait polling 支撑独立物理表 | WI-10 |
| Q-003 | Resolved | `LifecycleSubjectAssociation` 保留为 indexed relationship table；subject reverse lookup 支撑独立物理表 | WI-10 |
| Q-004 | Resolved | current delivery 采用 AgentRun-owned delivery binding/state；删除 `LifecycleAgent.current_delivery_*` | WI-06, WI-12 |
| Q-005 | Resolved | 本轮破坏式重命名 RuntimeSession trace tables 到 `runtime_session_*` | WI-02, WI-12 |
| Q-006 | Resolved | AgentFrame 采用 canonical typed surface document；generated/read-only projection columns 可选但不能作为写源 | WI-07, WI-12 |
| Q-007 | Resolved | tool approval 继续是 runtime connector approval；产品路径只允许 AgentRun-scoped endpoint，raw session approval 仅 diagnostic/internal | WI-01, WI-04, WI-09 |
| Q-008 | Resolved | `inventory.md` 已给 redundant table ledger 初判；WI-12 执行删除、合并、降级或保留 migration | WI-00, WI-12 |

## Change Control

后续工作项如果需要偏离 Accepted 决策，必须先补充：

- 推翻该决策的代码事实或产品约束。
- 受影响 work item。
- 替代决策编号或原决策状态变更。
- migration / API / frontend 影响。

本文件当前无阻塞实现启动的 Open/Conditional 架构问题。实现中发现新事实时，先更新本文件和 `inventory.md`，再调整对应工作项。
