# Channel 通信能力长期模型预评估

## Goal

建立 AgentDash 一等 Channel 通信领域与 `ChannelService` 主干设计。Channel 是 Project / Story / LifecycleRun / 外部 IM / Companion / Terminal 等来源共享的通信空间、广播信道、绑定入口、入栈标准化和回复/发布能力面；Mailbox 继续作为某个 AgentRun 消费输入的 durable scheduler。

本任务当前只修订规划与预评估，不启动实现。

## Background

最新对齐结论已经推翻此前 "Companion / SubAgent lifecycle-scoped temporary channel 作为第一版主模型" 的口径。企业内部 IM 接入是明确使用需求：Project 级公共 Channel 需要绑定外部 workspace / room / thread，让用户从更灵活入口访问 Agent，并把该 Channel 作为赋予特定 Agent 的能力。

因此 Channel 不能作为 `LifecycleRun` 的附属字段来建模。`LifecycleRun` 只是 runtime-scoped Channel 的一种宿主环境；通用 Channel 模型必须先表达 Project 公共资产、外部绑定、参与者、广播策略、消息、delivery planning 和 mailbox/outbox materialization。

已有基础仍然有效：

- AgentRun Mailbox 已具备开放的 `MailboxSourceIdentity`，可作为 `ChannelAddress` / delivery attribution 的迁移基础。
- Capability 维度管线支持 `AccumulationPolicy::Accumulate`，可用于把 Channel 可见/可操作投影写入 `CapabilityState.channel`。
- Companion reply contract 已完成模型可见合同收窄，适合作为未来 Channel facade 的局部基础，但不是 Channel 架构最终形态。
- 06-28 两个任务已经分别沉淀 Mailbox source identity 收束与外部 IM / 自定义信道草案；当前任务负责把它们拉回一套一等 ChannelService 边界。

## Requirements

- R1: 定义 Channel 作为一等领域语言，覆盖 Channel、Participant、Binding、Message、Delivery、DeliveryPolicy、ReplyAddress、PublishOutbox、ChannelCapability 和 ChannelService。
- R2: 明确 Channel 与 Mailbox 的边界：Channel 负责广播、参与者、绑定、delivery planning 和 materialization intent；Mailbox 只负责 materialized AgentRun input 的 durable consumption / queue / scheduler。
- R3: 明确 Channel 与 LifecycleGate / PermissionGrant / RuntimeSession / Terminal output 的边界，避免 Channel 接管等待结果、平台决策或运行 trace 事实。
- R4: 明确 `LifecycleRun` 是 Channel owner/scope 的一种，不是 Channel 一等类型来源；不再把 `LifecycleChannel` 作为目标模型。
- R5: 设计 Project 级公共 Channel 与企业 IM 接入语义：外部 workspace/room/thread/user/message identity normalization、ChannelBinding、delivery policy、publish outbox、权限审计和 rate limit。
- R6: 设计 Channel participant / broadcast / delivery 语义，支持 Project / Story / LifecycleRun 多 Agent 协作、角色路由、shared context 和 fan-out。
- R7: 设计 Channel capability：AgentFrame / AgentRun effective capability 暴露 visible channel refs、allowed operations、aliases、readiness、ingress/egress policy；该 projection 从 ChannelService / participant policy 派生，不作为 membership 事实源。
- R8: 设计 Companion 迁移方向：`companion_request` / `companion_respond` 作为 Channel facade；SubAgent 创建作为带 provision 副作用的 Channel target resolution。
- R9: 设计 Terminal / async producer 入栈方向：Terminal 保留 output/state owner，Channel 只承载 bounded event、refs、wake delivery planning。
- R10: 记录被推翻的旧结论和推翻原因，让后续实现不会继续沿用 lifecycle-only temporary channel 模型。

## Acceptance Criteria

- [x] `design.md` 以通用 `agentdash-domain::channel` / `ChannelService` 为主干，不再以 `LifecycleRun.channels: Vec<LifecycleChannel>` 作为最终模型。
- [x] `design.md` 明确 Channel participants / bindings / broadcast policy / message / delivery planning 属于 Channel 事实；`CapabilityState.channel` 只是 Agent 可见操作投影。
- [x] `design.md` 记录 Project / 外部 IM、Lifecycle runtime channel、Companion、Terminal 四类代表链路。
- [x] `design.md` 明确 Mailbox / LifecycleGate / PermissionGrant / RuntimeSession / Terminal output 的事实边界。
- [x] `implement.md` 记录未来分阶段路线：Channel domain/service skeleton -> Project/IM-capable asset model -> Companion/SubAgent runtime channel 迁移 -> mailbox/gate 旧路径清理。
- [x] `research/channel-service-first-principles-realignment.md` 记录本轮推翻原因、企业 IM 驱动、ChannelService 边界、Lifecycle scope 降级和 Capability projection 关系。
- [x] 现有 research 顶部标注 superseded 说明，指出此前 "LifecycleRun.channels 最终决策" 已被新对齐结论推翻。

## Out Of Scope

- 本轮不启动实现、不运行 `task.py start`。
- 本轮不创建数据库 migration。
- 本轮不改生产代码、不迁移 Companion 当前实现。
- 本轮不实现外部 IM provider、身份映射 UI 或 publish outbox。
- 本轮不重写 AgentRun Mailbox scheduler。
- 本轮不改写归档任务；归档的 Companion reply contract 任务保持历史事实。

## Superseded Conclusions

- **推翻：第一版实现范围应窄，优先 Companion/SubAgent lifecycle-scoped temporary channel。**  
  新结论：Companion/SubAgent 可作为第一条验证链路，但文档必须先建立通用 ChannelService 主干，因为企业 IM / Project 公共 Channel 是明确需求。
- **推翻：Channel 的家是共享 LifecycleRun，不新建表，以 `LifecycleRun.channels` 保存。**  
  新结论：`LifecycleRun` 只是 runtime scope；通用 Channel 应支持 Project、Story、LifecycleRun、ExternalBinding、System 等 owner/scope。持久化形态围绕通用 Channel 模型设计。
- **推翻：参与者不用字段或表，由 `CapabilityState.channel.visible_channels` 表达。**  
  新结论：participants、membership、broadcast policy 属于 Channel 事实；`CapabilityState.channel` 是 AgentFrame 可见操作投影。
- **推翻：`ChannelOwner` 收窄为 `LifecycleRun`。**  
  新结论：Project 公共 IM channel 与未来协作 channel 需要保留通用 owner/scope。
- **推翻：Project/Story/外部 IM 的 `ChannelMessage` / `ChannelDelivery` 等到后续再定义。**  
  新结论：可以分阶段落地 event log，但文档必须现在定义 Message / Delivery 的主干边界，否则广播、fan-out、IM ingress/outbox 和 mailbox materialization 无法解释。
- **保留但重解释：`CapabilityState.channel` 作为一等 dimension。**  
  新结论：它是 ChannelService / participant policy 对 AgentFrame 的投影，不是 Channel membership 事实源。
- **保留：`ChannelAddress` 从 `MailboxSourceIdentity` 抽象出来。**  
  新结论：它是 delivery/source attribution 值对象，不能替代 Channel 实体、Binding、Message 或 Delivery。

## Open Questions

- OQ1: Channel 持久化在实现阶段应使用独立 `channels` / `channel_participants` / `channel_bindings` 表，还是先通过 Project asset + runtime registry 混合承载？当前结论只要求不再以 `LifecycleRun.channels` 作为最终事实源。
- OQ2: Project 公共 Channel 的声明式 capability 应挂在 ProjectAgent preset、Project channel assignment，还是两者组合？实现任务需要结合 AgentRun effective capability/admission 细化。
- OQ3: 外部 IM event log 第一阶段是否必须落库，还是只定义 `ChannelMessage` / `ChannelDelivery` 合同并先实现 bounded ingress + mailbox materialization？当前文档只要求合同边界明确。
