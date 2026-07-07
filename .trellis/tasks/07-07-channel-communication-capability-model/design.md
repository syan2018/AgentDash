# Channel 通信能力长期模型预评估设计

## 核心判断

Channel 是 AgentDash 的通信能力面，不是队列。

```text
Channel = 通信空间 + 广播路由 + 入栈标准化 + 回复/发布能力面
Mailbox = AgentRun 消费输入的 durable scheduler
```

Channel 位于 Mailbox 之前。它将内部/外部 producer 产生的通信事件归一化成可广播、可绑定、可审计、可回复的结构；当某个 delivery 需要目标 AgentRun 消费时，再 materialize 成 AgentRun Mailbox message。

第一版实现范围应窄：优先以 Companion / SubAgent 的 lifecycle-scoped temporary channel 验证 Channel 作为能力面的形态，而不是立即建设 Project asset channel、外部 IM provider 或完整 ChannelMessage log。

## 领域边界

| Owner | 职责 |
| --- | --- |
| Channel | 通信空间、参与者、消息、广播、绑定、delivery plan、reply address、publish intent |
| AgentRun Mailbox | per-AgentRun input consumption、queue/steer/launch scheduling、retry/recovery、user attention |
| LifecycleGate | wait/adoption/request result authority |
| PermissionGrant / Platform broker | platform/system decision facts |
| RuntimeSession / Terminal | execution trace、terminal state、stdout/stderr cursor 和 output refs |
| Capability | 某个 AgentRun 能看到和操作哪些 Channel / Channel operation |

这个分层让 Channel 可以统一“入栈前结构”，但不会抢走已有事实源。

## 候选核心模型

长期 Channel 可以成为 Project / Story / AgentTeam / external binding 等 owner 下的持久资产；第一版 companion channel 则只需要支持 LifecycleRun owner 下的 runtime-scoped temporary channel。它仍归属 `agentdash-domain::channel`，但不要求第一版暴露全局管理面。

```rust
pub struct Channel {
    pub id: ChannelId,
    pub owner: ChannelOwner,
    pub medium: ChannelMedium,
    pub topology: ChannelTopology,
    pub policy: ChannelPolicy,
}

pub enum ChannelOwner {
    Project { project_id: Uuid },
    Story { story_id: Uuid },
    AgentTeam { team_id: Uuid },
    Agent { agent_id: Uuid },
    AgentRun { run_id: Uuid, agent_id: Uuid },
    ExternalBinding { provider: String, external_ref: String },
    System,
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

长期 Channel message 与 delivery 分离；第一版 Companion tracer 暂不要求完整 message log，消息事实继续由 Gate / Mailbox / Terminal owner 持有：

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
}

pub struct ChannelDelivery {
    pub id: ChannelDeliveryId,
    pub message_id: ChannelMessageId,
    pub target: ChannelDeliveryTarget,
    pub status: ChannelDeliveryStatus,
    pub materialized_ref: Option<MaterializedDeliveryRef>,
}
```

一条 Project / Story / external room broadcast 可以是一条 `ChannelMessage`，多条 `ChannelDelivery`。Mailbox 只保存目标 AgentRun 需要消费的 bounded preview、refs、source attribution 和 scheduler 状态。

### ChannelAddress

现有 `MailboxSourceIdentity` 的字段形态应提升为 canonical `ChannelAddress` 基础，而不是把所有 Mailbox source 都改成 `namespace=channel`。所有进入 Mailbox 的来源都可以被视为 channel family：

```text
namespace=core       kind=composer | draft_start | canvas_action
namespace=companion  kind=dispatch | result | parent_request | parent_response
namespace=terminal   kind=state_changed | completed | failed
namespace=platform   kind=permission_grant_response
namespace=im.slack   kind=room_message | thread_reply
```

推荐后续提取 `ChannelAddress` domain value object，并让 `MailboxSourceIdentity` 使用同构字段或嵌入 / 投影它。这样 Channel 地址模型不会被 Mailbox 命名限制，同时能复用 `namespace / kind / source_ref / correlation_ref / actor / route / metadata` 的 attribution 能力。

## Channel Capability

Channel 应作为类似 Workspace Module 的通用能力面。长期形态应是一等 `CapabilityState.channel` dimension；第一版可通过 runtime effect 将 lifecycle channel ref 暴露到 parent / child 当前 AgentFrame，不需要先建设配置式 channel cap UI。

```rust
pub struct ChannelCapabilityState {
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

pub enum ChannelOperation {
    Receive,
    Reply,
    Send,
    Broadcast,
    PublishExternal,
    Subscribe,
    ManageParticipants,
}
```

`ToolCapability` 只控制 `channel_send`、`channel_reply` 等工具是否可见；具体能否使用某个 channel、某个 operation、某个外部 room，应由 AgentRun effective channel capability / admission 判定。

Companion `target=sub` 的第一版 runtime exposure 语义：

```text
create child AgentRun
  -> create/resolve lifecycle-scoped temporary private channel
  -> create parent-child relation
  -> write RuntimeCapabilityEffect(dimension=channel, effect_type=expose_channel_ref)
  -> parent/child current AgentFrame sees task-local channel alias/reply surface
```

为特定 Agent 支持公司 IM，本质是给它添加对应 Channel capability：

```text
channel = im.company.room:platform-team
operations = receive mention, reply thread, publish with approval
aliases = ["platform-team", "team-room"]
ingress_policy = mention | assigned_thread | digest
egress_policy = thread_reply_only | approval_required | rate_limited
```

## 通用入栈路径

```text
Producer
  -> Ingress Adapter
  -> Identity Normalize
  -> Channel Resolve
  -> ChannelMessage
  -> Binding / Audience Resolve
  -> DeliveryPolicy
  -> ChannelDelivery
  -> Materializer
```

Materializer 按目标类型执行：

```text
AgentRun      -> AgentRunMailboxMessage
AgentIdentity -> resolve/create/select AgentRun -> AgentRunMailboxMessage
Human         -> UI notification / human inbox / LifecycleGate
Platform      -> broker fact, e.g. PermissionGrant
External      -> channel publish outbox
Runtime       -> trace/projection refs, optional mailbox wake
```

## Representative Flows

### Agent Team / Project / Story Broadcast

```text
Project or Story channel receives message
  -> participants / role resolver
  -> delivery policy: reviewer / planner / executor / all
  -> ChannelDelivery per target
  -> mailbox materialization only for agents that must act
  -> notifications for observers
```

Channel owns broadcast semantics and shared context; Mailbox owns each AgentRun's consumption.

### External IM

```text
IM adapter receives provider event
  -> normalize provider workspace / room / thread / user / message
  -> persist ChannelMessage(channel = im room)
  -> resolve ChannelBinding to Project / Story / AgentTeam / Agent
  -> apply mention / keyword / digest / approval policy
  -> materialize delivery to mailbox, notification, gate, or digest
```

Agent outbound publish uses `ChannelPublishOutbox` with capability/admission, permission, audit and rate-limit checks.

### Companion

```text
companion_request
  -> channel request facade
  -> target resolver: parent / child / human / platform
  -> optional provision side effect for subagent
  -> optional LifecycleGate
  -> ChannelDelivery
  -> mailbox / notification / broker materialization

companion_respond
  -> channel reply facade
  -> active ReplyAddress resolver
  -> ChannelMessage(kind=response)
  -> resolve gate / mailbox continuation / pending action
```

`target=sub` 是特例，因为 target resolution 同时创建 child AgentRun、private channel / relation 和 first delivery。

### Terminal

```text
Terminal owner keeps state/output/cursor
  -> terminal completed / failed / needs attention
  -> ChannelMessage(kind=terminal_state_changed, refs=terminal_id/output_ref)
  -> delivery policy decides whether to wake Agent
  -> mailbox message contains bounded preview + refs
```

Terminal output owner 不迁到 Channel；Channel 只承载异步消息入栈结构。

## Tool Surface

长期可以分为通用 Channel 工具和语义 facade：

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

Companion 工具保留是因为模型使用体验更好，但实现应调用 Channel application service。Agent-facing prompt/tool 只暴露业务 payload、短 alias 和 operation intent；内部 channel/message/delivery/gate/runtime refs 由 resolver 持有。

第一版选择 facade-only：不暴露通用 `channel_*` 工具；Channel capability 也不默认以 roster 形式进入模型上下文。模型只在当前任务需要时看到短 alias / ReplyContract。

## Open Design Risks

- 第一版 `LifecycleChannel` 的最小实体 / 表命名仍需确定：通用 `channels` + owner scope，或先建更窄的 lifecycle-scoped storage。
- Runtime effect payload 需要明确是否使用 `dimension=channel / effect_type=expose_channel_ref`。
- Channel capability 如果直接进入 `CapabilityState`，需要明确 base / runtime modifier / permission grant 的积累规则。
- Channel message 持久化的第一版范围需要控制；内部 Companion / Terminal 可能不需要完整 event log，而外部 IM 需要。
- AgentTeam broadcast 需要角色模型和 shared context 策略，否则 Channel 会先成为事件存储而无法体现协作价值。
- 外部 IM publish 需要 permission、audit、rate limit 和 identity mapping，否则风险远高于普通 internal channel。
- Extension protocol channel 已经占用 `channel_key/method` 作为 RPC surface；Agent communication Channel 需要在命名和模块边界上与它区分。

## Recommended First Principle

Channel owns communication semantics. Mailbox owns AgentRun consumption.
