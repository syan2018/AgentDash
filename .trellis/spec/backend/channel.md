# Channel Architecture

## Role

Channel 是有状态通信空间，拥有稳定身份、参与者、策略、binding、消息归属与 bounded delivery recovery state。它用于人、Agent、外部参与者和平台服务之间的消息、关注、唤醒与 handoff。

版本化 request/response provider contract 使用 `ExtensionProtocol`，一次受控调用使用 `Operation`。这些对象只通过 actor identity、capability、trace/correlation 和 provider adapter 与 Channel 关联，原因是调用事务与通信关系具有不同的生命周期和恢复边界。

## Identity And Ownership

```rust
pub struct ChannelRef {
    pub owner: ChannelOwner,
    pub channel_id: Uuid,
}

pub struct ChannelLocator {
    pub owner: ChannelOwner,
    pub channel_key: ChannelKey,
}

pub enum ChannelOwner {
    Project { project_id: Uuid },
    LifecycleRun { run_id: Uuid },
}
```

- `ChannelId` 是全局 authority identity。
- `ChannelKey` 是 owner-local unique 的稳定业务 key；`ChannelLocator` 支持原子 `create_if_absent`。
- aliases 服务展示与搜索，不参与 authority resolution。
- `ChannelOwner` 同时定义授权根、生命周期和 store routing。Project 与 LifecycleRun 是当前有真实产品与 storage 边界的 owners。
- external room/thread 由 `ChannelBinding` 表达；message source 由 `ChannelMessageOrigin` 表达，因此二者不扩展 owner enum。

## Orthogonal Dimensions

- participant cardinality 从 active membership 派生。
- delivery audience 表达 direct/broadcast 目标选择。
- thread relation 保存在 message relation 或 external binding 中。
- `ChannelLifetimePolicy` 表达 owner-bound 或 explicit-close。
- `ChannelRetentionPolicy` 表达 message/delivery metadata 的窗口和上限。
- transport/endpoint 只由 `ChannelBinding` 表达。

每个字段只承载一个可执行不变量，原因是 admission、persistence 和 provider dispatch 必须能够独立演进和测试。

## Service Admission

所有 publish、reply、broadcast 与 provider ingress 都进入 `ChannelService`。每次调用重新验证：

1. `ChannelRef` 的 owner、record owner 与 store routing 一致。
2. Channel 是 open 状态。
3. sender 是 active participant。
4. sender policy 包含当前 operation。
5. ingress/egress policy 接受当前来源与目标。
6. audience 都是 active、可接收的 participants。
7. external delivery 使用 active binding 和 ready provider。

`CapabilityState.channel` 是 registry 与 participant policy 派生出的 actor-specific projection。执行入口仍以上述 service admission 为 authority，原因是 AgentFrame surface 可能在调用前发生 membership、status 或 binding 变化。

## Persistence

LifecycleRun runtime Channel 保存在 `lifecycle_runs.channel_registry` typed owner document 中，通过 `ChannelOwnerStore::mutate_registry` 进行 row-lock、typed decode、domain mutation 与目标列写回。Project Channel 的物理承载归 Project Assets，并实现同一 owner store contract。

独立 store 只服务跨 owner query、claim/lease、独立 retention/audit、不可重建 reverse index 或数据库唯一约束。external binding inbound resolution 使用明确的 reverse index，不扫描 owner documents。

## Binding Provider

```rust
#[async_trait]
pub trait ChannelBindingProvider {
    async fn normalize_inbound(&self, event: ProviderInboundEvent)
        -> Result<NormalizedChannelIngress, ChannelBindingError>;
    async fn publish(&self, request: ChannelOutboundRequest)
        -> Result<ChannelProviderReceipt, ChannelBindingError>;
}
```

provider adapter 负责外部事件规范化和物理发送；`ChannelService` 负责 binding resolution、membership/policy admission、canonical message 与 delivery state。Extension package 可以同时贡献 `OperationProvider` 和 `ChannelBindingProvider`，两类 contribution 保持独立注册。

## Delivery Boundaries

- AgentRun Mailbox 拥有 durable intake、claim、launch/steer 和恢复。
- LifecycleGate 拥有 wait/result lifecycle。
- provider outbox/receipt 拥有物理 delivery 尝试。
- Channel 保存 canonical communication refs 与 bounded delivery recovery state，不复制上述 payload authority。
- Interaction/Operation 只以 typed content/correlation refs 进入 Channel message。

由通信关系触发的 runtime wake 必须解析真实 `ChannelLocator/ChannelRef` 后创建消息；source effect id 只用于 dedup/correlation。Terminal hook auto-resume 属于 AgentRun control-plane completion wake，直接使用 `MailboxSourceIdentity::hook_auto_resume`，原因是没有真实通信参与者时不应制造 Channel identity。这样 Channel message 始终来自 registry membership 与 service admission，而普通控制面 wake 保留在 Mailbox 边界。

## Required Tests

- Domain：owner/ref/locator consistency、key uniqueness、membership cardinality、lifetime/retention validation。
- Application：publish/reply/broadcast/ingress admission matrix。
- Persistence：owner document roundtrip、concurrent mutation、create-if-absent 与 broad update preservation。
- Binding：reverse-index resolution、normalize/publish、duplicate event 与 unavailable provider。
- Integration：真实 registry wake、mailbox/gate materialization、capability projection 失效后重新 admission。
