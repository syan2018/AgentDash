# ChannelService 完整通信主干设计

## 核心判断

Channel 是 AgentDash 的一等通信领域，不是队列，也不是 `LifecycleRun` 的附属类型。它的一等性落在领域语言、`ChannelService`、广播/投递规划和 Agent capability 投影上。

```text
Channel = 通信空间 + 参与者/绑定 + 广播路由 + 消息/投递规划 + 回复/发布能力面
ChannelRegistryDocument = 某个 owner 下的 Channel 事实文档
ChannelService = owner-scoped lazy registry resolver + ingress normalize + delivery intent planning + materialization + capability projection
Mailbox = AgentRun 消费输入的 durable scheduler
LifecycleGate = wait/result authority
OwnerDocumentMutation = owner document 的原子读改写策略
```

Channel 不默认拆成 `channels`、`channel_participants`、`channel_bindings` 等关系表。Agent / Lifecycle runtime 事实高频随 SubAgent、runtime relation 和 LifecycleRun 创建释放，适合跟随 owner aggregate 的业务文档生灭。Project 公共 Channel 是未来 Assets 系统需要承接的项目级资产，本任务定义 owner store / binding resolver / provider-neutral envelope，不抢先决定物理承载。

## 领域边界

| Owner | 职责 |
| --- | --- |
| Channel | 通信空间、参与者、绑定、广播策略、消息/投递规划语言、reply address、publish intent |
| ChannelRegistryDocument | LifecycleRun 或未来 Project/Asset owner store 下的 channel facts 文档 |
| ChannelService | create/update/close channel、participants、bindings、ingress normalize、broadcast plan、materialization intent |
| OwnerDocumentMutation | owner document row-lock、typed decode、domain mutation、目标列写回 |
| AgentRun Mailbox | per-AgentRun input consumption、queue/steer/launch scheduling、retry/recovery、user attention |
| LifecycleGate | wait/adoption/request result authority |
| PermissionGrant / Platform broker | platform/system decision facts、grant lifecycle、审计 |
| RuntimeSession / Terminal | execution trace、terminal state、stdout/stderr cursor 和 output refs |
| Capability | 某个 AgentFrame 当前能看到和操作哪些 Channel 的投影 |

`CapabilityState.channel` 不是 Channel membership 事实源。Channel participants / policy 是 owner registry 文档事实；capability 只是这些事实在 AgentFrame 上的可见操作面。

## 持久化与 Mutation 边界

采用 owner-local document registry：

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

Lifecycle 侧新增 `lifecycle_runs.channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL` 或等价业务语义列，列名表达业务含义，不使用 `_json` / `_jsonb` 后缀。Repository 侧映射为 typed `ChannelRegistryDocument`，避免把结构化文档降级成字符串协议。

Channel registry 是单一 owner 文档，不是多个关系表的替代名。`ChannelService` 必须通过 owner store 的原子 mutation port 更新 registry，避免 API、Mailbox、Companion、Terminal 分别写自己的 channel 片段。

### 通用 owner document mutation

新增 owner document mutation contract，供 Channel 和后续 owner-local document 复用：

```text
mutate_owner_document(owner_ref, document_key, mutation)
  -> begin transaction
  -> SELECT target_document FROM owner_table WHERE owner_id = ? FOR UPDATE
  -> decode target_document as typed value object
  -> apply domain mutation
  -> validate document invariants
  -> UPDATE owner_table SET target_document = Json(document), updated_at = now()
  -> commit
```

约束：

- application/domain 只暴露语义 port，例如 `ChannelOwnerStore::mutate_registry(owner, mutation)`；不得暴露任意 table/column 字符串。
- infrastructure helper 可以复用 typed JSONB read/write、row lock、error mapping，但每个 repository 必须显式绑定允许 mutation 的 owner/table/column。
- broad aggregate update 不写独立 owner document column。`LifecycleRunRepository::update` 继续更新 run 的 orchestration/task/status/execution_log 等 run aggregate 字段，但不得写 `channel_registry`。
- 需要同时变更多个 owner document 或跨聚合事实时，使用 application use case 编排多个 explicit ports；不要把跨聚合事务伪装成单一 repository update。
- document 内需要高频查询、排序、claim/lease、跨 owner scan 或数据库唯一约束时，按数据库规范提升为 scalar / independent store candidate。

## Lazy Loading 与 Binding Lookup

`ChannelService` 不在服务启动时全局扫描 Project、LifecycleRun 或未来 Assets 来预加载 Channel。所有 registry 都按 owner lazy load：

- AgentFrame capability projection 只加载当前 AgentRun / LifecycleRun 和显式 assignment 涉及的 owner。
- IM ingress 只通过 provider event 的 binding key 调用 `ChannelBindingResolver` 解析 owner；resolver 不得通过全局 Project/LifecycleRun scan 猜测归属。
- Companion/SubAgent 只加载当前 request/reply 所在 LifecycleRun owner。
- Mailbox/Gate materialization 只处理当前 delivery intent 涉及的 owner。

未实现具体 IM adapter 时，provider-neutral binding lookup 只返回明确 unresolved / unsupported 结果。这样 ChannelService 是通信领域入口，不是全局常驻 channel runtime。

## 核心模型

Channel 领域归属 `agentdash-domain::channel`。Lifecycle runtime channel 是 `ChannelOwner::LifecycleRun` owner registry 中的一条 channel record，不是一等 `LifecycleChannel` 类型。

```rust
pub struct ChannelRegistryDocument {
    pub schema_version: u32,
    pub channels: Vec<ChannelRecord>,
}

pub enum ChannelRegistryMutation {
    UpsertChannel(ChannelRecord),
    CloseChannel { channel_id: ChannelId, reason: Option<String> },
    AddParticipant { channel_id: ChannelId, participant: ChannelParticipant },
    RemoveParticipant { channel_id: ChannelId, participant_ref: ChannelParticipantRef },
    UpdateParticipantPolicy { channel_id: ChannelId, participant_ref: ChannelParticipantRef, operations: BTreeSet<ChannelOperation>, ingress_policy: ChannelIngressPolicy, egress_policy: ChannelEgressPolicy },
    UpsertBinding { channel_id: ChannelId, binding: ChannelBinding },
    RemoveBinding { channel_id: ChannelId, binding_ref: ChannelBindingRef },
    RecordDeliveryState { channel_id: ChannelId, state: ChannelDeliveryState },
    PruneDeliveryState { channel_id: ChannelId, before: DateTime<Utc>, max_items: usize },
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
```

`ChannelOwner` 表达持久化 owner / 权限根，不把外部 room/thread 自身当作 owner：

```rust
pub enum ChannelOwner {
    Project { project_id: Uuid },
    Story { story_id: Uuid },
    LifecycleRun { run_id: Uuid },
    System,
}
```

外部 IM workspace / room / thread 只通过 `ChannelBinding` 表达：

```rust
pub struct ChannelBinding {
    pub binding_id: ChannelBindingId,
    pub provider: String,
    pub external_workspace_ref: String,
    pub external_room_ref: Option<String>,
    pub external_thread_ref: Option<String>,
    pub identity_mapping_policy: ChannelIdentityMappingPolicy,
    pub status: ChannelBindingStatus,
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
```

Message 与 Delivery 是主干合同。完整 event log 不在本任务落库；本任务要求 typed envelope、delivery intent 和 materialized refs 清楚。

```rust
pub struct ChannelMessage {
    pub id: ChannelMessageId,
    pub channel_id: ChannelId,
    pub sender: ChannelParticipantRef,
    pub audience: ChannelAudience,
    pub thread_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub address: ChannelAddress,
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
    pub updated_at: DateTime<Utc>,
}
```

`ChannelDeliveryState` 只保存 registry 恢复和去重需要的 bounded 状态。实现必须提供 prune 规则，例如 per-channel 最大 item 数与按时间裁剪；大 payload、AgentRun 调度状态、gate result payload、terminal output 与 platform broker state 仍由各自 owner 保存。

## ChannelOwnerStore

```text
ChannelOwnerStore
  load_registry(owner) -> ChannelRegistryDocument
  mutate_registry(owner, ChannelRegistryMutation) -> ChannelRegistryDocument

ChannelBindingResolver
  resolve_binding(provider_event_key) -> ResolvedChannelBinding | Unresolved | Unsupported
```

LifecycleRun owner store 直接使用 `lifecycle_runs.channel_registry` 的 owner document mutation。Project owner store 只表达 contract，不决定物理 asset store。Binding resolver 只消费明确 provider binding key；未实现 provider adapter 时不扫描 Project / LifecycleRun。

## ChannelService

`ChannelService` 是 application 层维护 Channel registry 与广播投递规划的主入口。它不直接取代 Mailbox、Gate、PermissionGrant 或 Terminal owner。

核心职责：

- `create_channel / close_channel / update_policy`
- `add_participant / remove_participant / update_participant_policy`
- `bind_external_room / unbind_external_room`
- `ingest_external_event`
- `publish_message`
- `plan_broadcast_deliveries`
- `materialize_delivery_to_mailbox / materialize_delivery_to_gate / materialize_publish_outbox`
- `project_agent_channel_capability`

`ChannelService` 输出 delivery intent / materialization command；Mailbox scheduler 继续拥有 AgentRun input 的排队、claim、launch/steer、恢复和状态投影。

## ChannelAddress

现有 `MailboxSourceIdentity` 的字段形态应提升为 `ChannelAddress` / source attribution 基础，而不是把所有 Mailbox source 写成 `namespace=channel`。

```text
namespace=core       kind=composer | draft_start | canvas_action
namespace=companion  kind=dispatch | result | parent_request | parent_response
namespace=terminal   kind=state_changed | completed | failed
namespace=platform   kind=permission_grant_response
namespace=im.slack   kind=room_message | thread_reply
```

`ChannelAddress` 只表达来源、correlation、route、display label 与 adapter metadata。它不能替代 `Channel`、`ChannelBinding`、`ChannelMessage` 或 `ChannelDeliveryIntent`。Mailbox mapper 从 `ChannelAddress` 生成 `MailboxSourceIdentity` 时保留 `mailbox.source.{namespace}.{kind}` 这类 mailbox 展示 key；通用 ChannelAddress 不内置 mailbox 前缀。

## Channel Capability

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

新增 `CapabilityState.channel` 使用 default 空态，避免旧 frame JSON 反序列化失败。Tool capability 只控制 `channel_send`、`channel_reply`、`channel_broadcast` 等工具是否可见；具体能否访问某个 channel、某个 operation、某个外部 room，由 AgentRun effective channel capability / admission 判定。

## Representative Flows

### Lifecycle Runtime Channel

```text
Companion target=sub or workflow creates runtime relation
  -> ChannelService mutates LifecycleRun.channel_registry
  -> participants include parent/child AgentRun refs
  -> ChannelCapabilityProjector exposes aliases/operations to each active AgentFrame
  -> first message or reply is ChannelMessage + ChannelDeliveryIntent
  -> delivery materializes to child mailbox, parent mailbox, gate, or notification
  -> channel registry disappears with LifecycleRun
```

`LifecycleRun` owns the runtime registry document, but `LifecycleChannel` does not exist as a first-class model.

### Companion / SubAgent / Human

```text
companion_request
  -> ChannelService target resolver: parent / child / human / platform
  -> optional provision side effect for subagent
  -> optional LifecycleGate for wait/result authority
  -> ChannelDeliveryIntent materialized to mailbox / notification / broker

companion_respond
  -> ChannelService reply address resolver
  -> ChannelMessage(kind=response)
  -> resolve gate / mailbox continuation / pending action through materializer
```

Companion 工具保留是因为模型使用体验更好，但实现必须调用 ChannelService。Agent-facing prompt/tool 只暴露业务 payload、短 alias 和 operation intent；内部 channel/message/delivery/gate/runtime refs 由 resolver 持有。

### Terminal / Async Producer

```text
Terminal owner keeps state/output/cursor
  -> terminal completed / failed / needs attention
  -> ChannelMessage(kind=terminal_state_changed, refs=terminal_id/output_ref)
  -> delivery policy decides whether to wake Agent
  -> mailbox message contains bounded preview + refs
```

Terminal output owner 不迁到 Channel；Channel 只承载异步消息入栈结构和 wake delivery planning。

### Project IM Channel Contract

```text
Admin binds provider room in a future Project Asset task
  -> ChannelOwnerStore persists Project-owned channel facts
  -> ProjectAgent assignment grants receive/reply/publish operations
  -> ChannelBindingResolver resolves provider event to owner + channel
  -> ChannelService normalizes identity and derives ChannelMessage
  -> delivery policy selects target AgentRun / notification / digest
  -> AgentRun delivery materializes to Mailbox
```

本任务不实现 provider adapter 或 Project Asset 物理承载；上述 contract 必须足够让后续任务接入。

## Tool Surface

长期工具面分为通用 Channel 工具和语义工具入口：

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

本任务保持现有语义工具的模型体验，但所有工具入口的实现边界必须走 ChannelService，避免企业 IM 和 Project Channel 后续再迁一次 transport。

## Recommended First Principle

Channel owns communication semantics and broadcast planning. Owner documents own Channel facts. Owner document mutation owns atomic typed document updates. Mailbox owns AgentRun consumption. LifecycleGate owns wait/result state. Capability owns model-visible operation projection.
