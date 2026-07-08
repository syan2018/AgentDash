# ChannelService 完整通信主干推进

## Goal

交付 ChannelService 的完整端到端通信主干：建立 `agentdash-domain::channel` 领域模型、owner-local `ChannelRegistryDocument`、通用 owner document mutation 策略、`ChannelService`、`CapabilityState.channel`、Mailbox/Gate materialization，以及 Companion/SubAgent/async wake 旧路径向 ChannelService 的收束。

Channel 是一等通信领域和服务主干，但一等领域不等于一等关系表。Project 公共 Channel、企业 IM binding、Lifecycle runtime channel、Companion/SubAgent runtime channel 都通过同一套 Channel 语言表达；物理存储优先跟随业务 owner 的文档聚合生灭：

- LifecycleRun-scoped runtime channel 存在 `LifecycleRun.channel_registry` 业务文档中。
- Project-scoped persistent channel 通过 `ChannelOwnerStore` / `ChannelBindingResolver` 合同接入；Project Asset 物理承载由后续 Project Assets 任务决定。
- Mailbox 是 materialized AgentRun delivery 的 durable consumption scheduler，不是 Channel store。
- LifecycleGate 是 wait/result authority，不保存或复制进 Channel registry。

本任务要让 Companion/SubAgent/runtime wake 全量走 ChannelService，并让后续企业 IM 和 Project Channel Asset 任务能直接接入同一主干；本任务不新增 `channels` / `channel_participants` / `channel_bindings` 表。

## Background

最新对齐结论包含三层：

1. Channel 不能被理解为 `LifecycleRun` 附属概念。它是 Project / Story / LifecycleRun / 外部 IM / Companion / Terminal 等来源共享的通信领域。
2. Channel 的持久化不应默认拆成独立关系表。Agent runtime 事实高频随 Lifecycle / SubAgent 创建和释放，适合以 owner-local document 的方式挂在业务聚合下，由 `ChannelService` 维护一致性。
3. owner-local document 需要通用原子 mutation 策略。当前 `LifecycleRunRepository::update` 是整聚合写回，不能让 ChannelService 通过 load-modify-update 覆盖 orchestration/task/status 的并行更新，也不能让 broad aggregate update 覆盖 `channel_registry`。

已有基础仍然有效：

- `LifecycleRun` 已有 `orchestrations`、`tasks`、`execution_log` 这类文档型列，适合保存 runtime-scoped channel registry；新增目标列必须按当前规范使用 `jsonb` + typed value object。
- AgentRun Mailbox 已具备开放 `MailboxSourceIdentity`，可作为 `ChannelAddress` / delivery attribution 的迁移基础。
- Capability 维度管线支持 `AccumulationPolicy::Accumulate`，可用于把 Channel 可见/可操作投影写入 `CapabilityState.channel`。
- Companion reply contract 已完成模型可见合同收窄，适合作为 Channel 语义工具入口的用户体验层；实现边界必须进入 ChannelService。

## Requirements

- R1: 新增 `agentdash-domain::channel` 领域模型，覆盖 `Channel`、`ChannelRegistryDocument`、`ChannelRegistryMutation`、`ChannelParticipant`、`ChannelBinding`、`ChannelPolicy`、`ChannelMessage`、`ChannelDeliveryIntent`、`ChannelDeliveryState`、`ChannelAddress`、`ChannelCapabilityRef`。
- R2: 新增 owner-local registry 持久化形态：Lifecycle runtime channel registry 写入 `LifecycleRun.channel_registry`；Project/IM registry 通过 owner store contract 表达，不固定到 `ProjectConfig` 或具体资产表。
- R3: 新增通用 owner document mutation contract：repository 在事务中 row-lock owner、typed decode document、应用 domain mutation、只写目标 document column 和 `updated_at`；application/domain 只暴露语义 mutation port，不暴露任意 table/column 字符串。
- R4: `LifecycleRunRepository::update` 不写 `channel_registry`，避免 stale `LifecycleRun` 覆盖独立 document column；Channel registry 只能通过 owner document mutation port 更新。
- R5: 新增 application 层 `ChannelService`，作为创建 channel、维护 participants / bindings / broadcast policy、规划 delivery intent、materialize delivery 和生成 capability projection 的唯一入口。
- R6: `ChannelService` 必须按 owner lazy load registry；不得在服务启动时扫描全部 Project / LifecycleRun / Assets 并预加载 Channel。
- R7: 新增 `ChannelOwnerStore` 与 `ChannelBindingResolver`：`load_registry(owner)`、`mutate_registry(owner, mutation)`、provider-neutral binding lookup。未实现 IM provider 时，binding lookup 只能返回 unresolved / unsupported，不允许扫描 Project/LifecycleRun。
- R8: 明确不新增独立 `channels` / `channel_participants` / `channel_bindings` 表；只有在存在独立生命周期、跨 owner 全局查询、真实多 worker 抢占、数据库唯一约束保护的跨聚合不变量或长期审计保留时，才为具体事实新增表。
- R9: 新增 `LifecycleRun.channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL` 或等价业务语义列；Repository 映射为 typed `ChannelRegistryDocument`，不使用字符串 JSON 协议。
- R10: 新增 `CapabilityState.channel` 一等 dimension 与 projection contract：visible channel refs、aliases、allowed operations、readiness、ingress/egress policy 由 Channel registry 派生，不作为 membership 事实源。
- R11: `ChannelService` 只输出 delivery intent / materialization command；Mailbox scheduler 继续拥有 AgentRun input queue、claim、launch/steer、恢复与状态投影。
- R12: `ChannelService` 只引用 gate delivery intent；LifecycleGate 继续拥有 wait/result payload、resolution 与 watcher 语义。
- R13: `ChannelAddress` 从 mailbox source identity 提炼为通用 attribution 值对象；Mailbox 映射层保留 `mailbox.source.*` 展示语义。
- R14: Companion/SubAgent/human response/terminal async wake 旧路径必须通过 ChannelService 生成 `ChannelMessage` / `ChannelDeliveryIntent`，再由 mailbox/gate materializer 落到各自 owner。
- R15: Provider-neutral IM binding / ingress envelope 合同必须定义；具体 Slack / 飞书 / Teams adapter 不在本任务实现。

## Deliverables

- `agentdash-domain::channel` 模块及单元测试。
- 通用 owner document mutation 规范、repository helper / port、LifecycleRun registry mutation 实现与测试。
- LifecycleRun owner-local `ChannelRegistryDocument` 读写字段、serde default、repository roundtrip；Project/IM owner store trait、binding resolver 与 DTO 边界。
- `ChannelService` application module，覆盖 lifecycle runtime channel create/update、project-owned contract create/update、participant policy update、binding update、delivery planning、materialization、capability projection。
- owner-scoped lazy loading contract：每次由 AgentFrame projection、IM ingress、Companion 语义工具入口或 delivery materialization 携带 owner ref 触发 registry resolve。
- `CapabilityState.channel` dimension，包含 typed declaration/effect payload validation、Accumulate replay 与 projection normalization。
- `ChannelAddress` 值对象及 mailbox source attribution mapper。
- Mailbox/Gate materializer：`ChannelDeliveryIntent -> AgentRunMailboxMessage`、`ChannelDeliveryIntent -> LifecycleGate ref / wait intent`，并验证不复制各自 owner payload。
- Companion/SubAgent/runtime wake 旧路径收束到 ChannelService。
- 数据库 migration：只允许新增 owner document column，不新增 channel 独立表。
- 更新 `.trellis/spec/backend/database-guidelines.md`，沉淀 owner document 原子 mutation 策略与 Agent runtime aggregate 文档型持久化原则。

## Acceptance Criteria

- [ ] `design.md` 和实现代码均以 `ChannelService` + owner-local `ChannelRegistryDocument` 为主干，不以 `LifecycleChannel` 或独立 `channels` 表作为目标模型。
- [ ] `Channel` participants / binding / broadcast policy 是 Channel registry 文档事实；`CapabilityState.channel` 只是 AgentFrame 可见操作投影。
- [ ] Project 公共 Channel / 企业 IM binding 可在没有 active LifecycleRun 的情况下通过 `ChannelOwnerStore` / `ChannelBindingResolver` contract 被定义、读取或返回明确 unresolved；本任务不固定 Project Assets 的物理存储。
- [ ] Lifecycle runtime channel 可随 `LifecycleRun` 生灭，不需要额外清理孤立 channel rows。
- [ ] owner document mutation 通过 row lock + typed document mutation 原子更新，且只写目标 document column。
- [ ] `LifecycleRunRepository::update` 不覆盖 `channel_registry`；测试覆盖 stale run update 后 registry 保留。
- [ ] `ChannelService` 无启动期全局扫描逻辑；测试或静态检查覆盖 registry 只按 owner ref 加载。
- [ ] Mailbox materialization 只发生在需要 AgentRun 消费调度时；Mailbox 不成为第二套 Channel store。
- [ ] LifecycleGate wait/result 事实边界保持不变；Channel delivery intent 不保存 gate payload。
- [ ] Companion request/respond、SubAgent result、human response、terminal/exec wake 通过 ChannelService materialize；旧直接投递路径不存在或只保留在 materializer 内。
- [ ] migration 不新增 `channels`、`channel_participants`、`channel_bindings` 表。
- [ ] 新增 owner document column 使用 `jsonb`，Repository 使用 typed `ChannelRegistryDocument` 映射，不使用字符串 JSON 协议。
- [ ] repository / service tests 覆盖 Project owner store contract、binding unresolved、LifecycleRun registry、capability projection、delivery planning 和 materialization roundtrip。
- [ ] 新 spec 明确说明为何 Agent runtime 事实优先采用 owner aggregate document，并给出拆表判断矩阵和 mutation 策略。

## Out Of Scope

- 不实现具体企业 IM provider adapter。
- 不决定 Project 公共 Channel 在未来 Assets 系统中的物理表/文档形态。
- 不实现完整 `ChannelMessage` event log 或长期审计 outbox。
- 不重写 AgentRun Mailbox scheduler。
- 不迁移 Platform broker 缺失路径；除非已有 durable broker fact 可接入，否则 `target=platform` 保持 missing broker diagnostic。
- 不重构既有 `LifecycleGate` / `agent_run_mailbox_messages` / `agent_run_lineages` 表；既有表另立清理任务评估。
- 不并入 `07-08-database-jsonb-storage-cleanup` 的存量 TEXT JSONB 全仓清理；本任务只实现 Channel 所需的新 JSONB 文档和通用 mutation 策略。
- 不改写归档任务；归档的 Companion reply contract 任务保持历史事实。

## Superseded Conclusions

- **推翻：第一版实现范围应窄，优先 Companion/SubAgent lifecycle-scoped temporary channel。**
  新结论：Companion/SubAgent/runtime wake 是本任务必须打通的验证与收束路径；Channel 主干仍以通用 `ChannelService` 和 owner-local registry 表达。
- **推翻：Channel 的家是共享 LifecycleRun，不新建表，以 `LifecycleRun.channels` 保存。**
  新结论：`LifecycleRun` 是 runtime scope 的 owner document；Project 侧通过 owner store/Assets 系统承载；领域模型仍是通用 Channel。
- **推翻：Channel 必须用独立 `channels` / `channel_participants` 表表达一等性。**
  新结论：一等性落在领域、服务和能力投影上；高频 runtime 通信事实优先存入 owner aggregate document。
- **推翻：owner document 可以用普通 load-modify-update 写回。**
  新结论：owner document 必须有通用原子 mutation 策略，避免 broad aggregate update 和并行 document update 互相覆盖。
- **推翻：参与者不用字段或表，由 `CapabilityState.channel.visible_channels` 表达。**
  新结论：participants、membership、broadcast policy 属于 Channel registry 文档事实；`CapabilityState.channel` 是 AgentFrame 可见操作投影。
- **推翻：Project/Story/外部 IM 的 `ChannelMessage` / `ChannelDelivery` 等到后续再定义。**
  新结论：完整 event log 可以后置，但 Message / Delivery / DeliveryIntent 边界必须现在定义。
- **保留但重解释：`CapabilityState.channel` 作为一等 dimension。**
  新结论：它是 ChannelService / participant policy 对 AgentFrame 的投影，不是 Channel membership 事实源。
- **保留：`ChannelAddress` 从 `MailboxSourceIdentity` 抽象出来。**
  新结论：它是 delivery/source attribution 值对象，不能替代 Channel 实体、Binding、Message 或 Delivery。
