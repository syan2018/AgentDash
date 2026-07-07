# ChannelService 文档型通信主干一期实现

## Goal

交付一套可派发的一期实现任务：建立 `agentdash-domain::channel` 领域模型、owner-local `ChannelRegistryDocument` 持久化形态、application 层 `ChannelService` skeleton，以及 `CapabilityState.channel` 投影边界。

Channel 是一等通信领域和服务主干，但一等领域不等于一等关系表。Project 公共 Channel、企业 IM binding、Lifecycle runtime channel、Companion/SubAgent runtime channel 都通过同一套 Channel 语言表达；物理存储优先跟随业务 owner 的文档聚合生灭：

- Project-scoped persistent channel 通过 `ChannelOwnerStore` 抽象接入，物理承载等待 Project Assets 系统收束后决定。
- LifecycleRun-scoped runtime channel 存在 LifecycleRun 业务文档中。
- Mailbox 当前仍作为 materialized AgentRun delivery 的消费边界；它的物理表形态不作为 Channel 设计先例。
- LifecycleGate 仍只保存 wait/result authority。

本任务要让后续企业 IM 和 Companion/SubAgent 迁移都能接入同一 `ChannelService`，同时避免新增 `channels` / `channel_participants` / `channel_bindings` 这类把临时运行时通信事实过度关系型化的表。

## Background

最新对齐结论包含两层：

1. Channel 不能被理解为 `LifecycleRun` 附属概念。它是 Project / Story / LifecycleRun / 外部 IM / Companion / Terminal 等来源共享的通信领域。
2. Channel 的持久化不应默认拆成独立关系表。Agent runtime 事实高频随 Lifecycle / SubAgent 创建和释放，适合以 owner-local document 的方式挂在业务聚合下，由 `ChannelService` 维护一致性。

已有基础仍然有效：

- `LifecycleRun` 已有 `orchestrations`、`tasks`、`execution_log` 这类文档型列，适合保存 runtime-scoped channel registry。
- AgentRun Mailbox 已具备开放 `MailboxSourceIdentity`，可作为 `ChannelAddress` / delivery attribution 的迁移基础。
- Capability 维度管线支持 `AccumulationPolicy::Accumulate`，可用于把 Channel 可见/可操作投影写入 `CapabilityState.channel`。
- Companion reply contract 已完成模型可见合同收窄，适合作为未来 Channel facade 的局部基础，但不是 Channel 架构最终形态。

## Requirements

- R1: 新增 `agentdash-domain::channel` 领域模型，覆盖 `Channel`、`ChannelRegistryDocument`、`ChannelParticipant`、`ChannelBinding`、`ChannelPolicy`、`ChannelMessage`、`ChannelDelivery`、`ChannelAddress`、`ChannelCapabilityRef`。
- R2: 新增 owner-local registry 持久化形态：Lifecycle runtime channel registry 写入 LifecycleRun 业务文档；Project/IM registry 通过 `ChannelOwnerStore` port 表达，不在本任务固定到 ProjectConfig 或具体资产表。
- R3: 新增 application 层 `ChannelService` skeleton，作为创建 channel、维护 participants / bindings / broadcast policy、规划 delivery intent 和生成 capability projection 的唯一入口。
- R3a: `ChannelService` 必须按 owner lazy load registry；不得在服务启动时扫描全部 Project / LifecycleRun / Assets 并预加载 Channel。
- R4: 明确不新增独立 `channels` / `channel_participants` / `channel_bindings` 表；后续只有在存在独立生命周期、跨 owner 全局查询、真实多 worker 抢占或长期审计保留时，才为具体事实新增表。
- R5: 新增 `LifecycleRun.channel_registry` 文档列或等价业务语义列，列名表达业务语义，不使用 `_json` 后缀；Project 侧只定义 owner store contract。
- R6: 新增 `CapabilityState.channel` 一等 dimension 与 projection contract：visible channel refs、aliases、allowed operations、readiness、ingress/egress policy 由 Channel registry 派生，不作为 membership 事实源。
- R7: `ChannelService` 只输出 delivery intent / materialization command；Mailbox scheduler 继续拥有 AgentRun input queue、claim、launch/steer、恢复与状态投影。
- R8: `ChannelService` 只引用 gate delivery intent；LifecycleGate 继续拥有 wait/result payload、resolution 与 watcher 语义。
- R9: 一期只做 provider-neutral IM binding / ingress envelope 合同，不实现具体 Slack / 飞书 / Teams adapter。
- R10: Companion/SubAgent 只做 facade 接入点和 runtime channel registry 语义，不在本任务内迁移全部旧路径。

## Deliverables

- `agentdash-domain::channel` 模块及单元测试。
- LifecycleRun owner-local `ChannelRegistryDocument` 读写字段、serde default、repository roundtrip；Project/IM owner store trait 与 DTO 边界。
- `ChannelService` application skeleton，覆盖 project-owned channel create/update、lifecycle runtime channel create/update、participant policy update、delivery intent planning、capability projection。
- owner-scoped lazy loading contract：每次由 AgentFrame projection、IM ingress、Companion facade 或 delivery materialization 携带 owner ref 触发 registry resolve。
- `CapabilityState.channel` dimension skeleton，包含 typed declaration/effect payload validation 与 projection normalization。
- `ChannelAddress` 从 mailbox source identity 提炼出的值对象边界；本任务可先新增类型与 mapper，不强制迁移全部调用点。
- 数据库 migration：只允许新增 owner document column，例如 `lifecycle_runs.channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL`；Repository 映射为 typed `ChannelRegistryDocument`，不新增 channel 独立表。
- 测试覆盖 owner document roundtrip、service 边界、capability projection、mailbox/gate 不被替代。
- 更新 `.trellis/spec/backend/database-guidelines.md`，沉淀 Agent runtime aggregate 的文档型持久化原则。

## Acceptance Criteria

- [ ] `design.md` 和实现代码均以 `ChannelService` + owner-local `ChannelRegistryDocument` 为主干，不以 `LifecycleChannel` 或独立 `channels` 表作为目标模型。
- [ ] `Channel` participants / binding / broadcast policy 是 Channel registry 文档事实；`CapabilityState.channel` 只是 AgentFrame 可见操作投影。
- [ ] Project 公共 Channel / 企业 IM binding 可在没有 active LifecycleRun 的情况下通过 `ChannelOwnerStore` contract 被定义和读取；本任务不固定 Project Assets 的物理存储。
- [ ] Lifecycle runtime channel 可随 `LifecycleRun` 生灭，不需要额外清理孤立 channel rows。
- [ ] `ChannelService` 无启动期全局扫描逻辑；测试或静态检查覆盖 registry 只按 owner ref 加载。
- [ ] Mailbox materialization 只发生在需要 AgentRun 消费调度时；Mailbox 不成为第二套 Channel store。
- [ ] LifecycleGate wait/result 事实边界保持不变；Channel delivery intent 不保存 gate payload。
- [ ] migration 不新增 `channels`、`channel_participants`、`channel_bindings` 表。
- [ ] 新增 owner document column 使用 `jsonb`，Repository 使用 typed `ChannelRegistryDocument` 映射，不使用字符串 JSON 协议。
- [ ] repository / service tests 覆盖 Project owner store contract、LifecycleRun registry、capability projection 和 delivery planning 的基本 roundtrip。
- [ ] 新 spec 明确说明为何 Agent runtime 事实优先采用 owner aggregate document，并给出拆表判断矩阵。

## Out Of Scope

- 不实现具体企业 IM provider adapter。
- 不决定 Project 公共 Channel 在未来 Assets 系统中的物理表/文档形态。
- 不实现完整 `ChannelMessage` event log。
- 不迁移所有 Companion / Terminal / system wake 旧路径。
- 不重写 AgentRun Mailbox scheduler。
- 不重构既有 `LifecycleGate` / `agent_run_mailbox_messages` / `agent_run_lineages` 表；本任务只记录新建模型的持久化原则，既有表另立清理任务评估。
- 不改写归档任务；归档的 Companion reply contract 任务保持历史事实。

## Superseded Conclusions

- **推翻：第一版实现范围应窄，优先 Companion/SubAgent lifecycle-scoped temporary channel。**
  新结论：Companion/SubAgent 可作为第一条验证链路，但一期先交付通用 `ChannelService` 和 owner-local registry。
- **推翻：Channel 的家是共享 LifecycleRun，不新建表，以 `LifecycleRun.channels` 保存。**
  新结论：`LifecycleRun` 是 runtime scope 的 owner document；Project 侧通过 owner store/Assets 系统承载；领域模型仍是通用 Channel。
- **推翻：Channel 必须用独立 `channels` / `channel_participants` 表表达一等性。**
  新结论：一等性落在领域、服务和能力投影上；高频 runtime 通信事实优先存入 owner aggregate document。
- **推翻：参与者不用字段或表，由 `CapabilityState.channel.visible_channels` 表达。**
  新结论：participants、membership、broadcast policy 属于 Channel registry 文档事实；`CapabilityState.channel` 是 AgentFrame 可见操作投影。
- **推翻：Project/Story/外部 IM 的 `ChannelMessage` / `ChannelDelivery` 等到后续再定义。**
  新结论：可以不首期落完整 event log，但 Message / Delivery / DeliveryIntent 边界必须现在定义。
- **保留但重解释：`CapabilityState.channel` 作为一等 dimension。**
  新结论：它是 ChannelService / participant policy 对 AgentFrame 的投影，不是 Channel membership 事实源。
- **保留：`ChannelAddress` 从 `MailboxSourceIdentity` 抽象出来。**
  新结论：它是 delivery/source attribution 值对象，不能替代 Channel 实体、Binding、Message 或 Delivery。

## Dispatch Notes

- 实现顺序从 domain document 和 service skeleton 开始，再接 capability projection，最后补 mailbox/gate materialization intent 测试。
- DB 设计默认采用 owner-local document。派发者只有发现强并发 claim、跨 owner 全局索引或长期审计保留需求时，才应提出新表设计，并在设计文档里写明原因。
