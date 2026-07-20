# In-Memory Agent Runtime Kernel

## 1. Scope / Trigger

本规范适用于 Product command 到 Complete Agent 的协调、Agent snapshot normalize、live event
broadcast、timeout/cancel 与 typed error mapping。修改 Runtime command、read adapter、stream
或重连策略时必须复核。

Runtime 的事务边界是一次进程内 handoff，不是 durable aggregate。跨重启事实由 Product 与
concrete Agent 两端拥有。

## 2. Signatures

```rust
pub trait CompleteAgentService {
    async fn execute(
        &self,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentServiceError>;

    async fn read(
        &self,
        query: AgentReadQuery,
    ) -> Result<AgentSnapshot, AgentServiceError>;

    async fn live_events(
        &self,
        source: AgentSourceCoordinate,
    ) -> Result<Box<dyn AgentLiveEventStream>, AgentServiceError>;

    async fn inspect(
        &self,
        identity: AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, AgentServiceError>;
}
```

```rust
pub fn project_authoritative_agent_snapshot(
    runtime_thread_id: RuntimeThreadId,
    snapshot: AgentSnapshot,
) -> Result<ManagedRuntimeSnapshot, ProjectionError>;
```

Runtime 可以保留平台中立的 `ManagedRuntimeSnapshot`、operation receipt 与 command DTO，原因是
它们是 API/adapter contract；它们不因此成为数据库事实。

## 3. Contracts

- command 输入由 Product target、association、client command identity 和 payload 构成。
  Runtime 解析当前 Host route，使用稳定 Agent effect identity 调用 concrete Agent。
- Submit/Steer 的真实选择由 concrete Agent 当前 active turn 决定；Product 不根据缓存状态建立
  pending branch。
- command success 必须来自 concrete Agent receipt。Agent unavailable、unsupported、conflict
  或 provider failure 以 typed error 返回当前请求。
- post-dispatch response unknown 使用同一 effect identity 调用 `inspect`。`Applied/Accepted`
  返回原 receipt；`NotApplied` 才能执行；`Unknown` 保持 typed pending/unavailable，不能自动
  换 identity 重派。
- `read` 每次从 Product association 定位 concrete Agent source，调用 Agent authoritative
  read，再在内存中 normalize 为 Product/UI 所需 snapshot。
- `changes` 只有 concrete Agent 真正提供 ordered durable change tail 时才映射该 tail。
  Snapshot-only Agent 通过重复 `read` 恢复，不由 Runtime 伪造 durable cursor。
- live event 是 connection/process-local partial presentation。Runtime 只 normalize 和
  broadcast；gap、Lagged、断连后丢弃 partial lane并重新读取 Agent snapshot。
- Runtime 不持久化 operation、projection、journal、change、outbox、source identity map、
  availability revision 或 surface snapshot。
- Runtime 不比较 Product revision 与 derived Runtime projection revision。并发 gate 使用真实
  typed coordinate，例如 effect identity、Agent turn、interaction、fork cutoff 或当前 Host route。
- Runtime failure 不写 Product terminal 副本来“修正”Agent history。真实 execution terminal
  由 concrete Agent source history恢复；Product-owned workflow/gate effect按自己的生命周期
  观察并保存结果。

## 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| target 没有 Product association | typed unavailable/not bound |
| association 指向不可用 service/source | typed unavailable；Product shell 不受影响 |
| client command id 为空或 payload 无效 | side effect 前 invalid request |
| 同 identity 不同 payload | concrete Agent typed idempotency conflict |
| inspect = Applied/Accepted | 返回既有 receipt；不重复 side effect |
| inspect = NotApplied | 执行同一 effect identity |
| inspect = Unknown | typed pending/unavailable；不重派 |
| live subscriber lagged | 断流或 typed retryable unavailable；重新 read |
| Agent snapshot 无法 normalize | typed protocol error；不保存“修复后”副本 |
| availability 在请求间变化 | 下一请求重新 resolve；不使用 generic revision gate |

## 5. Good / Base / Bad Cases

- Good：Composer input 直接进入 Agent，返回 Agent receipt；同 client identity 重试命中同一
  effect。
- Base：live delta 中断，UI 丢弃 partial lane，重新 read 后得到 Agent 已提交的完整 history。
- Bad：先把 command 写进 Runtime outbox，再由 worker dispatch；这把同步 handoff 扩张成第二
  workflow engine。
- Bad：List 比较 persisted projection revision 与 binding revision；派生缓存过期不应改变
  Product read 的合法性。

## 6. Tests Required

- command tests 覆盖 stable identity、same-payload replay、different-payload conflict、
  unavailable 与 typed rejection。
- inspection tests 覆盖 Applied/Accepted/NotApplied/Unknown 和 dispatch response lost。
- read mapper tests 用 Agent snapshot 断言 history、terminal、interaction、compaction 与真实
  diagnostic 不丢失。
- live tests 覆盖 callback → stream、source-local order、Lagged、disconnect 和 read recovery。
- composition test 覆盖 Product input → Agent execute → live delta → Agent history →
  reconnect read。
- 负向源码搜索断言 Runtime repository、journal/outbox persistence、change worker 与
  projection revision gate 不在 production composition。

## 7. Wrong vs Correct

```rust
// Wrong
let accepted = runtime_repository.accept(command).await?;
runtime_outbox.enqueue(accepted).await?;

// Correct
let effect = stable_agent_effect(&target, &client_command_id, &payload)?;
dispatch_or_inspect_same_effect(complete_agent, effect, command).await
```

```rust
// Wrong
if cached_projection.revision != binding.expected_revision {
    return Err(ProjectionDrift);
}

// Correct
let snapshot = complete_agent.read(AgentReadQuery {
    source: binding.agent.source,
    at_revision: None,
}).await?;
project_authoritative_agent_snapshot(binding.runtime_thread_id, snapshot)
```
