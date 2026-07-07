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

长期 Channel 可以成为 Project / Story / external binding 等 owner 下的持久资产；v1 的 Companion / SubAgent channel 是运行时概念，挂在 parent/child 共享的同一个 `LifecycleRun` 下。它仍归属 `agentdash-domain::channel`，但不要求第一版暴露全局管理面。

**已决策（2026-07-07 五轮，最终版本，替换此前所有草稿）**：

- **Companion `target=sub` 不创建新的 `run_id`**：它是在同一个 `LifecycleRun`（`lifecycle_runs.id`）下新增一个 `lifecycle_agents` 行（新 `agent_id`），parent/child 本来就共享一个 run。`lifecycle_runs` 本身就是"单 Agent / 多 Agent 共存"的容器（`lifecycle_agents.run_id` 早已是一对多），`lifecycle_gates`/`lifecycle_workflow_instances` 都是同样挂在 `run_id` 下的子表。`AgentRunLineage`（`parent_run_id <> child_run_id`）是另一个场景（Fork），不是 Companion sub 用的路径。
- **Channel 的家是这个共享的 `LifecycleRun`，作为结构化字段挂在 run 上，不新建表、不挂在某一侧的 AgentFrame**：对齐 `orchestrations`/`execution_log` 的既有模式（`ALTER TABLE lifecycle_runs ADD COLUMN orchestrations text DEFAULT '[]'::text NOT NULL`，`migrations/0003_lifecycle_orchestration_contract.sql:3`）——`LifecycleRun` 领域结构体新增一个字段，序列化后作为 `lifecycle_runs` 表的新列，用同一次 `ALTER TABLE lifecycle_runs ADD COLUMN channels text DEFAULT '[]'::text NOT NULL` 迁移：
  ```rust
  pub struct LifecycleRun {
      // ...既有字段
      #[serde(default, skip_serializing_if = "Vec::is_empty")]
      pub channels: Vec<LifecycleChannel>,
  }

  pub struct LifecycleChannel {
      pub id: ChannelId,
      pub medium: ChannelMedium,   // v1 只有 Companion
      pub topology: ChannelTopology, // v1 只有 Direct 或 Thread
      pub status: ChannelStatus,
      pub created_at: DateTime<Utc>,
      pub closed_at: Option<DateTime<Utc>>,
  }
  ```
  不需要 `run_id` 字段——它已经是 `LifecycleRun.channels` 的元素，天然被这个 run 限定。由实现阶段的一个 Channel Service 在 run 的领域对象上做 create/close（append / 标记 closed_at），跟 `orchestrations` 现在的读写方式一致，不是独立事件日志。
- **参与者不用单独的字段或表**：parent 和 child 是同一个 `run_id` 下的两个 `lifecycle_agents` 行；"谁在这个 channel 里"由各自的 `CapabilityState.channel.visible_channels` 是否持有指向这个 `LifecycleChannel.id` 的引用来表达（引用，不是复制状态），不需要额外的 participants 字段或 join 表。
- **`ChannelOwner` 收窄**：`AgentRun { run_id, agent_id }` 和 `AgentTeam { team_id }` 两个 variant 合并成 `LifecycleRun { run_id }`——owner 是整个 run（可以有 1..N 个 agent），不是单个 agent；AgentTeam 不是独立实体，只是"一个 run 下有多个 agent 协作"的另一种说法。
- **判断原则**：Channel 什么时候才需要脱离 `LifecycleRun`、单独建独立资产表？只有当它的身份必须独立于任何单个 `LifecycleRun` 才能解析时才需要（例如"哪个 Project 绑定了哪个 Slack room"这种查询）。v1 的 Companion channel 完全锚定在一次具体的 parent/child 共享 run 上，不需要那种独立资产表。

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
    LifecycleRun { run_id: Uuid },
    Agent { agent_id: Uuid },
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

一条 Project / Story / external room broadcast 可以是一条 `ChannelMessage`，多条 `ChannelDelivery`。Mailbox 只保存目标 AgentRun 需要消费的 bounded preview、refs、source attribution 和 scheduler 状态。这两个结构在 v1 不实例化、不落库，只在 Channel 扩展到 Project/Story/外部 IM 等需要独立消息事实的场景才启用。

`LifecycleChannel.medium`/`topology`/`status` 不需要在应用层之外再加额外的封闭校验层——这是吸取 `MailboxMessageSource`（历史 closed check constraint，已在 migration `0032` 修复）和 `agent_run_lineages.relation_kind`（现在仍有 `CHECK (relation_kind = 'fork')`，是尚未清理的同类遗留）两次教训后的选择：新增取值只需要扩展 Rust enum，不要在数据库层用 CHECK 约束把取值也锁死一遍。

### ChannelAddress

现有 `MailboxSourceIdentity` 的字段形态应提升为 canonical `ChannelAddress` 基础，而不是把所有 Mailbox source 都改成 `namespace=channel`。所有进入 Mailbox 的来源都可以被视为 channel family：

```text
namespace=core       kind=composer | draft_start | canvas_action
namespace=companion  kind=dispatch | result | parent_request | parent_response
namespace=terminal   kind=state_changed | completed | failed
namespace=platform   kind=permission_grant_response
namespace=im.slack   kind=room_message | thread_reply
```

（代码核实：`core`/`companion`/`workflow`/`routine` 已有真实构造点；`platform`/`im.slack` 目前只是 spec/design 里的前瞻占位，代码中还没有任何路径真正构造这两个 namespace 的 `MailboxSourceIdentity`——`target=platform` 现状是 missing-broker 诊断，不是真实投递。引用这个清单做可实施性论证时不要假设 platform/外部 IM 已有先例。）

**已决策（2026-07-07 三轮，四轮确认不留别名）**：整体重定位 + 直接迁移全部调用点，不保留 `MailboxSourceIdentity` 别名或 re-export。把结构体搬到共享位置（`agentdash-domain::channel::ChannelAddress`），`agent_run_mailbox` 及其所有调用点（工厂方法调用、`crates/agentdash-application-agentrun/src/agent_run/mailbox.rs`、`project_agent_start.rs`、`companion/tools.rs`、API routes/contracts/TS 生成等，完整影响面参照 `.trellis/tasks/06-28-integration-channel-mailbox-convergence/research/W0-source-identity-impact.md` 当年 `MailboxSourceIdentity` 自己的迁移记录）直接改成引用新路径，不留兼容别名——保留 re-export 只会增加后续维护心智负担，这本身是一次跑不掉的强制迁移，不如一次改完。`dedup_fragment()`（`namespace:kind` 拼接）完全通用，直接搬过去。唯一需要处理的是 `new()` 里硬编码的 `display_label_key: format!("mailbox.source.{namespace}.{kind}")`（`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:68`），改成参数化前缀。**实现前置检查**：确认 `display_label_key` 目前只用于展示，没有被前端拿去做字符串匹配（`packages/app-web` 侧的消费点需要在实现阶段查一次）。

## Channel Capability

**已决策（2026-07-07 二轮对齐）**：Channel capability 从 v1 起就作为一等 `CapabilityState.channel` dimension 落地，不走 Workspace Module 式的 AgentFrame 平行列 + 独立 resolver 路线。理由：

- 代码核实发现 Workspace Module 并不是干净的"projection-only 先例"——它的声明式部分挂在 `CapabilityState.workspace_module` 但从未注册进 `CapabilityDimensionRegistry`（没有 effect/replay），运行时曝光部分（canvas attach 可见性）完全走独立的 `AgentFrame.visible_workspace_module_refs_json` 列，彻底绕开 `RuntimeCapabilityEffectRecord`；两者只在读取时由一个专用 resolver 函数 OR 合并。这不是一个值得复刻的目标形态，只是历史遗留的权宜实现。
- `AccumulationPolicy::Accumulate` 已有完整先例：VFS dimension 的 `apply_mount_operations` effect 用 `MountDirective::{AddMount, RemoveMount, ReplaceMount, AddLink, RemoveLink}` 实现"运行时累积挂载、可撤销"（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability.rs:822-838`）；`AccumulationPolicy::Accumulate` 的文档注释本身就把"canvas mount append"列为该策略的典型场景（`crates/agentdash-spi/src/session_persistence.rs:148-149`）。Channel 直接复刻这个模式，不需要新增第 4 种 AccumulationPolicy。
- 后续扩展到 Project / Story / 多 Agent LifecycleRun / 外部 IM 的全局 channel 时，一定需要一个独立的 `CapabilityState` 承载可见性，现在直接建对的形态，避免以后从 AgentFrame 侧信道迁移回 dimension 的技术债。

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

pub enum ChannelOperation {
    Receive,
    Reply,
    Send,
    Broadcast,
    PublishExternal,
    Subscribe,
    ManageParticipants,
}

/// 对齐 `MountDirective` 的 Add/Remove 形态；`replay_effect` 按 channel_ref id 在
/// `visible_channels` 上做 upsert / retain-remove，与 `apply_mount_directives` 同构。
pub enum ChannelDirective {
    Expose { channel_ref: ChannelRef, aliases: Vec<String>, operations: BTreeSet<ChannelOperation> },
    Revoke { channel_ref: ChannelRef },
}
```

`ToolCapability` 只控制 `channel_send`、`channel_reply` 等工具是否可见；具体能否使用某个 channel、某个 operation、某个外部 room，应由 AgentRun effective channel capability / admission 判定。

v1 的 dimension module 落地形态（对齐 `VfsCapabilityDimensionModule` 的写法）：

- `CAPABILITY_DIMENSION_CHANNEL = "channel"`，`policy() -> AccumulationPolicy::Accumulate`。
- `validate_declaration`：v1 不支持 declaration（对齐 `CompanionCapabilityDimensionModule` 的拒绝式实现），预设/配置式 channel 授予留给后续阶段。
- `effect_type = apply_channel_operations`，payload 是 `Vec<ChannelDirective>`；`replay_effect` 按 `channel_ref` id 做 upsert（Expose）或 retain-remove（Revoke），与 `apply_mount_directives` 同构。
- 是否参与 `intersect()`（`connector/mod.rs:638-668`）：对齐 `companion`/`vfs` 的现状，self 值直传，不做集合裁剪。
- `CapabilityState` 新增必填字段的历史先例（`workspace_module` 字段引入时不给 `#[serde(default)]`，强制显式迁移已持久化 AgentFrame JSON，对应测试 `capability_state_json_requires_workspace_module_dimension`）：Channel 字段引入时要重走一次同样的决策，这是实现阶段的具体任务，不是架构层面的未决问题。
- 通过 `PermissionGrant` 累积 channel 可见性目前没有通用机制可复用——`AgentRunGrantProjection::classify_path`/`partition_paths`（`effective_capability.rs:59-85`）硬编码基于 `ToolCapabilityPath`，只服务 Tool 维度。如果 Channel 需要"由 grant 驱动可见性"，这部分仍是需要新设计的开放问题（与 accumulation 机制本身无关，是 grant 集成的问题）。

Companion `target=sub` 的第一版 runtime exposure 语义：

```text
create new lifecycle_agents row under the SAME run_id as parent (no new LifecycleRun)
  -> append LifecycleChannel to that LifecycleRun.channels (Channel Service, create/close semantics)
  -> write RuntimeCapabilityEffect(dimension=channel, effect_type=apply_channel_operations,
       payload=[ChannelDirective::Expose{channel_ref, aliases, operations}]) to BOTH parent's and child's AgentFrame
  -> each side's replay_effect upserts a REFERENCE (ChannelCapabilityRef) into its own
       CapabilityState.channel.visible_channels — not a copy of the channel's full state
  -> parent/child AgentRun effective capability view 暴露 task-local channel alias/reply surface
  -> channel 结束时：LifecycleRun.channels 里对应条目标记 closed_at，双方 write
       ChannelDirective::Revoke{channel_ref}，从各自 visible_channels 移除
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

### Multi-Agent LifecycleRun / Project / Story Broadcast

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
  -> resolve ChannelBinding to Project / Story / LifecycleRun / Agent
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

`target=sub` 是特例，因为 target resolution 同时创建 child `lifecycle_agents` 行（复用 parent 的 `run_id`）、`LifecycleRun.channels` 里的新 channel 条目和 first delivery。

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

## Resolved

**2026-07-07 二轮对齐（代码核实 + 用户决策）**：

- **Capability 落地路线**：Channel 从 v1 起进入 `CapabilityState` 作为一等 dimension，policy 用 `Accumulate`，effect 形态对齐 VFS 的 `apply_mount_operations`/`MountDirective`。不走 Workspace Module 式的 AgentFrame 平行列路线（Workspace Module 现状本身是历史权宜实现，不是值得复刻的目标）。详见 "Channel Capability" 一节。
- **Runtime effect payload**：`dimension=channel`、`effect_type=apply_channel_operations`，payload 是 `Vec<ChannelDirective>`（`Expose`/`Revoke`），不是单条 `expose_channel_ref`。
- **命名边界**：新的 Agent 通信 Channel 保留 `Channel` 命名。已核实 Extension Protocol Channel（`channel_key`/`protocol_channels`，domain→contracts→relay→runtime-gateway→前端 bridge 全链路，见 `crates/agentdash-domain/src/shared_library/value_objects.rs:1503-1610`）虽然技术上已上线，但实际使用面不大；后续如果需要消解命名冲突，方向是重命名 Extension Protocol Channel 或将其收束为未来统一 Channel 体系下的一个 `ChannelMedium`/`ChannelTopology`（例如把插件 RPC method 分组视为一种 `Channel` 的 medium），而不是让新概念让路。这不是本任务 v1（Companion lifecycle channel）要处理的范围——v1 只涉及 Companion/SubAgent，不接触 extension runtime，命名冲突只在 Channel 扩展到更大范围时才会真正碰头；重命名/收束 Extension Protocol Channel 本身是有独立影响面（TS codegen、relay 协议、权限字符串）的后续任务，需要单独立项，不能在本任务内顺手改。
- **MVP 切片范围**：确认为 Companion / SubAgent lifecycle-scoped temporary channel（对应 implement.md Phase 1-5），不是多 Agent LifecycleRun/Project/Story 广播或外部 IM。

**2026-07-07 三/四/五轮（持久化模型的三次纠正，最终版本）**：

- **Companion `target=sub` 不创建新 `run_id`**：parent/child 共享同一个 `LifecycleRun`，child 只是这个 run 下新增的一个 `lifecycle_agents` 行。`lifecycle_runs` 本身天然是单/多 Agent 共存容器。
- **Channel 是 `LifecycleRun` 上的结构化字段，不是独立表**：对齐 `orchestrations`/`execution_log` 的既有模式（`ALTER TABLE lifecycle_runs ADD COLUMN ... text DEFAULT '[]'`），`LifecycleRun.channels: Vec<LifecycleChannel>` 序列化为 `lifecycle_runs` 的新列。不建 `channels`/`lifecycle_channels`/`channel_participants` 表——这个项目的既有约定是"结构化字段挂在已有聚合上"，不是遇到新概念就开新表。参与关系由各参与方 `CapabilityState.channel.visible_channels` 里指向该 channel `id` 的引用表达，不重复存储。
- **`ChannelOwner` 收窄**：`AgentRun { run_id, agent_id }` 与 `AgentTeam { team_id }` 合并为 `LifecycleRun { run_id }`——AgentTeam 不是独立实体，只是"一个 run 下多 agent 协作"的说法。
- **ChannelAddress 迁移方式**：`MailboxSourceIdentity` 直接重定位为 `agentdash-domain::channel::ChannelAddress`，全部调用点直接迁移，不保留别名 / re-export；`display_label_key` 的 `"mailbox.source."` 硬编码前缀改参数化。

## Still Open

- Channel capability 通过 `PermissionGrant` 累积可见性目前没有通用机制可复用（现有 grant 判定硬编码基于 Tool 维度），如果 Channel 需要走 grant 驱动可见性，这部分需要新设计。
- 多 Agent LifecycleRun broadcast（长期形态）需要角色模型和 shared context 策略，否则 Channel 会先成为事件存储而无法体现协作价值。
- 外部 IM publish 需要 permission、audit、rate limit 和 identity mapping，否则风险远高于普通 internal channel。

## Recommended First Principle

Channel owns communication semantics. Mailbox owns AgentRun consumption.
