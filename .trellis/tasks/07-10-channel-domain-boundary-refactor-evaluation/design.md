# Design · Channel 术语与领域边界收敛

## 1. 推荐决策

保留全局通信领域的 `Channel` 名称；将 Extension 的 `ProtocolChannel` 原子重命名为 `ExtensionProtocol`，并让其 method 只以 `Operation` 形式进入 Workspace Module 与 Canvas runtime surface。

```text
ExtensionProtocol.method ─┐
MCP tool ─────────────────┼─> Operation Catalog -> Runtime admission/dispatch
Runtime action ───────────┘

Interaction/Agent ── message or attention ref ──> Channel -> delivery adapters
```

两条主线只在 actor identity、capability、trace/correlation 和 provider adapter 处复用基础设施，不共享 aggregate。

## 2. 目标词汇

| 当前词汇 | 目标词汇 | 含义 |
| --- | --- | --- |
| `ExtensionProtocolChannelDefinition` | `ExtensionProtocolDefinition` | Extension 暴露的版本化 provider contract |
| `channel_key`（Extension） | `protocol_key` | Extension package 内稳定 contract key |
| `ProtocolChannel { channel_key, method }` dispatch | `ProtocolMethod { protocol_key, method }` | Operation 的内部 dispatch provenance |
| `ExtensionRuntimeChannelInvoker` | `ExtensionProtocolInvoker` | Extension protocol method adapter |
| `extension.invoke_channel` | `extension.invoke_protocol`（底层） | Host transport method；上层 UI 优先调用 Operation |
| `Channel`（全局） | `Channel` | 有 participant、message 与 delivery 的通信空间 |

`Protocol` 是 provider contract；`Operation` 是统一可调用投影；`Channel` 是通信空间。Workspace Module 只组织和描述资源，不拥有这三者的事实源。

## 3. ExtensionProtocol 目标模型

```rust
struct ExtensionProtocolDefinition {
    protocol_key: String,
    contract_version: SemVer,
    methods: Vec<ExtensionProtocolMethodDefinition>,
}

struct ExtensionProtocolMethodDefinition {
    method: String,
    input_schema: JsonSchema,
    output_schema: JsonSchema,
    permissions: Vec<CapabilityRequirement>,
    visibility: OperationVisibility,
}
```

关键不变量：

- `protocol_key + method` 只在确定的 extension package/install identity 内唯一。
- 完整调用引用包含 provider extension/install identity、protocol key、method 与 contract version requirement；不得在全部 installation 中按 key 首个命中。
- manifest 定义是 authoring 事实源；operation catalog 是生成投影。
- Agent、Canvas、Extension component 不直接获得未裁剪的 protocol invoker，只获得 actor-specific Operation surface。
- typed client 可以保留 protocol 心智，但最终通过统一 Operation invoke 进入 RuntimeGateway；底层 protocol dispatch 只是 adapter provenance。

## 4. 全局 Channel 目标模型

### 4.1 身份与 owner

推荐使用全局 `ChannelId` 作为内部权威身份，并新增 owner 内唯一的 `ChannelKey` 作为稳定业务地址；aliases 只用于显示/检索，不参与 authority resolution。

```text
ChannelId             globally unique authority identity
ChannelOwner          authorization + lifetime boundary
ChannelKey            owner-local stable product key
ChannelLocator        owner + channel_key stable lookup
aliases               non-authoritative display/search labels
```

Companion 等消费者使用原子 `create_if_absent(ChannelLocator)`，不再扫描 aliases 后 upsert。exact `ChannelRef` 的形状跟随 persistence 决策：owner-local registry 使用 `owner + channel_id`，独立 aggregate repository 只使用 `channel_id` 并从 row 校验 owner；不同时保留两份 authority。owner store 或独立 repository 均必须验证 record owner 与 locator owner 一致。

### 4.2 正交维度

删除当前 `ChannelMedium`。目标维度为：

- `ChannelPurpose`：conversation、notification、coordination 等业务用途；仅在确有行为差异时引入。
- `ChannelBinding`：internal mailbox、terminal、IM/provider room 等 transport/endpoint。
- `ChannelOwner`：scope/lifetime authority；只保留 owner evidence gate 后有真实消费者与 store 的 variants。

删除当前 `ChannelTopology`：direct/group 应由 active participant cardinality 表达，broadcast 属于 delivery audience/policy，thread 属于 message relation 或外部 binding。若未来确有行为不同的 topology，再以可执行不变量重新引入。

删除把存储方式写入领域的 `Persistent/Ephemeral`。改用：

- `ChannelLifetimePolicy`：owner-bound 或 explicit-close。
- `ChannelRetentionPolicy`：message/delivery metadata 的保留窗口和上限。

### 4.3 participant、origin 与 reply

- 收束到一个 canonical `PrincipalRef` 或 `ChannelParticipantRef::{Agent, User, External, Service}`，不保留同义 variant。
- 将当前 `ChannelAddress` 拆为 `ChannelMessageOrigin` 与 `ChannelReplyTarget`；correlation 属于 message envelope，display metadata 属于 projection。
- participant policy 继续拥有 read/receive/reply/publish/manage 权限；Runtime capability 只投影 actor 实际可用的子集。
- ChannelService 在每次 publish/reply/broadcast 时重新校验 status、membership、operation 与 audience；AgentFrame projection 只服务发现/UX，不成为最终授权。

### 4.4 persistence

当前 production 证据只覆盖 LifecycleRun/Companion；Project、Story、System 只有 enum 声明，没有 store 与真实消费者。`visible_channels` 是 AgentRun capability projection，不能单独证明 Channel 必须跨生命周期存在。

因此 persistence 先经过 owner evidence gate：逐一确认每类 owner 的创建者、稳定生命周期、查询方式、外部 binding resolution 与事务边界，然后二选一。

### 方案 A · 收窄为 LifecycleRun child

- 删除没有消费者的 Project/Story/System owner。
- 保留 owner-local registry，但升级 schema、补齐 owner/key/status/admission validation。
- `ChannelRef = owner + channel_id`，external binding 使用可重建 reverse index。

### 方案 B · 多 owner 独立 aggregate

只有确认 Project/System 等跨生命周期 use case 后，才建立独立 persistence：

```text
channels
channel_participants
channel_bindings
channel_reply_targets
channel_delivery_state
```

该方案将现有 `lifecycle_runs.channel_registry` 通过一次 migration 转换后删除，`ChannelRef = channel_id`。如果选择 owner-local multi-store 而非独立 aggregate，则必须实现 `ChannelOwnerStoreRouter`、owner validation 与可重建的 external binding reverse index；不能靠扫描 owner documents 解析 inbound provider event。

不允许“模型声明多 owner、runtime 只有 LifecycleRun store”的中间状态进入实现。

## 5. Provider adapter 关系

Extension/Integration 可以贡献两类彼此独立的 adapter：

1. `OperationProvider`：将 protocol method 投影为可调用 Operation。
2. `ChannelBindingProvider`：解析外部 room/thread/user，规范化 inbound event，并 materialize outbound publish。

同一个 Extension package 可以同时贡献二者，但一个 `ExtensionProtocol` 不自动成为 Channel binding。外部 IM provider 的 protocol methods 可以被 `ChannelBindingProvider` 内部调用，调用结果仍由 Channel application service 记录 delivery state。

## 6. 关键数据流

### Operation 调用

```text
Agent / Canvas / Component
  -> actor-specific OperationRef
  -> RuntimeGateway admission
  -> ExtensionProtocolInvoker
  -> selected backend / Extension Host
  -> method result + child trace
```

### Channel inbound

```text
provider event
  -> ChannelBindingProvider.normalize
  -> resolve ChannelId + participant
  -> Channel ingress policy
  -> ChannelMessage
  -> mailbox/gate/attention delivery
```

两条流可以共享 correlation/trace ref，但不会互相绕过 admission 或写对方 aggregate。

## 7. 被否决的方向

| 方向 | 原因 |
| --- | --- |
| 建立共同 `StableChannel` 基类 | 只抽出了“有 key”这一偶然相似点，无法提供共同不变量 |
| 将所有 operation 都建模为 Channel message | 同步 schema validation、返回值、取消和权限会退化成自制 RPC bus |
| 将全局 Channel 当作 Interaction event store | membership/delivery 与交互状态一致性是不同事务边界 |
| 只在文案中区分，保留代码同名 | SDK、manifest、dispatch 与诊断仍会持续误导后续设计 |

## 8. Review Gate

进入实现前需要确认：

- `Channel` 一词只属于通信领域。
- Extension authoring 采用 `ExtensionProtocol`，统一调用投影采用 `Operation`。
- 根据真实 owner/use case 证据选择独立 aggregate 或明确收窄为 LifecycleRun child；不从 capability projection 推导 persistence。
- Workspace 双工交互任务只通过引用与 Channel 连接，不把 command/event 并入 Channel。
