# Hosted Agent Context

AgentDashboard 平台托管 Agent 会话、执行与上下文生命周期的领域语言。它用于明确会话业务事实、执行副本和派生投影之间的所有权。

## Language

**Hosted Agent**:
平台拥有并托管 Agent Session 业务与执行的内聚边界；对外提供 command、authoritative read 与 committed change。
_Avoid_: Managed Runtime、Runtime Agent

**Agent Session**:
一个平台托管会话的业务 aggregate，拥有 Operation、Mailbox、Turn、Item、Interaction、Context 与 Compaction。
_Avoid_: Runtime Thread、Journal Session

**Agent Operation**:
Agent 对一个 command 的 durable acceptance、idempotency identity 与 terminal result。
_Avoid_: Worker Job、Runtime Event

**Agent Turn**:
一个获得 Agent Session 独占执行权的 typed activity；普通执行和上下文压缩各自使用独立 Turn。
_Avoid_: Request、Worker Run

**Context Revision**:
一次 Agent 执行所使用的 immutable、typed、model-visible 上下文。
_Avoid_: Journal Replay、Presentation History

**Execution Driver**:
Hosted Agent 内部把 execution effect 映射到 Native、Codex 或 Remote provider，并返回 receipt 或 observation 的 adapter。
_Avoid_: Agent Owner、Session Source

**Agent Change**:
Agent Session transaction 提交后发布的有序变更，用于增量协议与下游投影。
_Avoid_: Source Event、Runtime Journal Fact

**Journal**:
消费 Agent Change 建立审计、搜索或分析记录的派生投影。
_Avoid_: Agent Store、Session Source of Truth
