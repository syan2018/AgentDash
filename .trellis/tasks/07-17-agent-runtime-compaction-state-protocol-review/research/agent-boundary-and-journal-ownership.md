# Agent Boundary 与 Journal 权威性重评

## 结论

当前“Managed Runtime journal 是 Thread/Turn/Item/Interaction/Context 的唯一事实源”这一前提应被推翻。

目标不是把事实权威交给 Codex、Native 或 Remote driver，而是建立一个平台托管的 `Hosted Agent` 深模块：

- `AgentSession` 是平台托管的会话 aggregate；
- Agent 模块拥有 Session、Turn、Item、Interaction、Operation、Mailbox、Context 与 Compaction 的业务语义；
- execution driver 是 Agent 模块内部的执行 adapter，不拥有平台业务状态；
- Runtime 只作为 Agent 模块内部的调度、effect delivery、binding/recovery 实现，不再作为面向 Application 的第二个业务世界；
- Journal 只消费 Agent 已提交的 change，用于协议通知、审计、搜索或分析；它不是 command admission、恢复、context materialization 或 session query 的输入。

这一调整保留了 durable acceptance、generation fencing、effect settlement 和 failure recovery 的必要复杂度，但删除了“先把 Agent 事实翻译成 Runtime fact，再从 Runtime journal 重建 Agent”的循环。

## 1. 第一性原理

### F1. 会话业务必须只有一个 owner

同一个 Session 的 Turn、Item、Interaction、上下文 head 和压缩结果必须由一个 cohesive module 提交。否则“当前 active Turn”“模型实际看到的上下文”和“UI 展示的 Thread”会在不同 writer 之间分叉。

要求的最小机制是 `AgentSession` aggregate 与其 repository，不是 append-only journal。

### F2. 平台托管 Agent，而不是托管 driver notification 的副本

Driver notification 是执行 observation。它可以触发 Agent transaction，但不能直接成为平台业务事实。Driver 也不应被要求构造 `RuntimeJournalFact`，因为这迫使每个 adapter 理解 Runtime persistence、canonical ID、presentation durability 和内部状态机。

### F3. Durable command acceptance 与 conversation record 是两类事实

Operation、queue、continuation dependency 和 effect identity 解决异步命令与故障恢复；Turn、Item、Interaction 和 Context 解决会话业务。它们可以在同一个 Agent transaction 中保持不变量，但不需要被压成一条 event journal。

### F4. 查询当前事实与订阅变化是不同需求

`read` 必须从 Agent boundary 返回当前 authoritative Session。`subscribe` 只提供 committed change。断线重连使用“读取 revision R 的 snapshot，再消费 R 之后的 change”；change cursor 发生 gap 时重新读取 snapshot，而不是假设永久 journal 是唯一恢复路径。

### F5. Journal 的存在必须通过删除测试

删除 Journal 后：

- Agent 仍能 read/resume/fork/compact；
- command admission 与 availability 仍能正确；
- context materialization 与 continuation recovery 仍能正确；
- App Server 可以从 snapshot + committed change 恢复当前 UI。

删除 AgentSession repository 后，Journal 不能被允许接管上述能力。否则 Journal 仍是伪装后的事实源。

### F6. 外部副作用不要求复制 conversation truth

Runtime/driver 之间仍存在 dispatch、timeout、crash 和 unknown observation。这要求稳定 effect identity、inspect、generation fence 和 `Lost`，但这些属于 Agent 内部 execution coordination。它们不能证明需要一份包含所有会话内容的 Runtime journal。

## 2. 当前实现为何越权

### 2.1 一个 union 混合了三个 owner

`crates/agentdash-agent-runtime-contract/src/event.rs:702-707` 把 producer-owned presentation 与 Runtime internal lifecycle 放入同一个 `RuntimeJournalFact`。

`crates/agentdash-agent-runtime-contract/src/event.rs:725-727` 又把该 record 明确定义为 authoritative journal。

结果是：

- Agent/Codex 产生的 Session presentation；
- Runtime 自己的 Operation/Binding/Context 协调；
- App/Tool producer 追加的 presentation；

共享同一个 sequence、revision 和 persistence carrier，但它们的 owner、重建能力和失败语义并不相同。

### 2.2 Driver 被迫生产 Runtime persistence fact

`crates/agentdash-agent-runtime-contract/src/driver.rs:121-139` 的 `DriverEventEnvelope.facts` 直接携带 `Vec<RuntimeJournalFact>`。

Codex adapter 在 `crates/agentdash-integration-codex/src/driver.rs:1587-1605` 同时拼装 Runtime internal event 与 presentation fact；Native adapter也采用相同模式。Driver 因而不再只是执行 adapter，而被迫参与 Runtime journal schema 与 canonical state transition。

### 2.3 Runtime snapshot 读取自己的副本，而不是 Agent

`crates/agentdash-agent-runtime/src/gateway.rs:1691-1743` 的 Thread snapshot 只读取 `RuntimeRepository.load_thread` 并返回 `state.snapshot()`。

与此同时，Codex adapter 已经能在 `crates/agentdash-integration-codex/src/driver.rs:774-799` 通过 `thread/read(includeTurns=true)` 得到 agent session projection。当前架构选择复制后再读副本，而不是让统一 Agent boundary 拥有 read contract。

### 2.4 Journal 已成为模型上下文输入

`crates/agentdash-infrastructure/src/persistence/postgres/agent_runtime_context_broker.rs:70-158` 从 `journal_records_after` 加载 transcript，并通过 journal 内的 `ContextCheckpointActivated` 反查 active compaction boundary。

这使一个 presentation/reconnect log 反过来决定 provider-visible context，也是原始报错“native context replay 遇到 unsupported typed thread item”的结构性前提：Runtime 先保存 presentation item，activation adapter 再尝试把它解释回 agent context。

### 2.5 Journal 已成为 fork 与产品 Session 的事实源

`crates/agentdash-application-agentrun/src/agent_run/journal.rs:167-361`：

- 从 Runtime journal 拼出 AgentRun Session；
- 通过拼接 parent journal prefix 实现 fork；
- 重新编号 visible sequence；
- 将 Runtime internal gap 与 presentation cursor 组合成产品 stream。

Fork 应由 AgentSession 通过 stable Session/Turn/Item 或 immutable revision 建立，不应由 read-side feed 拼接历史记录得到。

### 2.6 Journal 已成为 context 与 terminal projection 的数据库

`crates/agentdash-application-agentrun/src/agent_run/context_projection.rs:42-215` 从 journal 中的 stringly `SessionMetaUpdate("context_compacted")` 重建压缩 archive 与模型上下文 read model。

`crates/agentdash-application-agentrun/src/agent_run/runtime_application_presentation.rs:59-136` 扫描 prior journal records 判断 rewind 的稳定点。

这些查询都应读取 AgentSession 的 Context、Compaction 与 Turn terminal，而不是解释协议投影。

## 3. `references/codex` 给出的相反设计证据

Codex App Server 没有把 notification stream 当作 Thread 真相：

- `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:2263-2334` 的 `thread/read` 从 persisted metadata、ThreadStore history 与 live state 组合 Thread；
- `references/codex/codex-rs/app-server/src/request_processors/thread_processor.rs:2643-2698` 的 `thread/items/list` 直接读取 ThreadStore；
- `references/codex/codex-rs/core/src/session/mod.rs:2979-3017` 的 compaction 先替换 Session history并持久化 `CompactedItem.replacement_history`，protocol `item/completed` 是该业务提交之后的通知；
- `references/codex/codex-rs/core/src/compact_remote_v2.rs:207-322` 把 ContextCompaction item lifecycle、replacement history 安装与 token state 都放在同一个 Session owner 中。

Codex 的具体 persistence 仍有自己的权衡，但 ownership 方向是正确的：Session/ThreadStore 是事实，App Server notification 是投影。

## 4. 分支可行性

当前分支相对 `main` 已改动约 1210 个文件；`agent-runtime-contract`、managed runtime schema、driver host 和 session presentation chain 本身就是该分支新建的架构，不构成已上线兼容约束。

直接引用 `RuntimeJournalRecord`、`RuntimeJournalFact`、`journal_records_after` 或 `agent_runtime_event` 的范围约 50 个文件，主要集中在：

- `agentdash-agent-runtime`；
- `agentdash-agent-runtime-contract`；
- `agentdash-infrastructure`；
- `agentdash-application-agentrun`；
- Native/Codex/Remote driver adapter；
- API、wire/schema 与前端生成类型。

因此应直接替换 ownership 与 schema，不增加兼容 facade、双写或旧 journal reader。

## 5. 目标所有权矩阵

| 事实 | 权威 owner | 持久化 | Journal 角色 |
| --- | --- | --- | --- |
| Agent definition / surface | Agent/Application composition | definition/frame repository | 可选配置审计 |
| AgentSession identity、name、health | Hosted Agent | `agent_session` | 投影 |
| Turn、Item、Interaction | Hosted Agent | entity repository | App Server notification/审计投影 |
| Operation、queue、continuation dependency | Hosted Agent | request/operation repository | 可选 operation feed |
| Context revision、checkpoint、compaction | Hosted Agent | context/compaction repository | 展示与诊断投影 |
| active execution slot、admission | Hosted Agent | Session aggregate state | 不参与决策 |
| driver binding、generation、replica health | Agent 内部 Runtime/Host | binding repository | 诊断投影 |
| effect claim、attempt、lease | Agent 内部 delivery implementation | effect ledger | 不投影为业务状态 |
| provider/driver observation | execution adapter 输入 | observation settlement 后并入 Agent transaction | 原始 telemetry 可单独记录 |
| App Server protocol event | protocol projector | 可重放 projection 或 transient stream | Journal 可以承载，但不是事实源 |
| analytics/audit entry | Journal module | 独立 projection store | 这是 Journal 自己的事实 |

## 6. 目标深模块与 seam

Application 只看到一个 Agent seam：

```rust
trait HostedAgentGateway {
    async fn execute(
        &self,
        command: AgentCommandEnvelope,
    ) -> Result<AgentOperationReceipt, AgentExecuteError>;

    async fn read(
        &self,
        query: AgentQuery,
    ) -> Result<AgentReadResult, AgentReadError>;

    async fn changes(
        &self,
        subscription: AgentChangeSubscription,
    ) -> Result<AgentChangeStream, AgentSubscribeError>;
}
```

Interface 的含义必须是：

- `execute` 由 Agent transaction 完成 durable acceptance；
- `read` 读取 AgentSession aggregate/repository，不 replay Journal；
- `changes` 发布 transaction 已提交的 `AgentChange`，不能接受命令或推进状态；
- driver、repository、mailbox、compaction、effect ledger 和 protocol projector 都是 module implementation 或 module 外部消费者，不暴露给 Application。

Agent 内部保留真实的 ports：

```rust
trait AgentExecutionPort {
    async fn dispatch(
        &self,
        effect: AgentExecutionEffect,
    ) -> Result<AgentExecutionReceipt, AgentExecutionError>;

    async fn inspect(
        &self,
        query: AgentExecutionInspection,
    ) -> Result<AgentExecutionObservation, AgentExecutionError>;
}
```

Native、Codex 与 Remote 是该 port 的 adapters。它们返回 observation，不返回 `RuntimeJournalFact`。

## 7. 目标持久化形状

推荐直接建立 Agent-owned normalized state：

- `agent_session`
- `agent_session_operation`
- `agent_session_queue_entry`
- `agent_session_turn`
- `agent_session_item`
- `agent_session_interaction`
- `agent_session_context_revision`
- `agent_session_context_checkpoint`
- `agent_session_compaction`
- `agent_execution_binding`
- `agent_execution_effect`

不保留 authoritative `agent_runtime_event`。

为 snapshot + tail、异步 protocol delivery 或审计需要的 change，使用 Agent transaction 同时写入的通用 `agent_change_outbox`：

- 它只负责可靠发布；
- payload 是 `AgentChange`，不是完整 aggregate replay 的唯一材料；
- consumer lag/gap 可以通过重新 `read` AgentSession 修复；
- retention 可以独立裁剪；
- Journal projector 可以消费它并建立自己的索引或审计记录。

## 8. 对压缩状态设计的影响

前序已确认的产品语义继续成立，但 owner 改为 Hosted Agent：

1. active Agent Turn 期间，manual compact 先成为 Agent operation/queue entry，不提前创建 Turn。
2. 当前 Turn terminal 后，Agent Session admission 原子选择 queued compaction。
3. 只有 Agent 真正接受 compaction request 后才创建 `Turn(kind=ContextCompaction)` 与同 ID item lifecycle。
4. Compaction 成功只释放 admission；manual 不自动创建后续 Turn。
5. automatic overflow 的 continuation 是独立 queue entry；成功后由 Agent admission 创建新的 Agent Turn。
6. clean Failed terminalize continuation；Lost 将 Session execution condition 置为 Desynchronized 并阻塞 queue。
7. App Server `turn/started`、`item/started`、`item/completed`、`turn/completed` 来自 Agent committed change 的 protocol projection，不由 Journal reducer或前端猜测。

Compaction 是否需要 `Activating` 取决于 Agent execution adapter：

- 若每个 execution effect 显式携带 immutable context revision，Agent transaction 安装新 context 后无需第二个持久化 activation 状态机；
- 若 stateful driver 必须持有 context replica，则 replica apply/inspect 是 Agent 内部 execution saga；`Activating/Lost` 描述 replica convergence，不能让 replica 或 Journal成为 conversation truth。

## 9. 被拒绝的替代方案

### Runtime journal event sourcing

拒绝。它强制 presentation、Agent business 与 execution coordination 共用事实模型，并已经让 context/fork/read 依赖 projection log。

### Driver-owned conversation

拒绝作为平台通用语义。Codex/Remote 可以在 adapter 内持有 native session replica，但平台 Agent boundary 必须拥有统一 Session contract、operation acceptance 与 failure semantics。无法恢复或验证的 replica 只能声明较低 fidelity 或进入 Lost。

### Agent state + authoritative Journal 双写

拒绝。双写会重新引入冲突解决问题。Agent state 是权威；change outbox 与 Journal 是派生物。Journal failure 不能改变已经提交的 Agent Session。

### 仅给现有 Runtime journal 加一层 Agent facade

拒绝。删除测试不通过：context、fork、terminal 与 snapshot 仍从 Journal 读取，facade 只会隐藏错误 ownership，而不会消除它。

## 10. 后续设计门禁

在更新 `design.md` 前必须逐项证明：

1. 所有 session/query/context/fork 读取都通过 Agent boundary；
2. driver contract 不再暴露 `RuntimeJournalFact`；
3. Agent transaction 不依赖 Journal cursor；
4. protocol projector 可以从 Agent snapshot + change 重建；
5. compaction、mailbox 与 continuation 在 Agent aggregate 内保持原子不变量；
6. Runtime/Host/worker 只保留 execution coordination，不再定义第二套 Thread/Turn/Item。
