# Agent Runtime 持久化权威

## 1. Scope / Trigger

本规范适用于 AgentRun、AgentFrame、Complete Agent source、Agent Runtime、Complete Agent
Host、Agent input handoff、Tool/Hook callback 与 presentation effect 的持久化设计。新增表、
JSONB 字段、repository、revision、receipt、outbox 或 recovery worker 时必须复核。

持久化的目的，是让某个领域 owner 在进程全部丢失后仍能继续履行业务承诺。可从 owner 的
document、`CompleteAgentService::read/changes/inspect` 或稳定输入 identity 恢复的中间协调状态
没有独立业务寿命，因此不建立数据库 owner。

## 2. Signatures

Product owner document 的最终数据库边界：

```text
lifecycle_agents(
  id text,
  run_id text,
  project_id text,
  ...Product lifecycle fields...,
  frames jsonb not null,
  runtime_binding jsonb null
)
```

`frames` 保存该 LifecycleAgent 的不可变 AgentFrame history。`runtime_binding` 保存 Product
定位 concrete Agent 所需的稳定关联：

```rust
pub struct AgentRunProductRuntimeBinding {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub agent: AgentRunCompleteAgentAssociation,
    pub launch_frame: ProductAgentFrameRef,
    pub execution_profile: ProductExecutionProfileRef,
    pub execution_profile_digest: String,
}
```

名称中的 `Runtime` 表示 Product 的执行关联，不表示 Runtime 持久化聚合。repository 只读写
`lifecycle_agents.runtime_binding`：

```rust
pub trait AgentRunProductRuntimeBindingRepository {
    async fn load_product_binding(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunProductRuntimeBinding>, String>;
}
```

concrete Agent 的权威边界：

```rust
pub trait CompleteAgentService {
    async fn read(&self, query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError>;
    async fn changes(&self, query: AgentChangesQuery)
        -> Result<AgentChangePage, AgentServiceError>;
    async fn inspect(
        &self,
        identity: AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, AgentServiceError>;
}
```

Dash 使用一个 source document 保存 repository history 与 source metadata；Create 前尚无 source
coordinate 的 effect 可以按 `effect_id` 保存 Agent-owned receipt。Runtime、Host 和 callback
没有 repository signature 或 revision table。

## 3. Contracts

- Product 持久化 LifecycleRun/LifecycleAgent、owner-local AgentFrame history、execution profile
  intent、workflow/lineage，以及 AgentRun 到 concrete Agent service/source 的稳定关联。
- `lifecycle_agents.frames` 与 `runtime_binding` 都是 LifecycleAgent 局部事实。scalar column
  只用于 owner lookup、唯一约束和索引；repository 从 JSONB document 解码，不从拆分列重建
  第二份对象。
- concrete Complete Agent 独占 native source history、context、fork lineage、compaction state、
  command/effect receipt、applied surface evidence 与 source change cursor。
- Dash source 使用单个 canonical JSONB document。source document 与 branch/history/command/
  effect/change 关系镜像不能同时作为写入目标。
- Create 前需要按 effect identity 查询的 receipt 属于 concrete Agent，不属于 Product、
  Runtime 或 Host。
- Agent Runtime 只在内存中完成 command mapping、timeout/cancel、snapshot normalize 与 live
  broadcast。Runtime process restart 通过 Product association 与 Agent `read/inspect` 重建。
- Complete Agent Host 只在内存中保存 attachment、target、binding、generation、callback route
  与 availability。Host process restart 重新 materialize、attach、apply surface 和 bind。
- Product input 在当前请求内同步 handoff。Product 不保存 pending input、claim、mailbox、
  background retry 或 handoff receipt ledger；真正接收输入的 Agent 保存 effect receipt。
- LifecycleGate、Routine、Channel 或 Workflow 可以保存自己的未决业务状态和下游 handoff
  coordinate，因为它们拥有独立 Product 生命周期。其 JSONB receipt 只能引用
  `handoff_id/operation_id`，不能复制 Agent command/history。
- Workspace/Terminal 等 Product presentation store 只有在表达独立 Product effect 时持久化。
  写入方向固定为 Agent observation → Product effect；这些 store 不回写 Agent execution，
  不参与 command、list、workspace 或 delete 的正确性 gate。
- owner document revision 只用于该 owner 内部 CAS。Agent snapshot/change revision 只用于
  Agent read/cursor。跨 Product、Runtime、Host 比较 generic revision、digest 或 surface
  snapshot 不构成合法并发命题。

一份新状态只有同时满足以下判据才允许持久化：

1. 进程全部丢失后，该事实仍必须成立；
2. 不能从唯一 owner 的 durable intent、`read`、`changes` 或 `inspect` 恢复；
3. 丢失会破坏业务承诺、造成不可接受的重复外部副作用或失去安全 fencing；
4. 有唯一 writer、独立生命周期和明确清理边界；
5. 它不是缓存、JOIN、availability、diagnostic 或一致性复验副本。

满足判据后仍优先写入归属聚合的 JSONB document。只有真正跨 owner 查询、独立 claim/retention
或独立生命周期需要时才建立全局表。

## 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| LifecycleAgent 不存在 | binding/frame 写入失败；不创建悬空局部事实 |
| 同一 LifecycleAgent 已有不同 association | typed conflict；不双写 |
| `runtime_binding` target/frame/profile digest 非法 | repository decode/commit 失败 |
| Agent source 不可用 | Product shell 仍可读；Agent presentation 返回 typed unavailable |
| Runtime/Host process restart | 内存状态为空；从 Product association + Agent source 重建 |
| callback route 不属于当前 Host incarnation | typed unknown/stale route；不查询数据库恢复旧 route |
| 相同 Agent effect identity 重试 | Agent `inspect/execute` 返回原 receipt 或 typed conflict |
| presentation cache/store 缺失 | 重新从 Product/Agent owner 读取；不阻断业务命令 |
| schema readiness 发现 Runtime/Host/Callback revision table | readiness 失败并列出残留 schema |
| owner-local handoff receipt 缺少下游 identity | owner 自己保持 pending/failed；不得推断 Agent 已接收 |

## 5. Good / Base / Bad Cases

- Good：LifecycleAgent 一行保存 frames 与 concrete Agent association；删除 owner 时局部事实自然
  消失，Dash history 仍由 Dash source 生命周期处理。
- Good：Host 重启后依据 association 重新 attach 和 apply surface；Agent `inspect(effect_id)`
  返回已应用结果，平台不需要 Host effect ledger。
- Base：Agent 暂时不可用，列表返回 Product shell 和 unavailable presentation；恢复后下一次
  read 直接组合 Agent snapshot。
- Bad：把 Agent operation、surface、source revision 再写进 Runtime/Product 表，然后在 List
  时比较 currentness。该比较只检测副本漂移，不能证明 Agent 事实。
- Bad：为同步 input handoff 建 pending row/outbox/receipt，再由 worker 投递。这样 Product
  在 Agent 接收前制造了离线可靠投递承诺。

## 6. Tests Required

- PostgreSQL migration 测试断言 `lifecycle_agents.frames/runtime_binding` 存在，
  `agent_frames`、Product binding 全局表、Runtime/Host/Callback revision 表及 mailbox/command
  ledger 不存在。
- LifecycleAgent repository 测试覆盖 frame exact/latest/history、binding commit/replay/conflict、
  runtime-thread unique lookup 与 owner deletion。
- Dash repository 测试覆盖 source document restart、fork/compaction/history read、effect
  inspection，以及不存在关系镜像双写。
- Product list/workspace/delete 测试在 Agent resolve/read 失败时仍返回 Product shell。
- Host restart 测试使用全新 Host 实例，从 Product association 与 Agent receipt 重建 route。
- input handoff 测试断言成功一定包含 concrete operation receipt；Agent unavailable 直接返回
  typed error，数据库中不产生 pending delivery。
- migration history guard、schema readiness、负向源码搜索与 `git diff --check` 必须通过。

## 7. Wrong vs Correct

```rust
// Wrong: 持久化可从 concrete Agent 重建的 Runtime 副本。
runtime_repository.commit(operation, projection, source_snapshot).await?;

// Correct: Product 保存定位关联；读取时即时投影 Agent owner facts。
let binding = bindings.load_product_binding(&target).await?;
let snapshot = complete_agent.read(AgentReadQuery {
    source: binding.agent.source,
    at_revision: None,
}).await?;
```

```rust
// Wrong: Host callback route 跨重启进入通用 receipt ledger。
callback_repository.reserve(route, invocation).await?;

// Correct: Host 只 fence 当前 route，真实 handler 按 idempotency key 拥有 receipt。
let route = host.resolve_callback_route(&invocation.meta).await?;
handler.invoke(invocation.meta.idempotency_key, invocation.payload).await
```
