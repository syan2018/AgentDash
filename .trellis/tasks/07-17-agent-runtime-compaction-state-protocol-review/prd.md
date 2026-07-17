# Hosted Agent 状态机收敛与压缩协议重构

## Goal

以压缩为第一个贯穿 Runtime、durable worker、driver、Codex App Server Protocol 与前端的 tracer bullet，整体收敛 Agent Runtime 的 canonical 状态模型，而不是只修复单个 replay 报错或添加一个 `Compacting` 枚举值。

收敛后的 `Hosted Agent` 是平台托管会话业务的深模块。`AgentSession` aggregate 拥有 Session/Turn/Item/Interaction、Operation、Mailbox、Context 与 Compaction；Runtime/Host/worker 是 Agent 内部的执行协调实现；driver 是 execution adapter。Application 通过 Agent boundary 执行命令、读取会话和订阅 committed change。

Journal 与 Agent 解耦，只消费 Agent 已提交的 change 形成协议通知、审计、搜索或分析投影。Session read、command admission、fork、context materialization、compaction recovery 与 availability 不得从 Journal replay 或 cursor 推导。

用户应能从会话状态与 Codex App Server Protocol 事件中明确判断当前是 idle、普通 Agent Turn、上下文压缩、失败恢复还是不可继续状态。Hosted Agent 应在 durable worker 重试、进程重启和驱动恢复后收敛到唯一正确状态。

## Background

- 当前 `ContextActivationDispatch` 可因 Native context replay 遇到不支持的 typed thread item 而失败。
- 当前 `RuntimeJournalFact` 把 driver-produced Session lifecycle、producer-owned presentation 与 Runtime operation/binding/context coordination 合并为一条 authoritative journal；Runtime snapshot、AgentRun fork/feed、context materialization 与 terminal rewind 又反向从这条 journal 重建 Agent 会话，形成循环 ownership。
- Codex adapter 已能通过 `thread/read(includeTurns=true)` 读取 native Agent Session；`references/codex` 自身也以 ThreadStore/history 为 Thread 事实、以 App Server notification 为投影。当前 Runtime 选择复制后再读取自己的 journal projection，seam 方向与该参考相反。
- 当前并不是只有一套 Runtime 状态：`RuntimeThreadStatus`、`active_turn_id`/`RuntimeTurnState`、context preparation/activation、Runtime Host binding、AgentRun execution、workspace conversation execution 与 Codex `TurnStatus` 分别表达部分重叠语义，但缺少统一的所有权矩阵和组合不变量。
- `RuntimeTurnState` 只有 active/terminal phase，没有区分普通 Agent Turn 与 ContextCompaction maintenance Turn；短期 activity 只能由消费者从 `active_turn_id` 和旁路事实猜测。
- `AgentRunExecutionState` 与 `ConversationExecutionStatusModel` 再次组合出 Idle/Running/Cancelling/Terminal 等状态；其中产品前置条件可以保留为 read model，但 Runtime execution 部分必须由 canonical snapshot 单点投影。
- `RuntimeBindingState` 与 `RuntimeThreadStatus` 都包含 Active/Desynchronized/Lost/Closed 一类健康语义。它们属于不同 aggregate，不应合并成一个枚举，但必须定义谁驱动谁以及 generation fence 下的合法组合。
- `CompactionPhase` 当前表示 PreProvider/StandaloneCompactTurn/OverflowRetry 等执行位置，不是 compaction saga lifecycle；命名与后续 lifecycle phase 容易混淆。
- 旧 `SessionCompactionStatus` 仍残留在 SPI 导出中，而对应 runtime session compaction 表已由 migration `0065_agent_runtime_cutover.sql` 删除；本次应审计并删除这类 cutover 残留。
- managed compaction 已能把 `CommandExecution`、`McpToolCall`、`DynamicToolCall` 与 `AgentDashNativeThreadItem` 投影为压缩消息，但 Native replay 只显式恢复 `UserMessage`、`AgentMessage`、`Reasoning` 与 `DynamicToolCall`。
- 当前压缩主要以 context preparation / candidate / activation / checkpoint 与 durable work 表达，尚需验证其是否具备明确的会话级压缩状态、合法状态迁移、失败终态和协议事件。
- 当前 `ContextCompact` acceptance 不创建 canonical Turn/Item，也不改变 `active_turn_id`；API 却根据请求时是否存在普通 active turn 推断 `scheduled_next_turn` / `launched_compaction_turn`。
- pending compaction 不进入 command availability，普通 `TurnStart` 可在 compaction accepted 与 worker prepare 之间被接受；worker 随后把 active turn 当作错误重试，而不是执行明确的 `Queued → Running` 调度迁移。
- preparation/activation 的普通失败缺少 terminalization 入口，`CompactionTerminal::Failed` 当前没有生产构造路径，导致不可恢复错误可能无限 release/reclaim。
- `references/codex` 把压缩建模为 `TaskKind::Compact` 的非 steerable active turn，并使用相同 ID 的 `ContextCompaction` item 发出 `item/started → item/completed`；Thread lifecycle 保持 Active，已废弃的 `thread/compacted` 不再是 v2 canonical lifecycle。
- 前端已存在 `contextCompaction` card，但当前把该 item 的状态固定解释为 completed，不能表达 started/failed。
- 项目尚未上线，本任务以建立最正确的最终模型为目标，不保留错误状态模型或协议行为的兼容路径；涉及数据库结构变化时必须提供 migration。
- 用户明确允许破坏性重写从 `main` 搬运到本分支、但尚未完成架构收敛的 Runtime 实现，包括内部 interface、事件、repository、worker、driver adapter、应用投影与前端消费；不采用双写、兼容读取、旧事件兜底或并行状态机。数据库 schema 必须通过明确的 forward migration 到达最终模型。

## Requirements

- R1. 盘点压缩从触发、准备、候选生成、激活、恢复到终结的完整状态机、事实源、事务边界和 durable work 链路。
- R2. 定义会话级压缩状态及合法迁移；以 Agent-owned typed maintenance Turn 表达 active `compacting`，以 Operation/Queue 表达 queued request，避免把短期活动与 Session lifecycle/consistency 混为一个枚举，并明确它与 operation、item、binding generation、context revision 的关系。
- R3. 定义并评估 Codex App Server Protocol 压缩相关事件的生成、顺序、durability、重放和前端投影语义，至少覆盖同 ID 的 `ContextCompaction item/started → item/completed` 成功链，以及失败时 error + failed/lost turn terminal。
- R4. 统一 Agent context materialization、managed compaction 与各 execution adapter 对 typed model contribution 的支持契约，不能静默丢失历史、从 presentation item 反向解释上下文，或在外部 apply 末端才发现能力不匹配。
- R5. 定义压缩失败、worker 重试、进程重启、stale generation、部分应用和不可验证结果的恢复与收敛规则。
- R6. 评估现有数据库模型、migration、repository、runtime domain、worker、driver、API/stream 与前端状态投影的修改范围。
- R7. 为状态迁移、协议事件顺序、typed item replay parity、失败恢复和持久化重放制定可执行的验证方案。
- R8. 形成可供后续实现和检查 agent 直接执行的技术设计与分阶段实施计划。
- R9. 直接研究 `references/codex` 的压缩状态、`contextCompaction` thread item 生命周期、通知顺序与失败处理，以其真实实现而不只是生成 schema 作为对照证据；明确哪些语义应保持 Codex App Server Protocol 同构，哪些属于 AgentDash managed compaction 的扩展。
- R10. 压缩 active 期间的新用户消息由 Agent mailbox durable 接受并 deferred；Hosted Agent 禁止提前创建普通 Turn，也禁止把输入 steer 进压缩 Turn。压缩成功或外部 apply 前的 clean failure 后继续无依赖队列；post-apply `Lost/Desynchronized` 时保持阻塞。
- R11. 定义 Hosted Agent 的 canonical state space 与所有权矩阵，至少区分并组合：
  - Agent Session lifecycle/execution consistency；
  - exclusive active activity / typed Turn；
  - Turn、Item、Interaction entity lifecycle；
  - Operation idempotency lifecycle；
  - Context transition saga；
  - Agent 内部 Runtime Host binding/generation lifecycle；
  - durable work delivery lifecycle；
  - AgentRun、协议和 UI read model。
- R12. 不建立“包含所有可能组合”的巨型状态枚举。Hosted Agent domain 应拥有少量正交 state machine，并在一个 transition kernel/interface 中强制跨状态不变量、准入、排序、terminalization 与恢复决策。
- R13. AgentSession Turn 增加明确的 kind/activity 语义，至少覆盖 `Agent` 与 `ContextCompaction`；command admission、mailbox promotion、steer/cancel、协议事件和前端展示读取 Agent boundary 的该事实，不再从 Journal、worker work 或 API 调用时机推断。
- R14. 将 worker claim/retry/release 降为纯 delivery mechanism：业务 retryability、Failed/Lost terminal、是否允许继续 mailbox 必须由 Agent transition 先持久化，worker 只按 settlement decision ack/retry/dead-letter。
- R15. 审计并删除重复、失去所有权或 cutover 后残留的状态模型与协议路径；允许直接替换内部 interface 和 schema，不保留旧错误模型的兼容层。
- R16. 把整体重构拆为依赖明确、可独立验证的垂直切片；先建立 Hosted Agent contract、canonical state kernel 与 repository，再让 execution coordination、compaction tracer bullet、authoritative read/change、Codex-shaped projection、AgentRun/UI 与 recovery 依次迁移到新 interface。
- R17. 从第一性原理删除可由其他 durable fact 唯一派生的重复状态：activity 由 AgentSession active Turn 及其 kind 派生；compaction 由 Agent-owned aggregate phase 表达；worker claim 不进入 domain；API/UI 从 Agent read/change 投影，不读取 canonical journal。
- R18. 收敛 Agent 内部 execution effect interface：普通 driver dispatch、必要的 context replica apply/inspect 使用 stable effect identity 和统一 delivery/observation settlement，不再通过多个专用 claim/status 表形成旁路业务状态机。
- R19. compaction 成功 observation、AgentSession active context head、Compaction/Item/Turn/Operation terminal 必须在同一 Agent transaction 收敛；只有 stateful execution replica 的外部 apply 无法与 Agent repository 原子提交时才保留内部 recovery phase。
- R20. `CompactionSucceeded` 只释放 active slot 并解除 continuation dependency，绝不隐式创建 Agent Turn。自动 overflow recovery 必须先建立独立 durable continuation request；压缩成功后由一次独立 mailbox promotion 以新的 `TurnStart` operation、新的 Turn ID 和新 context revision 提交。手动压缩不创建 continuation request，成功后保持 idle。
- R21. 普通 Agent Turn active 时允许手动压缩 durable 排队为 `Queued`。该状态不创建伪 Turn/Item；当前 Turn terminal 后，Hosted Agent 必须在释放 active slot 的同一 transaction 中选择 queued compaction 并开始真正的 ContextCompaction Turn/Item。queued 期间的新消息只进入 Agent mailbox；Session Lost/Desynchronized 时不得启动 queued compaction。
- R22. automatic compaction clean `Failed` 时，与其 `blocked_by_compaction_id` 关联的 continuation request 必须 exactly-once terminalize 为 `Failed`，不得使用旧 context promotion、自动重建或形成 recovery loop；Agent Session 保持 Open/Synchronized，其他无依赖 mailbox entry 可继续。compaction `Lost` 时关联 continuation 进入 Lost/blocked，Agent Session execution consistency 进入 Lost，全部 mailbox promotion 阻塞。
- R23. 删除 Runtime journal 作为 Agent 事实源的设计。AgentSession repository 是 Session/Turn/Item/Interaction/Context/Compaction 的权威读取面；Journal、App Server notifications、analytics 与 audit 只消费 Agent committed change，不参与 command admission、fork、context materialization、terminal 判断或 recovery。
- R24. Driver contract 返回 execution receipt/observation，不得返回 `RuntimeJournalFact` 或直接写 Agent entity。Native/Codex/Remote adapter 的 source identifiers 和 native session state只作为 binding/replica coordinate，经 Hosted Agent 验证后才能形成 Agent transaction。
- R25. Session reconnect 使用 Agent snapshot revision + change tail；若 change cursor gap，重新读取 Agent snapshot。Fork cutoff 使用 stable Session/Turn/Item identity 或 immutable Agent revision，不再拼接、截断或重新编号 presentation journal。

## Constraints

- C1. 项目尚未上线，以最终正确模型为目标；不提供旧 Runtime journal/state/schema/API 的兼容读取、双写、fallback 或数据 backfill。
- C2. 数据库结构必须通过 forward migration 到达最终 schema，并验证从实施时前一 migration 升级后的约束。
- C3. 普通 Agent、manual compaction、automatic overflow continuation 都必须遵守“一个 command 对应一个 Operation；一个获得 active slot 的 activity 对应一个独立 Turn”的 identity 规则。
- C4. queued command 可以 durable 接受，但 queued 状态不创建 Turn/Item，也不产生 fake App Server lifecycle event。
- C5. 本任务规划包可提交和推送，但在用户或后续执行方审阅并明确批准前不得运行 `task.py start` 或修改实现代码。
- C6. 工作区并行修改不属于本任务；实施与检查必须按文件 ownership 避免覆盖其他会话。

## Out of Scope

- ACP 或其他尚无真实调用方的 execution driver；本次只定义可供未来 adapter 实现的统一 port。
- 对通用审计、搜索和 analytics 产品能力做功能扩展；本次只把它们放到 Agent Change 下游。
- 保留或迁移尚未上线的旧开发数据、旧 Runtime journal event、旧 API field 与旧前端 reducer 状态。
- 把 Codex vendor DTO 直接作为 Agent domain type；只保持必要的 App Server protocol 语义同构。
- 在本规划任务中启动实施、运行全量开发环境或执行数据库 destructive migration。

## Acceptance Criteria

- [ ] AC1. 现状研究明确列出压缩生命周期每一阶段的事实源、状态所有者、持久化记录、worker command、driver command 和外发协议事件，并附代码证据。
- [ ] AC2. `design.md` 给出 Hosted Agent 唯一权威的目标状态机，包括状态、命令、committed change、合法迁移、不变量、失败终态、幂等键与恢复决策。
- [ ] AC3. `design.md` 给出 Codex App Server Protocol 压缩事件契约，包括事件名称/载荷、开始与完成边界、失败表达、durability、排序和 replay 语义。
- [ ] AC4. `design.md` 解决 typed thread item 支持集合不一致问题，并定义在 Agent entity commit、context preparation 或 execution dispatch 之前完成typed分类与能力校验的契约。
- [ ] AC5. `design.md` 明确数据库 schema 与 migration、Runtime/API/driver/frontend 各层的改动边界，不采用兼容或回退方案。
- [ ] AC6. `implement.md` 将改造拆成顺序明确、可独立验证的执行切片，包含测试命令、风险点和检查门禁。
- [ ] AC7. `implement.jsonl` 与 `check.jsonl` 包含真实的 spec/research 上下文条目，能够支持 Trellis 子 agent 实施与检查。
- [ ] AC8. 规划产物经过用户审阅并明确批准后，才进入实现阶段。
- [ ] AC9. 现状研究包含 `references/codex` 的源码级压缩状态机与事件时序，并把对照结论落实到目标状态机和协议事件契约。
- [ ] AC10. 并发测试证明压缩 active 期间的新用户消息只进入 Agent mailbox、不创建普通 Agent Turn；成功或外部 apply 前的 clean failure 后按顺序继续无依赖队列，Lost/Desynchronized 后不启动。
- [ ] AC11. `design.md` 提供完整的状态所有权矩阵，逐一说明每个状态属于 AgentSession、Agent operation/queue、Agent internal Runtime/Host、execution effect 或 derived Journal/protocol projection；并说明权威 repository、transition owner、允许读取者与禁止承担的职责。
- [ ] AC12. `design.md` 给出正交状态机及其组合不变量，而不是单一巨型枚举；至少覆盖 active Turn exclusivity、Turn child terminal、Operation terminal、binding generation、context head CAS 与 mailbox release 条件。
- [ ] AC13. 所有 Agent command availability 与 AgentRun execution projection 都从同一 Agent read/decision interface 生成；不存在 Journal、API、worker或前端基于间接信号重建业务状态的路径。
- [ ] AC14. 规划列出应直接删除或替换的旧 state/interface/schema/protocol 清单，并为数据库最终模型提供一次 hard-cut forward migration 与迁移后约束验证。
- [ ] AC15. 实施拆分保留一个端到端 compaction tracer bullet：从 Agent mailbox admission 到 Agent ContextCompaction Turn、context commit/必要的 execution replica convergence、协议 item lifecycle、UI 展示及 deferred message 恢复均由同一 Hosted Agent 状态链驱动。
- [ ] AC16. `design.md` 包含第一性原理、被拒绝的替代方案、最小持久状态、deep transition interface、effect settlement 与最终 schema；每个新增概念都能对应至少一条不可删除的基本事实。
- [ ] AC17. 目标模型不存在以下重复事实：authoritative Runtime journal、独立持久化的 activity enum、worker 推断的业务 phase、driver activation 侧第二套 transcript mapper、API 推断的 compaction started、前端固定 completed。
- [ ] AC18. PostgreSQL 与 in-memory adapter 通过同一 Hosted Agent interface behavior suite，且真实数据库测试证明 active slot、queue/compaction singleton、effect identity、context revision 与 terminal commit 原子性。
- [ ] AC19. 测试分别证明：manual compaction `Succeeded -> Idle` 且没有后继 Agent Turn；automatic overflow flow 中 Agent Turn A、Compaction Turn B、Continuation Agent Turn C identity 全部不同，B 的 terminal commit 不包含 C 的 start，只有 durable continuation request 被 mailbox 独立 promotion 后才创建 C。
- [ ] AC20. 并发测试证明 active Agent Turn 期间 manual compact 返回 `Queued`；Turn terminal 与 compaction start/ItemStarted/preparation effect 在一个 Agent transaction 中原子发生，mailbox message 无法抢占空窗，duplicate command 幂等，第二个不同 compact command typed 拒绝，Lost terminal 不启动 queued compaction。
- [ ] AC21. automatic failure 测试证明：B clean Failed 后 C Failed exactly once 且不存在 Agent Turn C；其他无依赖 mailbox entry 仍可 promotion。B Lost 后 C Lost/blocked、Agent Session consistency=Lost 且所有 mailbox promotion 被拒绝。重复 application effect、worker reclaim 与重启不复制 B/C 或创建后继 Turn。
- [ ] AC22. 删除测试证明：移除 Agent Journal 后，Agent read/resume/fork/compact/context/recovery 与 App Server snapshot+tail 仍成立；任何 Session query、context materialization、fork cutoff、terminal/rewind 或 command availability 都不调用 `journal_records_after`。
- [ ] AC23. Driver conformance 测试证明 adapter 只产生 execution observation；contract/wire/schema 中不存在 `RuntimeJournalFact`，且 unknown、duplicate 或 stale observation 不能直接写 Agent entity。
