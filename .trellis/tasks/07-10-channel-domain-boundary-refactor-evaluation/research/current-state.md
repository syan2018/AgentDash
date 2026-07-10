# Channel 现状审计

本文件综合了 Channel 全链路独立只读审计，以及与 Workspace 双工设计之间的对抗性交叉复核；结论已按交叉审稿修正 persistence 与 identity 的证据边界。

## 1. 审计结论

仓库中的两套 `Channel` 只有“提供稳定寻址入口”这一点相似，稳定对象并不相同：

- Extension `protocol_channels[]` 稳定的是一个版本化 provider API：`channel_key + method + JSON input/output`，运行形态是一次 request/response 调用。
- 全局 `agentdash_domain::channel::Channel` 稳定的是一个有状态通信空间：有 owner、participant、binding、message、delivery、reply address、policy 与 close lifecycle。

因此二者不应继承同一个基类，也不应合并为一个万能 Channel。推荐把 `channel` 一词只留给通信领域；Extension 侧改称 `ExtensionProtocol`，其 methods 在 Workspace Module 中继续投影为 `Operation`。

## 2. Extension Protocol Channel 事实链

| 层 | 当前事实源 | 语义 |
| --- | --- | --- |
| Extension authoring | `packages/extension/src/app/types.ts`、`packages/extension/src/host/index.ts` | 声明 `protocol_channels` 与 methods |
| Domain package model | `crates/agentdash-domain/src/shared_library/value_objects.rs` | `ExtensionProtocolChannelDefinition`、method schema、visibility、dispatch |
| Runtime projection | `crates/agentdash-workspace-module/src/extension_runtime.rs` | 将 package 声明投影到当前 Extension runtime surface |
| Workspace Module | `crates/agentdash-workspace-module/src/workspace_module/mod.rs`、`surface.rs` | generated operation dispatch 到 `ProtocolChannel { channel_key, method }` |
| Runtime gateway | `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs` | `ExtensionRuntimeChannelInvoker` 做 admission、relay 与 trace |
| Relay/local host | `crates/agentdash-relay/src/protocol/extension_runtime.rs`、`crates/agentdash-local/src/extensions/host/**` | 通过 `invoke_channel` 调 Extension Host method |
| Browser SDK | `packages/extension/src/browser/index.ts` | `extension.invoke_channel` request/response bridge |

这一调用链没有 participant、membership、message history、delivery state、broadcast 或 binding topology。`channel_key` 实际承担 provider contract key，`method` 才是可调用 operation。

现有 identity/admission 还有两个独立问题：RuntimeGateway 会在 enabled installations 中按 `channel_key` 找首个命中，dispatch 没有显式 provider installation identity；protocol `version` 被投影但未成为真实 contract resolution 约束。raw `SessionUser` invocation 也比 Workspace Module operation exposure 更宽，未来 program executor 不能复用这条宽入口绕过 operation catalog。

## 3. 全局 Channel 事实链

| 维度 | 当前实现 | 观察 |
| --- | --- | --- |
| Aggregate | `crates/agentdash-domain/src/channel/mod.rs` | `ChannelRecord` 内嵌 Channel、participants、bindings、reply addresses、delivery state |
| Owner | `Project / Story / LifecycleRun / System` | 模型允许四种 owner，实际 store 仅支持 LifecycleRun |
| Persistence | `lifecycle_runs.channel_registry jsonb` | migration `0057_lifecycle_run_channel_registry.sql`；不是独立全局存储 |
| Application | `crates/agentdash-application/src/channel.rs` | 只有 `LifecycleRunChannelOwnerStore`；binding resolver 默认 unsupported |
| Runtime exposure | AgentRun runtime capability 的 `visible_channels` | Channel capability 被投影给运行中 Agent |
| Materialization | mailbox、lifecycle gate、publish outbox/provider event | Channel 负责通信投递，不负责业务 command 执行 |

全局 Channel 的领域方向是合理的，但当前模型里有若干维度交叉：

- `ChannelMedium::{Runtime, Project, Im, Human, Terminal, System}` 同时混入 scope、transport、endpoint/audience。
- `ChannelLifecycle::{Runtime, Persistent, Ephemeral}` 同时混入 owner-bound lifetime、存储方式和 message retention。
- `ChannelParticipantRef` 存在 `AgentRun/LifecycleAgent`、`User/Human`、`System/Platform` 成对重叠。
- `ChannelAddress` 同时携带 source、actor、correlation、route 与 display metadata，实际兼任 message origin 和 reply target。
- `ChannelRef { owner, channel_id }` 用 owner 做 store routing，但 UUID 已是全局身份；aliases 只校验非空，没有 owner 内唯一性。
- owner enum 声称支持 Project/Story/System，但 persistence 与 application service 无法兑现。
- `plan_broadcast_deliveries` 尚未闭合 sender membership、operation、open status、audience 与 ingress/egress admission；多个 policy 字段目前只被保存/投影。
- Runtime capability directive 可以直接 `Expose` 一个 ChannelRef，形成绕开 registry membership 的第二事实源。
- hook auto-resume 等消费者已把 `ChannelMessage/ChannelAddress` 当成通用 delivery envelope，甚至用非 registry identity 伪造 channel_id，说明 provenance/delivery 语言需要从 Channel aggregate 拆出。

## 4. 逐维对照

| 维度 | Extension Protocol Channel | 全局 Channel | 结论 |
| --- | --- | --- | --- |
| 稳定身份 | extension installation/package + `channel_key` | `ChannelId`，另带 owner/aliases | 隔离 |
| 行为 | method request/response | publish/read/reply/broadcast/delivery | 隔离 |
| 状态 | provider host 的业务状态；调用面本身无会话状态 | membership、binding、status、delivery state | 隔离 |
| 生命周期 | 跟随 extension package/runtime activation | owner-bound 或显式 close | 隔离 |
| 权限 | extension permission + operation visibility + runtime admission | participant operations + ingress/egress policy | 只共享 actor/capability 基础设施，不共享领域模型 |
| transport | relay 到 selected local backend/Extension Host | internal mailbox、gate、outbox、外部 provider binding | Extension protocol 可实现 adapter，但不是 Channel 本身 |
| trace | 每次 invocation trace | message/delivery correlation | 可用关联 ID 串联，不合并对象 |
| Workspace Module | method 被投影为 operation | Channel capability 被投影为通信资源 | 两类资源分别描述 |

## 5. 与通用交互系统的边界

- `Operation` 表达一次原子、受 admission 控制的调用；MCP tool、Extension protocol method、runtime action 都可贡献 Operation。
- `OperationProgram` 表达一组有界 Operation 的组合执行。
- `InteractionInstance` 表达人和 AI 共同读写的状态对象及 command/event。
- `Channel` 表达参与者之间的消息、关注、唤醒、handoff 与异步 delivery。

Channel message 可以引用 `interaction_instance_id`、`interaction_event_id` 或 `operation_execution_id`，但 Channel 不拥有交互状态，也不承担同步 operation dispatch。

## 6. 当前重构风险

- `protocol_channels` 跨 Rust contracts、TS SDK、generated manifest、relay/local host 和示例，改名必须原子完成；项目未上线，不建设兼容字段或双读。
- 全局 Channel 若继续支持多 owner，必须脱离 `lifecycle_runs.channel_registry`；这会涉及 database migration 与 repository 事务边界。
- 若保留 aliases 作为稳定寻址，需要 owner 内唯一约束；若只是展示/搜索，应从 `ChannelRef` 中移除其身份含义。
- 当前消息、mailbox、gate、outbox 各自已有持久化职责；若选择独立 Channel aggregate，也不应复制完整 message body 或建立第二套投递事实源。
- Extension manifest 同时存在于 library asset、package artifact 与 project installation JSONB snapshot；协议改名会改变 artifact digest，迁移应重建 owned artifacts/install snapshots，不增加旧 decoder。
