# ChannelService 文档型通信主干设计

## 核心判断

Channel 是 AgentDash 的一等通信领域，不是队列，也不是 `LifecycleRun` 的附属类型。它的一等性落在领域语言、`ChannelService`、广播/投递规划和 Agent capability 投影上。

```text
Channel = 通信空间 + 参与者/绑定 + 广播路由 + 消息/投递规划 + 回复/发布能力面
ChannelRegistryDocument = 某个 owner 下的 Channel 事实文档
ChannelService = owner-scoped lazy registry resolver + ingress normalize + delivery intent planning + capability projection
Mailbox = AgentRun 消费输入的 durable scheduler
LifecycleGate = wait/result authority
```

Channel 不默认拆成 `channels`、`channel_participants`、`channel_bindings` 等关系表。Agent / Lifecycle runtime 事实高频随 SubAgent、runtime relation 和 LifecycleRun 创建释放，适合跟随 owner aggregate 的业务文档生灭。Project 公共 Channel 是未来 Assets 系统需要承接的项目级资产，本任务只定义 owner store port 和 Channel 领域合同，不抢先决定它的物理承载。

## 领域边界

| Owner | 职责 |
| --- | --- |
| Channel | 通信空间、参与者、绑定、广播策略、消息/投递规划语言、reply address、publish intent |
| ChannelRegistryDocument | LifecycleRun 或未来 Project/Asset owner store 下的 channel facts 文档 |
| ChannelService | create/update/close channel、participants、bindings、ingress normalize、broadcast plan、materialization intent |
| AgentRun Mailbox | per-AgentRun input consumption、queue/steer/launch scheduling、retry/recovery、user attention |
| LifecycleGate | wait/adoption/request result authority |
| PermissionGrant / Platform broker | platform/system decision facts、grant lifecycle、审计 |
| RuntimeSession / Terminal | execution trace、terminal state、stdout/stderr cursor 和 output refs |
| Capability | 某个 AgentFrame 当前能看到和操作哪些 Channel 的投影 |

`CapabilityState.channel` 不是 Channel membership 事实源。Channel participants / policy 是 owner registry 文档事实；capability 只是这些事实在 AgentFrame 上的可见操作面。

## 持久化边界

一期采用 owner-local document registry：

```text
LifecycleRun.channel_registry
  -> runtime-scoped Channel
  -> Companion/SubAgent runtime participants
  -> active reply address / delivery planning refs

ChannelOwnerStore(Project/Asset-backed, future)
  -> persistent Project Channel
  -> external IM binding
  -> ProjectAgent channel assignment
```

Lifecycle 侧新增 `lifecycle_runs.channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL` 或等价业务语义列，列名表达业务含义，不使用 `_json` / `_jsonb` 后缀。Repository 侧映射为 typed `ChannelRegistryDocument`，避免把结构化文档降级成字符串协议。Project 侧只定义 `ChannelOwnerStore` / DTO 合同；后续由 Project Assets 系统决定 channel asset 的实际承载。

Channel registry 是单一 owner 文档，不是多个关系表的替代名。`ChannelService` 必须通过 owner repository 读取、校验、更新 registry，避免 API、Mailbox、Companion、Terminal 分别写自己的 channel 片段。

`ChannelService` 不在服务启动时全局扫描 Project、LifecycleRun 或未来 Assets 来预加载 Channel。所有 registry 都按 owner lazy load：

- AgentFrame capability projection 只加载当前 AgentRun / LifecycleRun 和显式 assignment 涉及的 owner。
- IM ingress 只通过 provider event 的 binding key 解析对应 Project/asset owner。
- Companion/SubAgent 只加载当前 request/reply 所在 LifecycleRun owner。
- Mailbox/Gate materialization 只处理当前 delivery intent 涉及的 owner。

这样 ChannelService 是通信领域入口，不是全局常驻 channel runtime。

## Superseded Decisions

本节记录本轮修订明确推翻的旧结论，避免后续实现继续沿用。

| 旧结论 | 新结论 | 推翻原因 |
| --- | --- | --- |
| 第一版优先 Companion / SubAgent lifecycle-scoped temporary channel | 先建立通用 Channel / ChannelService 主干，再用 Companion/SubAgent 验证 | 企业 IM / Project 公共 Channel 是明确需求，lifecycle-only 模型会阻塞 Project 资产和外部绑定 |
| Channel 的家是共享 `LifecycleRun`，以 `LifecycleRun.channels` 保存 | LifecycleRun 是 runtime owner document；Project 通过 owner store/Assets 系统承载；Channel 领域仍通用 | Project channel 必须脱离具体 run，runtime channel 又应随 run 生灭 |
| Channel 一等性需要独立 `channels` / participants 表 | 一等性落在领域/service/capability；持久化采用 owner-local document | 运行时通信事实高频、短命、强 owner 绑定，拆表会增加清理和一致性成本 |
| 不存 participants，由 `CapabilityState.channel.visible_channels` 表达 | Channel registry 持有 participants / membership / broadcast policy；Capability 只做可见操作投影 | capability 是 AgentFrame surface，不应反向成为通信空间 membership authority |
| `ChannelMessage` / `ChannelDelivery` 等到 Project/IM 后再定义 | 现在定义合同边界，落库可分阶段 | 没有 Message/Delivery 边界无法解释广播、fan-out、IM ingress/outbox 和 mailbox materialization |

## 核心模型

Channel 领域归属 `agentdash-domain::channel`。Lifecycle runtime channel 是 `ChannelOwner::LifecycleRun` owner registry 中的一条 channel record，不是一等 `LifecycleChannel` 类型。

```rust
pub struct ChannelRegistryDocument {
    pub schema_version: u32,
    pub channels: Vec<ChannelRecord>,
}

pub struct ChannelRecord {
    pub channel: Channel,
    pub participants: Vec<ChannelParticipant>,
    pub bindings: Vec<ChannelBinding>,
    pub reply_addresses: Vec<ChannelReplyAddress>,
    pub delivery_state: Vec<ChannelDeliveryState>,
}

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

Participant 是 Channel registry 文档事实，不是 capability projection：

```rust
pub struct ChannelParticipant {
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
    pub provider: String,
    pub external_workspace_ref: String,
    pub external_room_ref: Option<String>,
    pub external_thread_ref: Option<String>,
    pub identity_mapping_policy: ChannelIdentityMappingPolicy,
    pub status: ChannelBindingStatus,
}
```

Message 与 Delivery 是主干合同。完整 event log 不在一期落库；一期只要求 typed envelope、delivery intent 和 materialized refs 清楚。

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

pub struct ChannelDeliveryIntent {
    pub id: ChannelDeliveryId,
    pub message: ChannelMessage,
    pub target: ChannelDeliveryTarget,
    pub policy: ChannelDeliveryPolicy,
}

pub struct ChannelDeliveryState {
    pub delivery_id: ChannelDeliveryId,
    pub message_id: ChannelMessageId,
    pub target: ChannelDeliveryTarget,
    pub status: ChannelDeliveryStatus,
    pub materialized_ref: Option<MaterializedDeliveryRef>,
}

pub enum MaterializedDeliveryRef {
    MailboxMessage { message_id: Uuid },
    LifecycleGate { gate_id: Uuid },
    Notification { notification_ref: String },
    PublishOutbox { outbox_id: Uuid },
}
```

`ChannelDeliveryState` 只保存 owner registry 需要恢复和去重的 bounded 状态。大 payload、AgentRun 调度状态和 gate result payload 仍由各自 owner 保存。

## ChannelService

`ChannelService` 是 application 层维护 Channel registry 与广播投递规划的主入口。它不直接取代 Mailbox、Gate、PermissionGrant 或 Terminal owner。

核心职责：

- `load_registry(owner) / save_registry(owner, registry)`，其中 LifecycleRun owner 直接走 run document，Project owner 走 `ChannelOwnerStore` port；所有 load 都由具体 owner ref 触发，不做全局 eager scan
- `create_channel / close_channel / update_policy`
- `add_participant / remove_participant / update_participant_policy`
- `bind_external_room / unbind_external_room`
- `ingest_external_event`
- `publish_message`
- `plan_broadcast_deliveries`
- `materialize_delivery_to_mailbox / materialize_delivery_to_gate / materialize_publish_outbox`
- `project_agent_channel_capability`

`ChannelService` 输出的是 delivery intent / materialization command；Mailbox scheduler 继续拥有 AgentRun input 的排队、claim、launch/steer、恢复和状态投影。

## ChannelAddress

现有 `MailboxSourceIdentity` 的字段形态应提升为 `ChannelAddress` / source attribution 基础，而不是把所有 Mailbox source 写成 `namespace=channel`。

```text
namespace=core       kind=composer | draft_start | canvas_action
namespace=companion  kind=dispatch | result | parent_request | parent_response
namespace=terminal   kind=state_changed | completed | failed
namespace=platform   kind=permission_grant_response
namespace=im.slack   kind=room_message | thread_reply
```

`ChannelAddress` 只表达来源、correlation、route、display label 与 adapter metadata。它不能替代 `Channel`、`ChannelBinding`、`ChannelMessage` 或 `ChannelDeliveryIntent`。

## Channel Capability

Channel capability 从 v1 起作为一等 `CapabilityState.channel` dimension 是正确方向，但语义需要重解释：

```text
ChannelRegistryDocument / Participant / Binding / Policy facts
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
    Expose {
        channel_ref: ChannelRef,
        aliases: Vec<String>,
        operations: BTreeSet<ChannelOperation>,
    },
    Revoke { channel_ref: ChannelRef },
}
```

`ToolCapability` 只控制 `channel_send`、`channel_reply`、`channel_broadcast` 等工具是否可见。具体能否访问某个 channel、某个 operation、某个外部 room，由 AgentRun effective channel capability / admission 判定。

## Representative Flows

### Project IM Channel

```text
Admin binds Feishu/Slack/Teams room
  -> ChannelService calls ChannelOwnerStore for Project-owned channel asset
  -> ProjectAgent assignment grants receive/reply/publish operations
  -> ChannelCapabilityProjector exposes visible channel refs to AgentFrame
  -> IM adapter ingests provider event
  -> ChannelService normalizes identity and derives ChannelMessage
  -> delivery policy selects target AgentRun / notification / digest
  -> AgentRun delivery materializes to Mailbox
  -> outbound reply creates ChannelPublishOutbox intent with audit/rate-limit/approval policy
```

### Lifecycle Runtime Channel

```text
Companion target=sub or workflow creates runtime relation
  -> ChannelService updates LifecycleRun.channel_registry
  -> participants include parent/child AgentRun refs
  -> ChannelCapabilityProjector exposes aliases/operations to each active AgentFrame
  -> first message or reply is ChannelMessage + ChannelDeliveryIntent
  -> delivery materializes to child mailbox, parent mailbox, gate, or notification
  -> channel registry disappears with LifecycleRun
```

`LifecycleRun` owns the runtime registry document, but `LifecycleChannel` does not exist as a first-class model.

### Companion Facade

```text
companion_request
  -> Channel facade
  -> target resolver: parent / child / human / platform
  -> optional provision side effect for subagent
  -> optional LifecycleGate for wait/result authority
  -> ChannelDeliveryIntent materialized to mailbox / notification / broker

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

## Implementation Scope

一期可派发任务只做：

1. Domain model + document registry。
2. LifecycleRun owner document 字段与 repository roundtrip。
3. Project/IM `ChannelOwnerStore` contract，不决定 Project Assets 物理承载。
4. ChannelService skeleton 和 service tests。
5. Channel capability dimension skeleton。
6. Mailbox/Gate materialization intent contract tests。

完整 IM provider、完整 Channel event log、旧路径迁移和既有 Gate/Mailbox 表清理拆为后续任务。

## Recommended First Principle

Channel owns communication semantics and broadcast planning. Owner documents own Channel facts. Mailbox owns AgentRun consumption. LifecycleGate owns wait/result state. Capability owns model-visible operation projection.
