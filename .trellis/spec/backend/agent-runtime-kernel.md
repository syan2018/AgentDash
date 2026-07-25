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
) -> Result<ManagedRuntimeSnapshot, AgentSnapshotProjectionError>;
```

```rust
pub struct AgentLiveEvent {
    pub source: AgentSourceCoordinate,
    pub sequence: AgentServiceU64,
    pub record: CanonicalConversationRecord,
}
```

```rust
pub struct AgentRunResolvedCompleteAgent {
    pub service: Arc<dyn CompleteAgentService>,
    pub binding_generation: AgentBindingGeneration,
}

pub trait AgentRunCompleteAgentResolverPort {
    async fn resolve(
        &self,
        binding: &AgentRunProductRuntimeBinding,
    ) -> Result<AgentRunResolvedCompleteAgent, String>;
}
```

```ts
interface ManagedRuntimeFeedConnection {
  readonly ready: Promise<void>;
  reload(): Promise<void>;
  close(): void;
}
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
- normalize 必须保留 concrete Agent 的 turn/item coordinate，并把
  `conversation_history: Vec<CanonicalConversationRecord>` 原样交给 Product/UI。Runtime snapshot
  不再维护 `turns/items/active_turn_id` 平行字段；需要 active/completed/item view 时使用
  `CanonicalConversationView` 即时推导。
- Product binding 是冷启动解析 Complete Agent 的最小完整输入。resolver 必须先用 binding 中的
  immutable execution profile 与 AgentFrame 重建当前 Host route，再原子返回 service 与
  binding generation；按裸 `service_instance_id` 直接查询进程内 catalog 无法恢复重启后的绑定。
- 同步 Product command 响应透传 concrete Agent operation receipt 的真实状态。前端收到响应后
  主动 `reload()` authoritative snapshot；live delta 继续负责执行中的低延迟展示，不承担终态
  提交证明。
- authoritative history 中没有 assistant item 的 terminal turn 仍是完整轮次。前端保留该
  segment，并展示 `turn.error.message`；错误终态不因“没有可渲染文本”而被过滤。
- `changes` 只有 concrete Agent 真正提供 ordered durable change tail 时才映射该 tail。
  Snapshot-only Agent 通过重复 `read` 恢复，不由 Runtime 伪造 durable cursor。
- live event 是 connection/process-local `CanonicalConversationRecord`。Runtime 只按 source-local
  sequence broadcast；gap、Lagged、断连后丢弃 partial lane并重新读取 Agent snapshot。
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
| Host 重启且 binding 指向尚未 materialize 的 Dash service | 从完整 binding 重建 route 后读取同一 source |
| client command id 为空或 payload 无效 | side effect 前 invalid request |
| 同 identity 不同 payload | concrete Agent typed idempotency conflict |
| inspect = Applied/Accepted | 返回既有 receipt；不重复 side effect |
| inspect = NotApplied | 执行同一 effect identity |
| inspect = Unknown | typed pending/unavailable；不重派 |
| live subscriber lagged | 断流或 typed retryable unavailable；重新 read |
| live consumer 收到旧 provider telemetry shape | typed stream parse failure；不静默丢弃或本地补造 item |
| Agent snapshot 无法 normalize | typed protocol error；不保存“修复后”副本 |
| availability 在请求间变化 | 下一请求重新 resolve；不使用 generic revision gate |
| command receipt 为 failed/interrupted/lost | API 返回真实状态；UI 重读 authoritative snapshot 并展示 terminal/error |
| terminal turn 没有 assistant item | 保留 terminal-only segment，不显示无限等待 |

## 5. Good / Base / Bad Cases

- Good：Composer input 直接进入 Agent，返回 Agent receipt；同 client identity 重试命中同一
  effect。
- Good：进程重启后首次 snapshot/live/command 都以持久 Product binding 恢复 Host route，读取
  concrete Agent 已保存的同一 source history。
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
- canonical view tests 覆盖 active/completed turn 与 completed item 均从唯一 history 推导。
- composition test 覆盖 Product input → Agent execute → live delta → Agent history →
  reconnect read。
- cold-start composition test 先清空 Host/catalog 进程态，再以既有 Product binding 读取
  snapshot，断言同一 service/source 被重新 materialize 且 generation 来自新 Host route。
- frontend feed test 覆盖同步 command 后 authoritative reload；turn segmentation 与静态渲染测试
  覆盖无 assistant item 的 failed terminal 及其错误文本。
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

```rust
// Wrong: cold Host 中 catalog 必然为空，后续恢复逻辑永远没有机会运行。
let service = live_catalog.current(&binding.agent.service_instance_id).await?;
let generation = ensure_product_binding_route(&binding).await?;

// Correct: 完整 binding 先恢复 route，再一起返回 service 与 generation。
let resolved = complete_agent_resolver.resolve(&binding).await?;
resolved.service.read(AgentReadQuery {
    source: binding.agent.source,
    at_revision: None,
}).await?;
```

```ts
// Wrong: command HTTP 完成后继续等待某个不保证存在的 terminal live event。
await submitComposerInput(request);

// Correct: live 展示 partial，command 完成后以 authoritative read 收束终态。
await submitComposerInput(request);
await runtimeFeed.reload();
```
