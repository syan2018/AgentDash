# ChannelService 文档型通信主干执行计划

## Current State

本任务已从长期预评估收束为可派发的一期实现任务。

最新决策：

- Channel 是一等领域与 `ChannelService` 主干。
- Channel 持久化采用 owner-local `ChannelRegistryDocument`，不新增独立 channel 关系表。
- 新增 owner document 物理列使用 `jsonb`，repository 映射为 typed domain document。
- Project 公共 Channel 通过 `ChannelOwnerStore` 抽象接入，等待 Project Assets 系统决定物理承载。
- Lifecycle runtime channel 写入 LifecycleRun 业务文档，并随 LifecycleRun 生灭。
- `ChannelService` 是 owner-scoped lazy resolver，不做启动期全局扫描或预加载。
- `CapabilityState.channel` 是 AgentFrame 可见操作投影，不是 membership 事实源。
- Mailbox 与 LifecycleGate 继续保持各自 owner 边界。

## Implementation Phases

1. **Domain Document Model**
   - 新增 `agentdash-domain::channel` 模块。
   - 定义 `ChannelRegistryDocument`、`ChannelRecord`、`Channel`、`ChannelParticipant`、`ChannelBinding`、`ChannelPolicy`、`ChannelMessage`、`ChannelDeliveryIntent`、`ChannelDeliveryState`、`ChannelAddress`。
   - 所有 registry 文档字段提供 `serde(default)`，保证旧 Project / LifecycleRun row 可读。
   - 不定义 `LifecycleChannel`。

2. **Owner Document Persistence**
   - LifecycleRun 侧扩展 `LifecycleRun.channel_registry`。
   - 新增 migration：`lifecycle_runs.channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL` 或等价业务语义列。
   - 更新 LifecycleRun PostgreSQL repository 的 insert/select/update mapping 和 roundtrip tests。
   - 定义 Project/IM `ChannelOwnerStore` trait/DTO，不绑定 `ProjectConfig` 或具体资产表。
   - 不新增 `channels`、`channel_participants`、`channel_bindings` 表。

3. **ChannelService Skeleton**
   - 在 application 层新增 channel service / ports。
   - service 通过 `ChannelOwnerStore` / LifecycleRunRepository 读取和写回 owner registry。
   - registry 只能由具体 owner ref lazy load，不允许启动期扫描全部 Project / LifecycleRun / Assets。
   - 支持 project-owned channel create/update、lifecycle runtime channel create/update、participant add/remove、binding add/remove、policy update。
   - service 返回 typed delivery intent，不直接操作 scheduler 状态。

4. **Capability Dimension**
   - 新增 `CapabilityState.channel` dimension。
   - 注册 channel dimension module，声明 `AccumulationPolicy::Accumulate`。
   - 定义 `ChannelDirective::Expose/Revoke` typed payload validation。
   - 实现 `ChannelCapabilityProjector`，从 owner registry + participant policy 生成 visible channel refs、aliases、operations、readiness。

5. **Materialization Intent Boundary**
   - 定义 mailbox materializer adapter 接口：`ChannelDeliveryIntent -> AgentRunMailboxMessage`。
   - 定义 gate materializer adapter 接口：`ChannelDeliveryIntent -> LifecycleGate ref / wait intent`。
   - `ChannelAddress` 作为 mailbox source attribution 值对象；本任务可先新增 mapper，不强制迁移所有旧调用点。
   - 测试确认 materializer 不把 mailbox/gate 变成 Channel 事实源。

6. **Provider-neutral IM Contract**
   - 定义 External IM binding envelope：workspace / room / thread / user / message refs。
   - 定义 inbound event -> `ChannelMessage` 的 normalized shape。
   - 定义 outbound publish outbox intent shape。
   - 不实现具体 Slack / 飞书 / Teams adapter。

7. **Companion/SubAgent Facade Preparation**
   - 标注 `companion_request` / `companion_respond` 后续通过 ChannelService 的入口。
   - 为 `target=sub` 定义 runtime channel create/participant projection 测试数据。
   - 不在本任务迁移完整 Companion 旧路径。

## Files To Expect

实现者应优先检查并修改这些区域：

- `crates/agentdash-domain/src/channel/`
- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs`
- `crates/agentdash-infrastructure/migrations/`
- `crates/agentdash-spi/src/capability*` 或当前 capability state 所在模块
- `crates/agentdash-application*/` 中适合承载 `ChannelService` 的 application module

具体路径以代码搜索结果为准，优先遵守既有 crate 边界。

## Validation Plan

- Domain unit tests:
  - `ChannelRegistryDocument::default()` 可读空 registry。
  - channel id / owner / participant policy validation。
  - `ChannelAddress` 与 mailbox source attribution mapper。
- Repository tests:
  - LifecycleRun channel registry roundtrip。
  - 旧 row 默认 registry 兼容读取。
- Application tests:
  - `ChannelService` 通过 `ChannelOwnerStore` 处理 Project owner registry，不依赖具体 ProjectConfig 字段。
  - `ChannelService` 创建 Lifecycle runtime channel 后随 run owner 写入 registry。
  - `ChannelService` 不在构造或启动阶段列举所有 LifecycleRun。
  - delivery intent planning 不写 mailbox queue 状态。
- Capability tests:
  - registry participant policy 投影为 `CapabilityState.channel.visible_channels`。
  - capability projection 不反向修改 registry。
- Database checks:
  - 新 migration 只新增 owner document column。
  - `pnpm run migration:guard` 通过。
- Static checks:
  - `rg -n "CREATE TABLE .*channel|channel_participants|channel_bindings" crates/agentdash-infrastructure/migrations` 不出现本任务新增表。
  - `rg -n "LifecycleChannel" crates` 不出现新增一等模型。
  - `rg -n "list_all\\(|list_by_project\\(|scan.*LifecycleRun" crates/*/src/*channel* crates/*/src/**/channel*` 不出现 ChannelService 启动期全局扫描路径。

## Research Anchors

- `.trellis/tasks/07-07-channel-communication-capability-model/research/channel-service-first-principles-realignment.md`
- `.trellis/tasks/07-07-channel-communication-capability-model/research/channel-discussion-journal.md`
- `.trellis/tasks/07-07-channel-communication-capability-model/research/v1-decision-evidence-and-open-items.md`（其中 lifecycle-only 与独立 channel 表建议均已被最新 realignment 推翻；代码证据仍可参考）
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/tasks/06-28-agent-custom-channel-draft/design.md`
- `.trellis/tasks/06-28-integration-channel-mailbox-convergence/design.md`

## Decisions To Preserve

- Channel 是一等领域与 `ChannelService` 主干；它不等价于一等关系表。
- LifecycleRun owner document 是 runtime Channel registry 的一期持久化边界。
- 新增 Channel owner document column 使用 `jsonb`，列名使用业务语义名，Rust 侧映射为 typed `ChannelRegistryDocument`。
- Project owner store 的具体物理承载不在本任务决定；后续由 Assets 系统收束。
- ChannelService 只按 owner ref lazy load registry。
- Project 公共 Channel / 企业 IM 接入是明确需求，应从架构起点纳入。
- Channel participants、binding、broadcast policy、message/delivery planning 是 Channel registry 事实。
- `CapabilityState.channel` 是 AgentFrame 可见操作投影，不是 membership 或 policy 事实源。
- `ChannelAddress` 从 `MailboxSourceIdentity` 抽象出来的方向保留，但它只负责 source/delivery attribution。
- Mailbox 只负责 AgentRun durable consumption；LifecycleGate 只负责 wait/result authority。

## Follow-up Tasks

- 具体 IM provider adapter。
- 完整 Channel event log / audit outbox，如企业审计要求需要。
- Companion / SubAgent 旧路径迁移到 ChannelService。
- Terminal / async producer 旧 wake 路径迁移。
- 既有 `LifecycleGate`、`agent_run_mailbox_messages`、`agent_run_lineages` 是否应向 owner document 或更窄事实表收敛的独立数据库设计审计。
