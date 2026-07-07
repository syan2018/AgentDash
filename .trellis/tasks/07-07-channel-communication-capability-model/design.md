# Channel 通信能力长期模型预评估设计

## 核心判断

Channel 是 AgentDash 的一等通信领域，不是队列，也不是 `LifecycleRun` 的附属字段。

```text
Channel = 通信空间 + 参与者/绑定 + 广播路由 + 消息/投递规划 + 回复/发布能力面
ChannelService = Channel 事实维护、ingress、broadcast planning、materialization intent owner
Mailbox = AgentRun 消费输入的 durable scheduler
LifecycleGate = wait/result authority
```

Channel 位于 Mailbox 之前。它将内部/外部 producer 产生的通信事件归一化成可广播、可绑定、可审计、可回复的结构；当某个 delivery 需要目标 AgentRun 消费时，再 materialize 成 AgentRun Mailbox message。

企业内部 IM / Project 公共 Channel 是明确需求，不是远期占位。因此第一性模型必须直接支持 Project-level persistent channel asset、External IM binding 和 Agent capability assignment。Companion / SubAgent 是第一条迁移和验证链路，但不能把 Channel 缩成 lifecycle-only temporary channel。

## 领域边界

| Owner | 职责 |
| --- | --- |
| Channel | 通信空间、参与者、绑定、广播策略、消息事实、delivery plan、reply address、publish intent |
| ChannelService | create/update/close channel、participants、bindings、ingress normalize、broadcast plan、materialization intent |
| AgentRun Mailbox | per-AgentRun input consumption、queue/steer/launch scheduling、retry/recovery、user attention |
| LifecycleGate | wait/adoption/request result authority |
| PermissionGrant / Platform broker | platform/system decision facts、grant lifecycle、审计 |
| RuntimeSession / Terminal | execution trace、terminal state、stdout/stderr cursor 和 output refs |
| Capability | 某个 AgentFrame 当前能看到和操作哪些 Channel 的投影 |

`CapabilityState.channel` 不是 Channel membership 事实源。Channel participants / policy 是 Channel 事实；capability 只是这些事实在 AgentFrame 上的可见操作面。

## Superseded Decisions

本节记录本轮修订明确推翻的旧结论，避免后续实现继续沿用。

| 旧结论 | 新结论 | 推翻原因 |
| --- | --- | --- |
| 第一版优先 Companion / SubAgent lifecycle-scoped temporary channel | 先建立通用 Channel / ChannelService 主干，再用 Companion/SubAgent 验证 | 企业 IM / Project 公共 Channel 是明确需求，lifecycle-only 模型会阻塞 Project 资产和外部绑定 |
| Channel 的家是共享 `LifecycleRun`，以 `LifecycleRun.channels` 保存 | `LifecycleRun` 只是 runtime owner/scope 的一种；Channel 是通用领域实体 | Project / Story / ExternalBinding Channel 必须脱离具体 run 仍可解析 |
| 不存 participants，由 `CapabilityState.channel.visible_channels` 表达 | Channel 持有 participants / membership / broadcast policy；Capability 只做可见操作投影 | capability 是 AgentFrame surface，不应反向成为通信空间 membership authority |
| `ChannelOwner` 收窄为 `LifecycleRun` | 保留 Project / Story / LifecycleRun / ExternalBinding / System 等 owner/scope | Project 公共 IM channel 和未来 Story 协作 channel 都需要通用 owner |
| `ChannelMessage` / `ChannelDelivery` 等到 Project/IM 后再定义 | 现在定义合同边界，落库可分阶段 | 没有 Message/Delivery 边界无法解释广播、fan-out、IM ingress/outbox 和 mailbox materialization |

## 核心模型

Channel 领域归属 `agentdash-domain::channel`。Lifecycle runtime channel 是它的一种 scoped instance，不是一等 `LifecycleChannel` 类型。

```rust
pub struct Channel {
    pub id: ChannelId,
    pub owner: ChannelOwner,
    pub medium: ChannelMedium,
    pub topology: ChannelTopology,
    pub lifecycle: ChannelLifecycle,
    pub status: ChannelStatus,
    pub policy: ChannelPolicy,
}

pub enum ChannelOwner {
    Project { project_id: Uuid },
    Story { story_id: Uuid },
    LifecycleRun { run_id: Uuid },
    ExternalBinding { provider: String, external_ref: String },
    System,
}

pub enum ChannelLifecycle {
    Persistent,
    RuntimeScoped,
    Ephemeral,
}

pub enum ChannelMedium {
    Internal,
    Companion,
    Human,
    Terminal,
    Platform,
    ExternalIm { provider: String },
}

pub enum ChannelTopology {
    Room,
    Direct,
    Thread,
    Broadcast,
    RequestReply,
    EventStream,
}
```

Participant 是 Channel 事实，不是 capability projection：

```rust
pub struct ChannelParticipant {
    pub channel_id: ChannelId,
    pub participant_ref: ChannelParticipantRef,
    pub role: ChannelRole,
    pub operations: BTreeSet<ChannelOperation>,
    pub ingress_policy: ChannelIngressPolicy,
    pub egress_policy: ChannelEgressPolicy,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
}

pub enum ChannelParticipantRef {
    AgentRun { run_id: Uuid, agent_id: Uuid },
    ProjectAgent { project_id: Uuid, agent_id: Uuid },
    Human { user_ref: String },
    ExternalUser { provider: String, external_user_ref: String },
    System { key: String },
}
```

Binding 表达外部入口与 Project/Story/Lifecycle channel 的连接：

```rust
pub struct ChannelBinding {
    pub channel_id: ChannelId,
    pub provider: String,
    pub external_workspace_ref: String,
    pub external_room_ref: Option<String>,
    pub external_thread_ref: Option<String>,
    pub identity_mapping_policy: ChannelIdentityMappingPolicy,
    pub status: ChannelBindingStatus,
}
```

Message 与 Delivery 是主干合同。是否第一阶段完整落库由实现任务决定，但边界必须现在固定：

```rust
pub struct ChannelMessage {
    pub id: ChannelMessageId,
    pub channel_id: ChannelId,
    pub sender: ChannelParticipantRef,
    pub audience: ChannelAudience,
    pub thread_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub payload: ChannelPayload,
    pub content_refs: Vec<ChannelContentRef>,
    pub provider_event_ref: Option<String>,
}

pub struct ChannelDelivery {
    pub id: ChannelDeliveryId,
    pub message_id: ChannelMessageId,
    pub target: ChannelDeliveryTarget,
    pub status: ChannelDeliveryStatus,
    pub policy: ChannelDeliveryPolicy,
    pub materialized_ref: Option<MaterializedDeliveryRef>,
}

pub enum MaterializedDeliveryRef {
    MailboxMessage { message_id: Uuid },
    LifecycleGate { gate_id: Uuid },
    Notification { notification_ref: String },
    PublishOutbox { outbox_id: Uuid },
}
```

## ChannelService

`ChannelService` 是 application 层维护 Channel 事实与广播投递规划的主入口。它不直接取代 Mailbox、Gate、PermissionGrant 或 Terminal owner。

核心职责：

- `create_channel / close_channel / update_policy`
- `add_participant / remove_participant / update_participant_policy`
- `bind_external_room / unbind_external_room`
- `ingest_external_event`
- `publish_message`
- `plan_broadcast_deliveries`
- `materialize_delivery_to_mailbox / materialize_delivery_to_gate / materialize_publish_outbox`
- `project_agent_channel_capability`

ChannelService 输出的是 delivery intent / materialization command；Mailbox scheduler 继续拥有 AgentRun input 的排队、claim、launch/steer、恢复和状态投影。

## ChannelAddress

现有 `MailboxSourceIdentity` 的字段形态应提升为 `ChannelAddress` / source attribution 基础，而不是把所有 Mailbox source 写成 `namespace=channel`。

```text
namespace=core       kind=composer | draft_start | canvas_action
namespace=companion  kind=dispatch | result | parent_request | parent_response
namespace=terminal   kind=state_changed | completed | failed
namespace=platform   kind=permission_grant_response
namespace=im.slack   kind=room_message | thread_reply
```

`ChannelAddress` 只表达来源、correlation、route、display label 与 adapter metadata。它不能替代 `Channel`、`ChannelBinding`、`ChannelMessage` 或 `ChannelDelivery`。

实现方向仍是整体重定位：把 `MailboxSourceIdentity` 的通用字段搬到 `agentdash-domain::channel::ChannelAddress`，再让 mailbox 侧消费该值对象。`display_label_key` 里硬编码的 `"mailbox.source."` 前缀需要参数化或下沉到 mailbox projection。

## Channel Capability

Channel capability 从 v1 起作为一等 `CapabilityState.channel` dimension 是正确方向，但语义需要重解释：

```text
Channel / Participant / Binding / Policy facts
  -> ChannelCapabilityProjector
  -> RuntimeCapabilityEffect / declaration
  -> AgentFrame CapabilityState.channel.visible_channels
```

`CapabilityState.channel` 只表达某个 AgentFrame 当前可见、可操作哪些 Channel；它不是 Channel participant 列表，也不是 broadcast policy authority。

```rust
pub struct ChannelDimension {
    pub visible_channels: Vec<ChannelCapabilityRef>,
}

pub struct ChannelCapabilityRef {
    pub channel_ref: ChannelRef,
    pub aliases: Vec<String>,
    pub operations: BTreeSet<ChannelOperation>,
    pub ingress_policy: ChannelIngressPolicy,
    pub egress_policy: ChannelEgressPolicy,
    pub readiness: ChannelReadiness,
}

pub enum ChannelDirective {
    Expose { channel_ref: ChannelRef, aliases: Vec<String>, operations: BTreeSet<ChannelOperation> },
    Revoke { channel_ref: ChannelRef },
}
```

`ToolCapability` 只控制 `channel_send`、`channel_reply`、`channel_broadcast` 等工具是否可见。具体能否访问某个 channel、某个 operation、某个外部 room，由 AgentRun effective channel capability / admission 判定。

## Representative Flows

### Project IM Channel

```text
Admin binds Feishu/Slack/Teams room
  -> ChannelService creates Project-owned Channel + ChannelBinding
  -> ProjectAgent assignment grants receive/reply/publish operations
  -> ChannelCapabilityProjector exposes visible channel refs to AgentFrame
  -> IM adapter ingests provider event
  -> ChannelService normalizes identity and writes/derives ChannelMessage
  -> delivery policy selects target AgentRun / notification / digest
  -> AgentRun delivery materializes to Mailbox
  -> outbound reply creates ChannelPublishOutbox with audit/rate-limit/approval policy
```

### Lifecycle Runtime Channel

```text
Companion target=sub or workflow creates runtime relation
  -> ChannelService creates LifecycleRun-owned runtime-scoped Channel
  -> participants include parent/child AgentRun refs
  -> ChannelCapabilityProjector exposes aliases/operations to each active AgentFrame
  -> first message or reply is ChannelMessage/Delivery
  -> delivery materializes to child mailbox, parent mailbox, gate, or notification
```

`LifecycleRun` 可以引用或 own 这些 runtime-scoped channel，但 `LifecycleChannel` 不作为一等类型存在。

### Companion Facade

```text
companion_request
  -> Channel facade
  -> target resolver: parent / child / human / platform
  -> optional provision side effect for subagent
  -> optional LifecycleGate for wait/result authority
  -> ChannelDelivery materialized to mailbox / notification / broker

companion_respond
  -> Channel reply facade
  -> active ReplyAddress resolver
  -> ChannelMessage(kind=response)
  -> resolve gate / mailbox continuation / pending action
```

Companion 工具保留是因为模型使用体验更好，但实现应调用 Channel application service。Agent-facing prompt/tool 只暴露业务 payload、短 alias 和 operation intent；内部 channel/message/delivery/gate/runtime refs 由 resolver 持有。

### Terminal / Async Producer

```text
Terminal owner keeps state/output/cursor
  -> terminal completed / failed / needs attention
  -> ChannelMessage(kind=terminal_state_changed, refs=terminal_id/output_ref)
  -> delivery policy decides whether to wake Agent
  -> mailbox message contains bounded preview + refs
```

Terminal output owner 不迁到 Channel；Channel 只承载异步消息入栈结构和 wake delivery planning。

## Tool Surface

长期工具面分为通用 Channel 工具和语义 facade：

```text
channel_list
channel_describe
channel_send
channel_reply
channel_broadcast
```

```text
companion_request
companion_respond
ask_human
request_permission
```

第一阶段可以继续 facade-first，不急于把 `channel_*` 全量暴露给模型。但实现边界应已经走 ChannelService，避免后续企业 IM 和 Project Channel 再迁一次 transport。

## Still Open

- Project Channel 的声明式 assignment 应挂在 ProjectAgent preset、Project channel assignment，还是两者组合。
- Channel 持久化第一阶段采用独立表还是 Project asset + runtime registry 混合承载。
- 外部 IM event log 是否首期落库，还是先实现 bounded ingress + delivery materialization。
- PermissionGrant 如何驱动 Channel capability 可见性；现有 grant classifier 仍主要围绕 Tool 维度。

## Recommended First Principle

Channel owns communication semantics and broadcast planning. Mailbox owns AgentRun consumption. LifecycleGate owns wait/result state. Capability owns model-visible operation projection.
