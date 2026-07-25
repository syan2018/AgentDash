# Agent Conversation Architecture

## 1. Scope / Trigger

本规范定义 AgentRun Product 坐标如何读取和控制 concrete Agent conversation。新增 input、
steer、interrupt、interaction、context、compaction、fork、snapshot 或 live stream 时复核。

Product 拥有 Lifecycle/AgentFrame/association；concrete Agent 拥有 conversation history、
context、fork、compaction 与 execution effect。Runtime 只做进程内协议协调。

## 2. Signatures

```text
Product target + client identity
  -> LifecycleAgent association
  -> current Complete Agent service/source
  -> execute / inspect
  -> Agent-owned receipt and history

conversation read
  -> Complete Agent read(source)
  -> in-memory normalize
  -> Product waiting facts composition
  -> frontend snapshot baseline

live
  -> committed Agent history suffix + Core ephemeral callback
  -> process-local broadcast
  -> frontend canonical lane
```

## 3. Contracts

- Product input通过 `AgentRunProductInputDeliveryPort` 同步交接。成功必须包含 concrete Agent
  operation receipt；Agent不可用时当前请求失败。
- 相同 target/client identity/payload 派生同一 handoff/effect；不同 payload复用 identity是
  typed conflict。
- conversation snapshot来自 `CompleteAgentService::read(source)`，并在内存中投影为平台
  conversation contract。
- LifecycleGate waiting items等Product事实可以与snapshot组合展示，但不写入Agent history，
  也不形成第二份conversation。
- live event承载刚提交的 durable Agent history record 与当前连接的 ephemeral delta。durable record
  由 snapshot 使用的同一个 canonical projector产生；断线、gap或lag后丢弃partial lane并重新read
  snapshot。
- 输入接纳的可观察顺序固定为 durable `UserInputSubmitted` → durable `TurnStarted` → ephemeral
  provider/Core output → durable terminal history。前端不等待execute返回才补入用户消息。
- Product 提交 context/surface intent，concrete Agent 保存实际接纳结果；adapter只从Agent native
  history反解ContextFrame，因此展示与真实执行输入一致。
- context、compaction、fork与interaction最终由concrete Agent receipt/inspection证明。
  Product只保存自己的lineage、association和workflow evidence。
- 命令并发使用Agent turn/interaction/fork cutoff/effect identity等typed coordinate，不使用
  generic projection revision。
- Agent unavailable不影响Product list/workspace/delete；conversation enrichment明确报告
  unavailable。

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Product association缺失 | typed not bound/unavailable |
| duplicate input | 返回原 Agent receipt |
| duplicate identity + different payload | typed conflict |
| active turn/interaction已变化 | typed stale coordinate；refresh snapshot |
| live gap/lag | 清除partial lane并重新read |
| Agent read失败 | Product shell保持可用；conversation unavailable |
| compaction/fork回包未知 | 同 effect identity inspect |

## 5. Good / Base / Bad Cases

- Good：输入同步进入Agent；UI先收到已提交的用户输入与turn开始，再观察live delta，终态后
  snapshot保存同一完整history。
- Base：连接中断时partial文本丢失，重连snapshot仍恢复Agent已提交内容。
- Bad：平台保存journal/projection再与Agent history比较；两份conversation没有独立业务意义。

## 6. Tests Required

- input identity/replay/conflict/unavailable测试。
- snapshot mapper覆盖所有message/item/terminal/interaction/compaction类型。
- waiting items组合测试证明Product gate与Agent history分层。
- durable input/turn顺序、live delta、gap、disconnect、snapshot recovery测试。
- surface/initial context写入Agent native history并投影ContextFrame的测试。
- create/fork/compaction response-lost inspection测试。
- list/workspace在Agent unavailable时仍返回Product shell。

## 7. Wrong vs Correct

```rust
// Wrong: 从平台journal恢复Agent conversation。
let history = runtime_journal.load(thread_id).await?;

// Correct: 从concrete Agent owner读取。
let history = complete_agent.read(AgentReadQuery {
    source: binding.agent.source,
    at_revision: None,
}).await?;
```
